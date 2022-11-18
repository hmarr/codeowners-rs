#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub(crate) struct StateId(pub u32);

impl From<StateId> for usize {
    fn from(id: StateId) -> usize {
        id.0 as usize
    }
}

#[derive(Debug, Clone)]
pub(crate) struct State {
    pub(crate) terminal_for_patterns: Vec<usize>,
    pub(crate) transitions: Vec<Transition>,
    pub(crate) epsilon_transition: Option<StateId>,
}

impl State {
    pub(crate) fn new() -> Self {
        Self {
            terminal_for_patterns: Vec::new(),
            transitions: Vec::new(),
            epsilon_transition: None,
        }
    }

    pub(crate) fn is_terminal(&self) -> bool {
        !self.terminal_for_patterns.is_empty()
    }

    pub(crate) fn add_transition(&mut self, transition: Transition) {
        self.transitions.push(transition);
    }

    pub(crate) fn set_terminal_for_pattern(&mut self, pattern_id: usize) {
        self.terminal_for_patterns.push(pattern_id);
    }
}

#[derive(Clone)]
pub(crate) struct Nfa {
    states: Vec<State>,
}

impl Nfa {
    pub(crate) const START_STATE: StateId = StateId(0);

    pub(crate) fn new() -> Self {
        Self {
            states: vec![State::new()],
        }
    }

    pub(crate) fn add_state(&mut self) -> StateId {
        let id = self.states.len();

        let state = State::new();
        self.states.push(state);

        StateId(id as u32)
    }

    pub(crate) fn state(&self, id: StateId) -> &State {
        &self.states[usize::from(id)]
    }

    pub(crate) fn state_mut(&mut self, id: StateId) -> &mut State {
        &mut self.states[usize::from(id)]
    }

    #[cfg(test)]
    pub(crate) fn states_iter(&self) -> std::slice::Iter<'_, State> {
        self.states.iter()
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

#[derive(Debug, Clone)]
pub(crate) struct Transition {
    pub(crate) path_segment: String,
    pub(crate) target: StateId,
    condition: TransitionCondition,
}

impl Transition {
    pub(crate) fn new(path_segment: String, target: StateId) -> Transition {
        let condition = TransitionCondition::new(&path_segment);
        Self {
            path_segment,
            condition,
            target,
        }
    }

    pub(crate) fn is_match(&self, candidate: &str) -> bool {
        self.condition.is_match(&self.path_segment, candidate)
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
