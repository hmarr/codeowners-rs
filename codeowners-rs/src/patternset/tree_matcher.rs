use std::{collections::HashMap, path::Path};

use crate::path_tree::{NodeId, PathTree};

use super::{nfa::Nfa, nfa::StateId};

/// Matches a path against a set of patterns. Includes a thread-safe transition
/// cache to speed up subsequent lookups. Created using a [`super::Builder`].
#[derive(Clone)]
pub struct TreeMatcher {
    nfa: Nfa,
}

impl TreeMatcher {
    pub(crate) fn new(nfa: Nfa) -> TreeMatcher {
        Self { nfa }
    }

    /// Match many paths against the patterns in the set. Returns a map of paths
    /// to a vec of pattern indices that match the path. The pattern indices
    /// match the order in which the patterns were added to the builder.
    pub fn matches_for_paths(&self, paths: &[impl AsRef<Path>]) -> HashMap<String, Vec<usize>> {
        let mut tree = PathTree::new();
        for path in paths {
            tree.insert(path);
        }

        let initial_states = self.nfa.initial_states();
        let mut queue = vec![(initial_states, NodeId(0))];
        let mut matches = HashMap::new();
        while !queue.is_empty() {
            let (states, node_id) = queue.pop().unwrap();
            let node = tree.node(node_id);
            if !node.paths.is_empty() {
                // We've reached a path node. Check if any of the states are
                // accepting.
                for &id in &states {
                    if self.nfa.state(id).is_terminal() {
                        for path in &node.paths {
                            let path_matches = matches.entry(path.to_owned()).or_insert(Vec::new());
                            path_matches
                                .extend(self.nfa.state(id).terminal_for_patterns.iter().copied());
                        }
                    }
                }
            }

            for (segment, child_id) in &node.children {
                let next_states = self.next_states(segment, &states);
                if !next_states.is_empty() {
                    queue.push((next_states, *child_id));
                }
            }
        }
        matches
    }

    // Given a set of states and a slice of path components, return the set of
    // states we're in after stepping through the NFA. This is the core of the
    // matching logic. `next_states` calls itself recursively until the path
    // segment slice is empty.
    fn next_states(&self, segment: &str, from_states: &[StateId]) -> Vec<StateId> {
        // Now that we have the states for the current path's prefix, compute the
        // next states for the current path by following the matching transitions for
        // the current set of states we're in. The `unwrap` won't panic because we
        // checked that the slice isn't empty above.
        let mut next_states = Vec::new();
        for &state_id in from_states {
            self.nfa
                .transitions_from(state_id)
                .filter(|transition| transition.is_match(segment))
                .for_each(|transition| next_states.push(transition.target));
        }

        // Automatically traverse epsilon edges
        let epsilon_nodes = next_states
            .iter()
            .flat_map(|&state_id| self.nfa.epsilon_transitions_from(state_id))
            .collect::<Vec<_>>();
        next_states.extend(epsilon_nodes);
        next_states
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use crate::patternset::Builder;

    use super::*;

    #[test]
    fn test_literals() {
        let patterns = [
            "/src/parser/mod.rs",
            "/lib/parser/parse.rs",
            "/bin/parser/mod.rs",
            "mod.rs",
        ];
        let expected = &[
            ("src/parser/mod.rs", vec![0, 3]),
            ("lib/parser/parse.rs", vec![1]),
            ("lib/parser/mod.rs", vec![3]),
            ("lib/parser/util.rs", vec![]),
            ("src/lexer/mod.rs", vec![3]),
            ("src/parser/mod.go", vec![]),
        ];

        assert_all_matches(expected, &patterns);
    }

    #[test]
    fn test_prefixes() {
        let patterns = ["src", "src/parser", "src/parser/"];
        let expected = &[
            ("src/parser/mod.rs", vec![0, 1, 2]),
            ("src/parser", vec![0, 1]),
            ("foo/src/parser/mod.rs", vec![0]),
        ];

        assert_all_matches(expected, &patterns);
    }

    #[test]
    fn test_anchoring() {
        let patterns = ["/script/foo", "script/foo", "/foo", "foo"];
        let expected = &[
            ("script/foo", vec![0, 1, 3]),
            ("foo", vec![2, 3]),
            ("bar/script/foo", vec![3]),
        ];

        assert_all_matches(expected, &patterns);
    }

    #[test]
    fn test_wildcards() {
        let patterns = [
            "src/*/mod.rs",
            "src/parser/*",
            "*/*/mod.rs",
            "src/parser/*/",
        ];
        let expected = &[
            ("src/parser/mod.rs", vec![0, 1, 2]),
            ("src/lexer/mod.rs", vec![0, 2]),
            ("src/parser/parser.rs", vec![1]),
            ("test/lexer/mod.rs", vec![2]),
            ("parser/mod.rs", vec![]),
            ("src/parser/subdir/thing.rs", vec![3]),
        ];

        assert_all_matches(expected, &patterns);
    }

    #[test]
    fn test_trailing_wildcards() {
        let patterns = ["/mammals/*", "/fish/*/"];
        let expected = &[
            ("mammals", vec![]),
            ("mammals/equus", vec![0]),
            ("mammals/equus/zebra", vec![]),
            ("fish", vec![]),
            ("fish/gaddus", vec![]),
            ("fish/gaddus/cod", vec![1]),
        ];

        assert_all_matches(expected, &patterns);
    }

    #[test]
    fn test_complex_patterns() {
        let patterns = ["/src/parser/*.rs", "/src/p*/*.*"];
        let expected = &[
            ("src/parser/mod.rs", vec![0, 1]),
            ("src/p/lib.go", vec![1]),
            ("src/parser/README", vec![]),
        ];

        assert_all_matches(expected, &patterns);
    }

    #[test]
    fn test_leading_double_stars() {
        let patterns = ["/**/baz", "/**/bar/baz"];
        let expected = &[
            ("x/y/baz", vec![0]),
            ("x/bar/baz", vec![0, 1]),
            ("baz", vec![0]),
        ];

        assert_all_matches(expected, &patterns);
    }

    #[test]
    fn test_infix_double_stars() {
        let patterns = ["/foo/**/qux", "/foo/qux"];
        let expected = &[
            ("foo/qux", vec![0, 1]),
            ("foo/bar/qux", vec![0]),
            ("foo/bar/baz/qux", vec![0]),
            ("foo/bar", vec![]),
            ("bar/qux", vec![]),
        ];

        assert_all_matches(expected, &patterns);
    }

    #[test]
    fn test_trailing_double_stars() {
        let patterns = ["foo/**", "**"];
        let expected = &[
            ("foo", vec![1]),
            ("bar", vec![1]),
            ("foo/bar", vec![0, 1]),
            ("x/y/baz", vec![1]),
            ("foo/bar/baz", vec![0, 1]),
        ];

        assert_all_matches(expected, &patterns);
    }

    #[test]
    fn test_escape_sequences() {
        let patterns = ["f\\*o", "a*b\\??", "\\*qux", "bar\\*", "\\*"];
        let expected = &[
            ("f*o", vec![0]),
            ("foo", vec![]),
            ("axb?!", vec![1]),
            ("axb?", vec![]),
            ("axbc!", vec![]),
            ("*qux", vec![2]),
            ("xqux", vec![]),
            ("bar*", vec![3]),
            ("bar", vec![]),
            ("*", vec![4]),
            ("a", vec![]),
        ];

        assert_all_matches(expected, &patterns);
    }

    fn assert_all_matches(expected: &[(&str, Vec<usize>)], patterns: &[&str]) {
        let paths = expected.iter().map(|(path, _)| path).collect::<Vec<_>>();
        let matcher = matcher_for_patterns(patterns);
        let matches = matcher.matches_for_paths(&paths);
        for (path, expected) in expected {
            assert_eq!(
                HashSet::<&usize>::from_iter(expected.iter()),
                HashSet::from_iter(matches.get(*path).unwrap_or(&vec![]).iter())
            );
        }
    }

    fn matcher_for_patterns(patterns: &[&str]) -> TreeMatcher {
        let mut builder = Builder::new();
        for pattern in patterns {
            builder.add(pattern);
        }
        builder.build_tree_matcher()
    }
}
