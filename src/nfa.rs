use std::{
    collections::{BTreeMap, HashSet},
    path::PathBuf,
};

#[derive(Debug)]
struct State {
    matching_patterns: Vec<usize>,
}

impl State {
    fn new() -> Self {
        Self {
            matching_patterns: Vec::new(),
        }
    }

    fn terminal(&self) -> bool {
        !self.matching_patterns.is_empty()
    }

    fn set_terminal_for_pattern(&mut self, pattern_id: usize) {
        self.matching_patterns.push(pattern_id);
    }
}

pub struct PatternNFA {
    states: Vec<State>,

    // For each state, there is a map of literal path segments to the next state
    literal_edges: Vec<BTreeMap<String, usize>>,

    // For each state, there is either a wildcard edge to the next state, or None
    wildcard_edges: Vec<Option<usize>>,

    // For each state, we map complex segments (those including wildcard characters)
    // to a regex that matches the segment and the next state
    complex_edges: Vec<BTreeMap<String, (regex::Regex, usize)>>,

    // For each state, there is boolean indicating whether there's a self-loop
    self_loop_edges: Vec<bool>,

    next_pattern_id: usize,
}

impl PatternNFA {
    const START_STATE: usize = 0;

    pub fn new() -> Self {
        Self {
            states: vec![State::new()],
            literal_edges: vec![BTreeMap::new()],
            wildcard_edges: vec![None],
            complex_edges: vec![BTreeMap::new()],
            self_loop_edges: vec![false],
            next_pattern_id: 0,
        }
    }

    pub fn add_pattern(&mut self, pattern: &str) -> usize {
        let pattern_id = self.next_pattern_id;
        self.next_pattern_id += 1;

        let pattern = if pattern.starts_with('/') {
            &pattern[1..]
        } else {
            self.add_self_loop(Self::START_STATE);
            pattern
        };

        let pattern = if pattern.ends_with('/') {
            &pattern[..pattern.len() - 1]
        } else {
            pattern
        };

        let end_state_id = pattern
            .split('/')
            .fold(Self::START_STATE, |prev_state_id, segment| {
                self.add_pattern_segment(prev_state_id, segment)
            });

        // Mark the final state as the terminal state for this pattern
        self.state_mut(end_state_id)
            .set_terminal_for_pattern(pattern_id);

        // Patterns are all prefix-matched, which effectively means they all end in
        // a /**, so we need to add a self loop to the final state
        self.add_self_loop(end_state_id);

        pattern_id
    }

    fn add_pattern_segment(&mut self, prev_state_id: usize, segment: &str) -> usize {
        match segment {
            "*" => self.add_wildcard_segment(prev_state_id),
            "**" => self.add_self_loop(prev_state_id),
            _ => {
                if segment.chars().any(|c| c == '*' || c == '?') {
                    self.add_complex_segment(prev_state_id, segment)
                } else {
                    self.add_literal_segment(prev_state_id, segment)
                }
            }
        }
    }

    fn add_literal_segment(&mut self, prev_state_id: usize, segment: &str) -> usize {
        match self.literal_edges[prev_state_id].get(segment) {
            Some(next_state_id) => *next_state_id,
            None => {
                let state_id = self.add_state();
                self.literal_edges[prev_state_id].insert(segment.to_owned(), state_id);
                state_id
            }
        }
    }

    fn add_complex_segment(&mut self, prev_state_id: usize, segment: &str) -> usize {
        match self.complex_edges[prev_state_id].get(segment) {
            Some((_, next_state_id)) => *next_state_id,
            None => {
                let state_id = self.add_state();
                // TODO improve regex generation (escaping first, handling ? characters...)
                let mut segment_pattern = r#"\A"#.to_owned();
                for c in segment.chars() {
                    match c {
                        '*' => segment_pattern.push_str(r#"[^/]*"#),
                        '?' => segment_pattern.push_str(r#"[^/]"#),
                        _ => segment_pattern.push_str(&regex::escape(&c.to_string())), // TODO eugh!
                    }
                }
                segment_pattern.push_str(r#"\z"#);
                let segment_regex = regex::Regex::new(&segment_pattern).unwrap();
                self.complex_edges[prev_state_id]
                    .insert(segment.to_owned(), (segment_regex, state_id));
                state_id
            }
        }
    }

    fn add_wildcard_segment(&mut self, prev_state_id: usize) -> usize {
        match self.wildcard_edges[prev_state_id] {
            Some(next_state_id) => next_state_id,
            None => {
                let state_id = self.add_state();
                self.wildcard_edges[prev_state_id] = Some(state_id);
                state_id
            }
        }
    }

    fn add_self_loop(&mut self, state_id: usize) -> usize {
        self.self_loop_edges[state_id] = true;
        state_id
    }

    pub fn matches(&self, path: impl Into<PathBuf>) -> bool {
        !self.matching_patterns(path).is_empty()
    }

    pub fn matching_patterns(&self, path: impl Into<PathBuf>) -> HashSet<usize> {
        let mut states = vec![Self::START_STATE];
        let mut matches = HashSet::<usize>::new();
        for segment in path.into().iter() {
            let segment = segment.to_str().unwrap();
            let mut next_states = Vec::new();
            for state_id in states {
                if let Some(next_id) = self.literal_edges[state_id].get(segment) {
                    next_states.push(*next_id);
                }

                self.complex_edges[state_id]
                    .values()
                    .filter(|(pat, _)| pat.is_match(segment))
                    .for_each(|(_, next_id)| next_states.push(*next_id));

                if let Some(next_id) = self.wildcard_edges[state_id] {
                    next_states.push(next_id);
                }

                if self.self_loop_edges[state_id] {
                    next_states.push(state_id);
                }
            }
            states = next_states;
        }

        for state_id in states {
            if self.state(state_id).terminal() {
                matches.extend(self.state(state_id).matching_patterns.iter().copied());
            }
        }
        matches
    }

    fn add_state(&mut self) -> usize {
        let id = self.states.len();

        let state = State::new();
        self.states.push(state);
        self.literal_edges.push(BTreeMap::new());
        self.complex_edges.push(BTreeMap::new());
        self.wildcard_edges.push(None);
        self.self_loop_edges.push(false);

        id
    }

    fn state(&self, id: usize) -> &State {
        &self.states[id]
    }

    fn state_mut(&mut self, id: usize) -> &mut State {
        &mut self.states[id]
    }

    fn generate_dot(&self) -> String {
        let mut dot = String::from("digraph G {\n  rankdir=\"LR\"\n");
        for (state_id, state) in self.states.iter().enumerate() {
            if state.terminal() {
                dot.push_str(&format!("  s{} [shape=doublecircle];\n", state_id));
            }
            for (segment, next_state_id) in self.literal_edges[state_id].iter() {
                dot.push_str(&format!(
                    "  s{} -> s{} [label=\"{}\"];\n",
                    state_id, next_state_id, segment
                ));
            }
            for (segment, (_, next_state_id)) in self.complex_edges[state_id].iter() {
                dot.push_str(&format!(
                    "  s{} -> s{} [label=\"{}\"];\n",
                    state_id, next_state_id, segment
                ));
            }
            if let Some(next_state_id) = self.wildcard_edges[state_id] {
                dot.push_str(&format!(
                    "  s{} -> s{} [label=\"*\"];\n",
                    state_id, next_state_id
                ));
            }
            if self.self_loop_edges[state_id] {
                dot.push_str(&format!(
                    "  s{} -> s{} [label=\"*\"];\n",
                    state_id, state_id
                ));
            }
        }
        dot.push_str("}\n");
        dot
    }
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
        assert!(!nfa.matches("lib/parser/util.rs"));
        assert_eq!(
            nfa.matching_patterns("src/lexer/mod.rs"),
            HashSet::from([patterns[3]])
        );
        assert!(!nfa.matches("src/parser/mod.go"));
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
        println!("{}", nfa.generate_dot());

        // BUG!
        // if we start with /script, we'll generate an anchored node
        // if we then add a pattern that begins with script, we'll unanchor the node

        assert_eq!(
            nfa.matching_patterns("script/foo"),
            HashSet::from([patterns[0], patterns[1]])
        );
        assert_eq!(
            nfa.matching_patterns("/script/foo"),
            HashSet::from([patterns[0], patterns[1]])
        );

        assert_eq!(
            nfa.matching_patterns("/bar/script/foo"),
            HashSet::from([patterns[1]])
        );
    }

    #[test]
    fn test_double_star_bug() {
        let mut nfa = PatternNFA::new();
        let patterns = [nfa.add_pattern("/foo/**/bar"), nfa.add_pattern("/foo/bar")];
        println!("{}", nfa.generate_dot());

        assert_eq!(
            nfa.matching_patterns("foo/bar"),
            HashSet::from([patterns[0], patterns[1]])
        );
        assert_eq!(
            nfa.matching_patterns("foo/baz/bar"),
            HashSet::from([patterns[0]])
        );
    }

    #[test]
    fn test_wildcard_matches() {
        let mut nfa = PatternNFA::new();
        let patterns = [
            nfa.add_pattern("src/*/mod.rs"),
            nfa.add_pattern("src/parser/*"),
            nfa.add_pattern("*/*/mod.rs"),
        ];

        println!("{}", nfa.generate_dot());

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
        assert_eq!(nfa.matching_patterns("parser/mod.rs"), HashSet::new())
    }

    #[test]
    fn test_complex_matches() {
        let mut nfa = PatternNFA::new();
        let patterns = [
            nfa.add_pattern("/src/parser/*.rs"),
            nfa.add_pattern("/src/p*/*.*"),
        ];

        println!("{}", nfa.generate_dot());

        assert_eq!(
            nfa.matching_patterns("src/parser/mod.rs"),
            HashSet::from([patterns[0], patterns[1]])
        );
        assert_eq!(
            nfa.matching_patterns("src/p/lib.go"),
            HashSet::from([patterns[1]])
        );
        assert!(!nfa.matches("src/parser/README"));
    }

    #[test]
    fn test_leading_double_star_matches() {
        let mut nfa = PatternNFA::new();
        let patterns = [nfa.add_pattern("/**/baz"), nfa.add_pattern("/**/bar/baz")];

        println!("{}", nfa.generate_dot());

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
        nfa.add_pattern("/foo/**/qux");

        assert!(nfa.matches("foo/qux"));
        assert!(nfa.matches("foo/bar/qux"));
        assert!(nfa.matches("foo/bar/baz/qux"));
        assert!(!nfa.matches("foo/bar"));
        assert!(!nfa.matches("bar/qux"));
    }

    #[test]
    fn test_trailing_double_star_matches() {
        let mut nfa = PatternNFA::new();
        let patterns = [nfa.add_pattern("foo/**"), nfa.add_pattern("**")];

        println!("{}", nfa.generate_dot());

        assert_eq!(
            nfa.matching_patterns("x/y/baz"),
            HashSet::from([patterns[1]])
        );
        assert_eq!(
            nfa.matching_patterns("foo/bar/baz"),
            HashSet::from([patterns[0], patterns[1]])
        );
    }
}
