use std::{
    collections::{HashMap, HashSet},
    path::Path,
    sync::{Arc, RwLock},
};

use memchr::memmem;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
struct StateId(u32);

impl From<StateId> for usize {
    fn from(id: StateId) -> usize {
        id.0 as usize
    }
}

#[derive(Debug, Clone)]
struct State {
    matching_patterns: Vec<usize>,
    transitions: Vec<Transition>,
    epsilon_transition: Option<StateId>,
}

impl State {
    fn new() -> Self {
        Self {
            matching_patterns: Vec::new(),
            transitions: Vec::new(),
            epsilon_transition: None,
        }
    }

    fn add_transition(&mut self, transition: Transition) {
        self.transitions.push(transition);
    }

    fn terminal(&self) -> bool {
        !self.matching_patterns.is_empty()
    }

    fn set_terminal_for_pattern(&mut self, pattern_id: usize) {
        self.matching_patterns.push(pattern_id);
    }
}

#[derive(Debug, Clone)]
struct Transition {
    path_segment: String,
    matcher: TransitionCondition,
    target: StateId,
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

    fn is_match(&self, candidate: &str) -> bool {
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
                memmem::find(candidate.as_bytes(), pattern.trim_matches('*').as_bytes()).is_some()
            }
            Self::Regex(re) => re.is_match(candidate),
        }
    }
}

fn pattern_to_regex(pattern: &str) -> regex::Regex {
    let mut regex = r#"\A"#.to_owned();
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

#[derive(Clone)]
pub struct PatternNFA {
    states: Vec<State>,
    transition_cache: Arc<RwLock<HashMap<String, Vec<StateId>>>>,
    next_pattern_id: usize,
}

impl PatternNFA {
    const START_STATE: StateId = StateId(0);

    pub fn new() -> Self {
        Self {
            states: vec![State::new()],
            transition_cache: Arc::new(RwLock::new(HashMap::new())),
            next_pattern_id: 0,
        }
    }

    pub fn add_pattern(&mut self, pattern: &str) -> usize {
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

        // We (currently) only match files (as opposed to directories), so the trailing slash
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
            .transitions(prev_state_id)
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
            .transitions(prev_state_id)
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

    pub fn is_match(&self, path: impl AsRef<Path>) -> bool {
        !self.matching_patterns(path).is_empty()
    }

    pub fn matching_patterns(&self, path: impl AsRef<Path>) -> HashSet<usize> {
        let components = path.as_ref().iter().map(|c| c.to_str().unwrap());
        let final_states = self.step(&components.collect::<Vec<_>>(), self.initial_states());

        let mut matches = HashSet::new();
        for state_id in final_states {
            if self.state(state_id).terminal() {
                matches.extend(self.state(state_id).matching_patterns.iter().copied());
            }
        }
        matches
    }

    fn step(&self, path_segments: &[&str], start_states: Vec<StateId>) -> Vec<StateId> {
        let states = if !path_segments.is_empty() {
            let subpath_segments = &path_segments[..path_segments.len() - 1];
            let subpath = subpath_segments.join("/");
            let cached_state = self
                .transition_cache
                .read()
                .expect("valid lock")
                .get(&subpath)
                .cloned();
            if let Some(states) = cached_state {
                states
            } else {
                let states = self.step(subpath_segments, start_states);
                self.transition_cache
                    .write()
                    .expect("valid lock")
                    .insert(subpath, states.clone());
                states
            }
        } else {
            return start_states;
        };

        let segment = *path_segments.last().unwrap();
        let mut next_states = Vec::new();
        for state_id in states {
            self.state(state_id)
                .transitions
                .iter()
                .filter(|transition| transition.is_match(segment))
                .for_each(|transition| next_states.push(transition.target));
        }

        // Automatically traverse epsilon edges
        let epsilon_nodes = next_states
            .iter()
            .flat_map(|&state_id| &self.state(state_id).epsilon_transition)
            .collect::<Vec<_>>();
        next_states.extend(epsilon_nodes);
        next_states
    }

    fn add_state(&mut self) -> StateId {
        let id = self.states.len();

        let state = State::new();
        self.states.push(state);

        StateId(id as u32)
    }

    #[inline]
    fn state(&self, id: StateId) -> &State {
        &self.states[usize::from(id)]
    }

    #[inline]
    fn state_mut(&mut self, id: StateId) -> &mut State {
        &mut self.states[usize::from(id)]
    }

    fn initial_states(&self) -> Vec<StateId> {
        let mut states = vec![Self::START_STATE];
        if let Some(epsilon_node_id) = self.state(Self::START_STATE).epsilon_transition {
            states.push(epsilon_node_id);
        }
        states
    }

    fn transitions(&self, state_id: StateId) -> impl Iterator<Item = &Transition> {
        self.state(state_id).transitions.iter()
    }
}

fn has_wildcard(mut char_iter: impl Iterator<Item = char>) -> bool {
    char_iter.any(|c| c == '*' || c == '?')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_literal_matches() {
        let mut nfa = PatternNFA::new();
        let patterns = [
            nfa.add_pattern("/src/parser/mod.rs"),
            nfa.add_pattern("/lib/parser/parse.rs"),
            nfa.add_pattern("/bin/parser/mod.rs"),
            nfa.add_pattern("mod.rs"),
        ];

        assert_eq!(
            nfa.matching_patterns("src/parser/mod.rs"),
            HashSet::from([patterns[0], patterns[3]])
        );
        assert_eq!(
            nfa.matching_patterns("lib/parser/parse.rs"),
            HashSet::from([patterns[1]])
        );
        assert_eq!(
            nfa.matching_patterns("lib/parser/mod.rs"),
            HashSet::from([patterns[3]])
        );
        assert!(!nfa.is_match("lib/parser/util.rs"));
        assert_eq!(
            nfa.matching_patterns("src/lexer/mod.rs"),
            HashSet::from([patterns[3]])
        );
        assert!(!nfa.is_match("src/parser/mod.go"));
    }

    #[test]
    fn test_prefix_matches() {
        let mut nfa = PatternNFA::new();
        let patterns = [
            nfa.add_pattern("src"),
            nfa.add_pattern("src/parser"),
            nfa.add_pattern("src/parser/"),
        ];

        assert_eq!(
            nfa.matching_patterns("src/parser/mod.rs"),
            HashSet::from([patterns[0], patterns[1], patterns[2]])
        );
    }

    #[test]
    fn test_anchoring_matches() {
        let mut nfa = PatternNFA::new();
        let patterns = [
            nfa.add_pattern("/script/foo"),
            nfa.add_pattern("script/foo"),
        ];

        assert_eq!(
            nfa.matching_patterns("script/foo"),
            HashSet::from([patterns[0], patterns[1]])
        );
        assert_eq!(
            nfa.matching_patterns("bar/script/foo"),
            HashSet::from([patterns[1]])
        );
    }

    #[test]
    fn test_wildcard_matches() {
        let mut nfa = PatternNFA::new();
        let patterns = [
            nfa.add_pattern("src/*/mod.rs"),
            nfa.add_pattern("src/parser/*"),
            nfa.add_pattern("*/*/mod.rs"),
            nfa.add_pattern("src/parser/*/"),
        ];

        assert_eq!(
            nfa.matching_patterns("src/parser/mod.rs"),
            HashSet::from([patterns[0], patterns[1], patterns[2]])
        );
        assert_eq!(
            nfa.matching_patterns("src/lexer/mod.rs"),
            HashSet::from([patterns[0], patterns[2]])
        );
        assert_eq!(
            nfa.matching_patterns("src/parser/parser.rs"),
            HashSet::from([patterns[1]])
        );
        assert_eq!(
            nfa.matching_patterns("test/lexer/mod.rs"),
            HashSet::from([patterns[2]])
        );
        assert_eq!(nfa.matching_patterns("parser/mod.rs"), HashSet::new());
        assert_eq!(
            nfa.matching_patterns("src/parser/subdir/thing.rs"),
            HashSet::from([patterns[3]])
        );
    }

    #[test]
    fn test_trailing_wildcards() {
        let mut nfa = PatternNFA::new();
        nfa.add_pattern("/mammals/*");
        nfa.add_pattern("/fish/*/");

        assert!(!nfa.is_match("mammals"));
        assert!(nfa.is_match("mammals/equus"));
        assert!(!nfa.is_match("mammals/equus/zebra"));

        assert!(!nfa.is_match("fish"));
        assert!(!nfa.is_match("fish/gaddus"));
        assert!(nfa.is_match("fish/gaddus/cod"));
    }

    #[test]
    fn test_complex_matches() {
        let mut nfa = PatternNFA::new();
        let patterns = [
            nfa.add_pattern("/src/parser/*.rs"),
            nfa.add_pattern("/src/p*/*.*"),
        ];

        assert_eq!(
            nfa.matching_patterns("src/parser/mod.rs"),
            HashSet::from([patterns[0], patterns[1]])
        );
        assert_eq!(
            nfa.matching_patterns("src/p/lib.go"),
            HashSet::from([patterns[1]])
        );
        assert!(!nfa.is_match("src/parser/README"));
    }

    #[test]
    fn test_leading_double_star_matches() {
        let mut nfa = PatternNFA::new();
        let patterns = [nfa.add_pattern("/**/baz"), nfa.add_pattern("/**/bar/baz")];

        assert_eq!(
            nfa.matching_patterns("x/y/baz"),
            HashSet::from([patterns[0]])
        );
        assert_eq!(
            nfa.matching_patterns("x/bar/baz"),
            HashSet::from([patterns[0], patterns[1]])
        );
        assert_eq!(nfa.matching_patterns("baz"), HashSet::from([patterns[0]]));
    }

    #[test]
    fn test_infix_double_star_matches() {
        let mut nfa = PatternNFA::new();
        let patterns = [nfa.add_pattern("/foo/**/qux"), nfa.add_pattern("/foo/qux")];

        assert_eq!(
            nfa.matching_patterns("foo/qux"),
            HashSet::from([patterns[0], patterns[1]])
        );
        assert_eq!(
            nfa.matching_patterns("foo/bar/qux"),
            HashSet::from([patterns[0]])
        );
        assert_eq!(
            nfa.matching_patterns("foo/bar/baz/qux"),
            HashSet::from([patterns[0]])
        );
        assert!(!nfa.is_match("foo/bar"));
        assert!(!nfa.is_match("bar/qux"));
    }

    #[test]
    fn test_trailing_double_star_matches() {
        let mut nfa = PatternNFA::new();
        let patterns = [nfa.add_pattern("foo/**"), nfa.add_pattern("**")];

        assert_eq!(nfa.matching_patterns("bar"), HashSet::from([patterns[1]]));
        assert_eq!(
            nfa.matching_patterns("x/y/baz"),
            HashSet::from([patterns[1]])
        );
        assert_eq!(
            nfa.matching_patterns("foo/bar/baz"),
            HashSet::from([patterns[0], patterns[1]])
        );
    }

    #[allow(dead_code)]
    fn generate_dot(nfa: &PatternNFA) -> String {
        let mut dot = String::from("digraph G {\n  rankdir=\"LR\"\n");
        for (state_id, state) in nfa.states.iter().enumerate() {
            if state.terminal() {
                dot.push_str(&format!("  s{} [shape=doublecircle];\n", state_id));
            }
            for transition in state.transitions.iter() {
                dot.push_str(&format!(
                    "  s{} -> s{} [label=\"{}\"];\n",
                    state_id, transition.target.0, transition.path_segment
                ));
            }
            if let Some(next_state_id) = nfa.states[state_id].epsilon_transition {
                dot.push_str(&format!(
                    "  s{} -> s{} [label=\"ε\"];\n",
                    state_id, next_state_id.0
                ));
            }
        }
        dot.push_str("}\n");
        dot
    }
}
