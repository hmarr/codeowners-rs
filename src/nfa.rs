use std::{
    collections::{BTreeMap, HashMap, HashSet},
    path::PathBuf,
    sync::{Arc, RwLock},
};

use memchr::memmem;

#[derive(Debug, Clone)]
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

#[derive(Debug, Clone)]
enum GlobTransition {
    Regex(regex::Regex),
    Prefix(String),
    Suffix(String),
    Contains(String),
}

impl GlobTransition {
    fn new(glob: &str) -> Self {
        let mut chars = glob.chars();
        let leading_star = chars.next().map(|c| c == '*').unwrap_or(false);
        let trailing_star = chars.next_back().map(|c| c == '*').unwrap_or(false);
        let internal_wildcards = has_wildcard(chars);

        match (leading_star, trailing_star, internal_wildcards) {
            (false, true, false) => Self::Prefix(glob.trim_end_matches('*').to_string()),
            (true, false, false) => Self::Suffix(glob.trim_start_matches('*').to_string()),
            (true, true, false) => Self::Contains(glob.trim_matches('*').to_string()),
            _ => Self::Regex(pattern_to_regex(glob)),
        }
    }

    fn is_match(&self, s: &str) -> bool {
        match self {
            Self::Regex(re) => re.is_match(s),
            Self::Prefix(prefix) => s.starts_with(prefix),
            Self::Suffix(suffix) => s.ends_with(suffix),
            Self::Contains(substr) => memmem::find(s.as_bytes(), substr.as_bytes()).is_some(),
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

    // For each state, there is a map of literal path segments to the next state
    literal_edges: Vec<BTreeMap<String, usize>>,

    // For each state, there is either a wildcard edge to the next state, or None
    wildcard_edges: Vec<Option<usize>>,

    // For each state, we map complex segments (those including wildcard characters)
    // to a regex that matches the segment and the next state
    complex_edges: Vec<BTreeMap<String, (GlobTransition, usize)>>,

    // For each state, there is optionally an epsilon edge – that is, an edge that we
    // automatically traverse without consuming any input
    double_star_edges: Vec<Option<usize>>,

    // For each state, there is boolean indicating whether there's a self-loop
    self_loop_edges: Vec<bool>,

    transition_cache: Arc<RwLock<HashMap<String, Vec<usize>>>>,

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
            double_star_edges: vec![None],
            self_loop_edges: vec![false],
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
                start_state_id = self.add_double_star_segment(Self::START_STATE);
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
            .fold(start_state_id, |prev_state_id, segment| {
                self.add_pattern_segment(prev_state_id, segment)
            });

        // If the pattern ends with a trailing slash, we match everything under the
        // directory, but not the directory itself, so we need one more segment
        if trailing_slash {
            end_state_id = self.add_wildcard_segment(end_state_id);
        }

        // Most patterns are all prefix-matched, which effectively means they end in
        // a /**, so we need to add a self loop to the final state. The exception is
        // patterns that end with a single wildcard, which we handle separately, which
        // don't match recursively. This appears to be a discrepancy between the
        // CODEOWNERS globbing rules and the .gitignore rules.
        if let Some(&last_segment) = segments.last() {
            if last_segment != "*" {
                end_state_id = self.add_double_star_segment(end_state_id);
            }
        }

        // Mark the final state as the terminal state for this pattern
        self.state_mut(end_state_id)
            .set_terminal_for_pattern(pattern_id);

        pattern_id
    }

    fn add_pattern_segment(&mut self, prev_state_id: usize, segment: &str) -> usize {
        match segment {
            "*" => self.add_wildcard_segment(prev_state_id),
            "**" => self.add_double_star_segment(prev_state_id),
            _ => {
                if has_wildcard(segment.chars()) {
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
                let transition = GlobTransition::new(segment);
                self.complex_edges[prev_state_id]
                    .insert(segment.to_owned(), (transition, state_id));
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

    fn add_double_star_segment(&mut self, prev_state_id: usize) -> usize {
        // Double star segments match zero or more of anything, so there's never a need to
        // have multiple consecutive double star states. Multiple consecutive double star
        // states mean we require multiple path segments, which violoates the gitignore spec
        if self.self_loop_edges[prev_state_id] {
            return prev_state_id;
        }

        match self.double_star_edges[prev_state_id] {
            Some(next_state_id) => next_state_id,
            None => {
                let state_id = self.add_state();
                self.self_loop_edges[state_id] = true;
                self.double_star_edges[prev_state_id] = Some(state_id);
                state_id
            }
        }
    }

    pub fn matches(&self, path: impl Into<PathBuf>) -> bool {
        !self.matching_patterns(path).is_empty()
    }

    pub fn matching_patterns(&self, path: impl Into<PathBuf>) -> HashSet<usize> {
        let mut states = vec![Self::START_STATE];
        if let Some(epsilon_node_id) = self.double_star_edges[Self::START_STATE] {
            states.push(epsilon_node_id);
        }

        states = self.step(
            &path
                .into()
                .iter()
                .map(|c| c.to_str().unwrap())
                .collect::<Vec<_>>(),
            states,
        );

        let mut matches = HashSet::new();
        for state_id in states {
            if self.state(state_id).terminal() {
                matches.extend(self.state(state_id).matching_patterns.iter().copied());
            }
        }
        matches
    }

    pub fn step(&self, path_segments: &[&str], start_states: Vec<usize>) -> Vec<usize> {
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
            if let Some(next_id) = self.literal_edges[state_id].get(segment) {
                next_states.push(*next_id);
            }

            self.complex_edges[state_id]
                .values()
                .filter(|(transition, _)| transition.is_match(segment))
                .for_each(|(_, next_id)| next_states.push(*next_id));

            if let Some(next_id) = self.wildcard_edges[state_id] {
                next_states.push(next_id);
            }

            if self.self_loop_edges[state_id] {
                next_states.push(state_id);
            }
        }

        // Automatically traverse epsilon edges
        let epsilon_nodes = next_states
            .iter()
            .flat_map(|state_id| &self.double_star_edges[*state_id])
            .collect::<Vec<_>>();
        next_states.extend(epsilon_nodes);
        next_states
    }

    fn add_state(&mut self) -> usize {
        let id = self.states.len();

        let state = State::new();
        self.states.push(state);
        self.literal_edges.push(BTreeMap::new());
        self.complex_edges.push(BTreeMap::new());
        self.wildcard_edges.push(None);
        self.double_star_edges.push(None);
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
            if let Some(next_state_id) = self.double_star_edges[state_id] {
                dot.push_str(&format!(
                    "  s{} -> s{} [label=\"ε\"];\n",
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
            nfa.matching_patterns("bar/script/foo"),
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
            nfa.add_pattern("src/parser/*/"),
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

        println!("{}", nfa.generate_dot());

        assert!(!nfa.matches("mammals"));
        assert!(nfa.matches("mammals/equus"));
        assert!(!nfa.matches("mammals/equus/zebra"));

        assert!(!nfa.matches("fish"));
        assert!(!nfa.matches("fish/gaddus"));
        assert!(nfa.matches("fish/gaddus/cod"));
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
}
