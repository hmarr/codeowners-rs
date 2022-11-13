#[derive(Clone)]
pub struct Nfa {
    states: Vec<State>,
    next_pattern_id: usize,
}

impl Nfa {
    const START_STATE: StateId = StateId(0);

    pub fn new() -> Self {
        Self {
            states: vec![State::new()],
            next_pattern_id: 0,
        }
    }

    pub fn add(&mut self, pattern: &str) -> usize {
        let pattern_id = self.next_pattern_id;
        self.next_pattern_id += 1;

        let mut start_state_id = Self::START_STATE;

        let pattern = match pattern.strip_prefix('/') {
            Some(pattern) => pattern,
            None => {
                start_state_id = self.add_epsilon_transition(Self::START_STATE);
                pattern
            }
        };

        // We currently only match files (as opposed to directories), so the trailing slash
        // has no effect except adding an extra empty path component at the end
        let (pattern, trailing_slash) = match pattern.strip_suffix('/') {
            Some(pattern) => (pattern, true),
            None => (pattern, false),
        };

        let segments = pattern.split('/').collect::<Vec<_>>();
        let mut end_state_id = segments
            .iter()
            .fold(start_state_id, |prev_state_id, segment| match *segment {
                "**" => self.add_epsilon_transition(prev_state_id),
                _ => self.add_transition(prev_state_id, segment),
            });

        // If the pattern ends with a trailing slash, we match everything under the
        // directory, but not the directory itself, so we need one more segment
        if trailing_slash {
            end_state_id = self.add_transition(end_state_id, "*");
        }

        // Most patterns are all prefix-matched, which effectively means they end in
        // a /**, so we need to add a self loop to the final state. The exception is
        // patterns that end with a single wildcard, which we handle separately, which
        // don't match recursively. This appears to be a discrepancy between the
        // CODEOWNERS globbing rules and the .gitignore rules.
        if let Some(&last_segment) = segments.last() {
            if last_segment != "*" {
                end_state_id = self.add_epsilon_transition(end_state_id);
            }
        }

        // Mark the final state as the terminal state for this pattern
        self.state_mut(end_state_id)
            .set_terminal_for_pattern(pattern_id);

        pattern_id
    }

    fn add_transition(&mut self, prev_state_id: StateId, segment: &str) -> StateId {
        let existing_transition = self
            .transitions_from(prev_state_id)
            .find(|t| t.path_segment == segment && t.target != prev_state_id);
        if let Some(t) = existing_transition {
            t.target
        } else {
            let state_id = self.add_state();
            self.state_mut(prev_state_id)
                .add_transition(Transition::new(segment.to_owned(), state_id));
            state_id
        }
    }

    fn add_epsilon_transition(&mut self, prev_state_id: StateId) -> StateId {
        // Double star segments match zero or more of anything, so there's never a need to
        // have multiple consecutive double star states. Multiple consecutive double star
        // states mean we require multiple path segments, which violoates the gitignore spec
        let has_existing_transition = self
            .transitions_from(prev_state_id)
            .any(|t| t.path_segment == "*" && t.target == prev_state_id);
        if has_existing_transition {
            return prev_state_id;
        }

        match self.state(prev_state_id).epsilon_transition {
            Some(next_state_id) => next_state_id,
            None => {
                let state_id = self.add_state();
                self.state_mut(state_id)
                    .add_transition(Transition::new("*".to_owned(), state_id));
                self.state_mut(prev_state_id).epsilon_transition = Some(state_id);
                state_id
            }
        }
    }

    fn add_state(&mut self) -> StateId {
        let id = self.states.len();

        let state = State::new();
        self.states.push(state);

        StateId(id as u32)
    }

    #[inline]
    pub(crate) fn state(&self, id: StateId) -> &State {
        &self.states[usize::from(id)]
    }

    #[inline]
    fn state_mut(&mut self, id: StateId) -> &mut State {
        &mut self.states[usize::from(id)]
    }

    pub(crate) fn initial_states(&self) -> Vec<StateId> {
        let mut states = vec![Self::START_STATE];
        if let Some(epsilon_node_id) = self.state(Self::START_STATE).epsilon_transition {
            states.push(epsilon_node_id);
        }
        states
    }

    pub(crate) fn transitions_from(&self, state_id: StateId) -> impl Iterator<Item = &Transition> {
        self.state(state_id).transitions.iter()
    }

    pub(crate) fn epsilon_transitions_from(&self, state_id: StateId) -> Option<StateId> {
        self.state(state_id).epsilon_transition
    }
}

impl Default for Nfa {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub(crate) struct StateId(u32);

impl From<StateId> for usize {
    fn from(id: StateId) -> usize {
        id.0 as usize
    }
}

#[derive(Debug, Clone)]
pub(crate) struct State {
    pub(crate) terminal_for_patterns: Vec<usize>,
    transitions: Vec<Transition>,
    epsilon_transition: Option<StateId>,
}

impl State {
    fn new() -> Self {
        Self {
            terminal_for_patterns: Vec::new(),
            transitions: Vec::new(),
            epsilon_transition: None,
        }
    }

    pub(crate) fn is_terminal(&self) -> bool {
        !self.terminal_for_patterns.is_empty()
    }

    fn add_transition(&mut self, transition: Transition) {
        self.transitions.push(transition);
    }

    fn set_terminal_for_pattern(&mut self, pattern_id: usize) {
        self.terminal_for_patterns.push(pattern_id);
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Transition {
    pub(crate) path_segment: String,
    matcher: TransitionCondition,
    pub(crate) target: StateId,
}

impl Transition {
    fn new(path_segment: String, target: StateId) -> Transition {
        let matcher = TransitionCondition::new(&path_segment);
        Self {
            path_segment,
            matcher,
            target,
        }
    }

    pub(crate) fn is_match(&self, candidate: &str) -> bool {
        self.matcher.is_match(&self.path_segment, candidate)
    }
}

#[derive(Debug, Clone)]
enum TransitionCondition {
    Unconditional,
    Literal,
    Prefix,
    Suffix,
    Contains,
    Regex(regex::Regex),
}

impl TransitionCondition {
    fn new(glob: &str) -> Self {
        if glob == "*" {
            return Self::Unconditional;
        }

        let mut chars = glob.chars();
        let leading_star = chars.next().map(|c| c == '*').unwrap_or(false);
        let trailing_star = chars.next_back().map(|c| c == '*').unwrap_or(false);
        let internal_wildcards = has_wildcard(chars);

        match (leading_star, trailing_star, internal_wildcards) {
            (false, false, false) => Self::Literal,
            (false, true, false) => Self::Prefix,
            (true, false, false) => Self::Suffix,
            (true, true, false) => Self::Contains,
            _ => Self::Regex(pattern_to_regex(glob)),
        }
    }

    fn is_match(&self, pattern: &str, candidate: &str) -> bool {
        match self {
            Self::Unconditional => true,
            Self::Literal => pattern == candidate,
            Self::Prefix => candidate.starts_with(pattern.trim_end_matches('*')),
            Self::Suffix => candidate.ends_with(pattern.trim_start_matches('*')),
            Self::Contains => {
                memchr::memmem::find(candidate.as_bytes(), pattern.trim_matches('*').as_bytes())
                    .is_some()
            }
            Self::Regex(re) => re.is_match(candidate),
        }
    }
}

fn pattern_to_regex(pattern: &str) -> regex::Regex {
    let mut regex = String::with_capacity(pattern.len() + 8);
    regex.push_str(r#"\A"#);
    for c in pattern.chars() {
        match c {
            '*' => regex.push_str(r#"[^/]*"#),
            '?' => regex.push_str(r#"[^/]"#),
            _ => {
                if regex_syntax::is_meta_character(c) {
                    regex.push('\\');
                }
                regex.push(c);
            }
        }
    }
    regex.push_str(r#"\z"#);
    regex::Regex::new(&regex).unwrap_or_else(|_| panic!("invalid regex: {}", regex))
}

fn has_wildcard(mut char_iter: impl Iterator<Item = char>) -> bool {
    char_iter.any(|c| c == '*' || c == '?')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nfa_generation() {
        let mut mg = Nfa::new();

        mg.add("/foo/*");
        assert_eq!(
            transitions_for(&mg),
            vec![(0, "foo".to_owned(), 1), (1, "*".to_owned(), 2)]
        );

        mg.add("/foo/bar");
        assert_eq!(
            transitions_for(&mg),
            vec![
                (0, "foo".to_owned(), 1),
                (1, "*".to_owned(), 2),
                (1, "bar".to_owned(), 3),
                (4, "*".to_owned(), 4)
            ]
        );
    }

    #[allow(dead_code)]
    fn generate_dot(nfa: &Nfa) -> String {
        let mut dot = String::from("digraph G {\n  rankdir=\"LR\"\n");
        for (state_id, state) in nfa.states.iter().enumerate() {
            if state.is_terminal() {
                dot.push_str(&format!("  s{} [shape=doublecircle];\n", state_id));
            }
            for transition in state.transitions.iter() {
                dot.push_str(&format!(
                    "  s{} -> s{} [label=\"{}\"];\n",
                    state_id, transition.target.0, transition.path_segment
                ));
            }
            if let Some(next_state_id) = nfa.state(StateId(state_id as u32)).epsilon_transition {
                dot.push_str(&format!(
                    "  s{} -> s{} [label=\"ε\"];\n",
                    state_id, next_state_id.0
                ));
            }
        }
        dot.push_str("}\n");
        dot
    }

    fn transitions_for(nfa: &Nfa) -> Vec<(usize, String, usize)> {
        nfa.states
            .iter()
            .enumerate()
            .flat_map(|(idx, s)| {
                s.transitions
                    .iter()
                    .map(|t| (idx, t.path_segment.clone(), t.target.0 as usize))
                    .collect::<Vec<_>>()
            })
            .collect()
    }
}
