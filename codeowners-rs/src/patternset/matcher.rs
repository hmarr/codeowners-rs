use std::{
    borrow::Cow,
    collections::HashMap,
    path::Path,
    sync::{Arc, RwLock},
};

use super::{nfa::Nfa, nfa::StateId};

/// Matches a path against a set of patterns. Includes a thread-safe transition
/// cache to speed up subsequent lookups. Created using a [`super::Builder`].
#[derive(Clone)]
pub struct Matcher {
    nfa: Nfa,
    transition_cache: Arc<RwLock<HashMap<String, Vec<StateId>>>>,
}

impl Matcher {
    pub(crate) fn new(nfa: Nfa) -> Matcher {
        Self {
            nfa,
            transition_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Match a path against the patterns in the set. Returns a list of pattern
    /// indices that match the path. The pattern indices match the order in which
    /// the patterns were added to the builder.
    pub fn matching_patterns(&self, path: impl AsRef<Path>) -> Vec<usize> {
        let components = path
            .as_ref()
            .iter()
            .map(|c| c.to_string_lossy())
            .collect::<Vec<_>>();
        let initial_states = self.nfa.initial_states();
        let final_states = self.next_states(&components, initial_states);

        let mut matches = Vec::new();
        for state_id in final_states {
            // After processing the path, find the states we're in that are
            // terminal, and return the pattern ids for those states.
            if self.nfa.state(state_id).is_terminal() {
                matches.extend(
                    self.nfa
                        .state(state_id)
                        .terminal_for_patterns
                        .iter()
                        .copied(),
                );
            }
        }
        matches
    }

    // Given a set of states and a slice of path components, return the set of
    // states we're in after stepping through the NFA. This is the core of the
    // matching logic. `next_states` calls itself recursively until the path
    // segment slice is empty.
    fn next_states(&self, path_segments: &[Cow<str>], start_states: Vec<StateId>) -> Vec<StateId> {
        // Base case - no more path segments to match
        if path_segments.is_empty() {
            return start_states;
        }

        // Get the states for the current path's prefix
        let subpath_segments = &path_segments[..path_segments.len() - 1];
        let subpath = subpath_segments.join("/");

        // Start by checking the cache
        let cached_states = self.get_cached_states_for(&subpath);
        let states = if let Some(states) = cached_states {
            states
        } else {
            // If the cache doesn't have the states, recursively compute them
            let states = self.next_states(subpath_segments, start_states);
            self.set_cached_states_for(subpath, states.clone());
            states
        };

        // Now that we have the states for the current path's prefix, compute the
        // next states for the current path by following the matching transitions for
        // the current set of states we're in. The `unwrap` won't panic because we
        // checked that the slice isn't empty above.
        let segment = path_segments.last().unwrap();
        let mut next_states = Vec::new();
        for state_id in states {
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

    fn get_cached_states_for(&self, path: &str) -> Option<Vec<StateId>> {
        self.transition_cache
            .read()
            .expect("valid lock")
            .get(path)
            .cloned()
    }

    fn set_cached_states_for(&self, path: String, states: Vec<StateId>) {
        self.transition_cache
            .write()
            .expect("valid lock")
            .insert(path, states);
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
        let matcher = matcher_for_patterns(&patterns);

        assert_matches(&matcher, "src/parser/mod.rs", &patterns, &[0, 3]);
        assert_matches(&matcher, "lib/parser/parse.rs", &patterns, &[1]);
        assert_matches(&matcher, "lib/parser/mod.rs", &patterns, &[3]);
        assert_matches(&matcher, "lib/parser/util.rs", &patterns, &[]);
        assert_matches(&matcher, "src/lexer/mod.rs", &patterns, &[3]);
        assert_matches(&matcher, "src/parser/mod.go", &patterns, &[]);
    }

    #[test]
    fn test_prefixes() {
        let patterns = ["src", "src/parser", "src/parser/"];
        let matcher = matcher_for_patterns(&patterns);

        assert_matches(&matcher, "src/parser/mod.rs", &patterns, &[0, 1, 2]);
        assert_matches(&matcher, "src/parser", &patterns, &[0, 1]);
        assert_matches(&matcher, "foo/src/parser/mod.rs", &patterns, &[0]);
    }

    #[test]
    fn test_anchoring() {
        let patterns = ["/script/foo", "script/foo", "/foo", "foo"];
        let matcher = matcher_for_patterns(&patterns);

        assert_matches(&matcher, "script/foo", &patterns, &[0, 1, 3]);
        assert_matches(&matcher, "foo", &patterns, &[2, 3]);
        assert_matches(&matcher, "bar/script/foo", &patterns, &[3]);
    }

    #[test]
    fn test_wildcards() {
        let patterns = [
            "src/*/mod.rs",
            "src/parser/*",
            "*/*/mod.rs",
            "src/parser/*/",
        ];
        let matcher = matcher_for_patterns(&patterns);

        assert_matches(&matcher, "src/parser/mod.rs", &patterns, &[0, 1, 2]);
        assert_matches(&matcher, "src/lexer/mod.rs", &patterns, &[0, 2]);
        assert_matches(&matcher, "src/parser/parser.rs", &patterns, &[1]);
        assert_matches(&matcher, "test/lexer/mod.rs", &patterns, &[2]);
        assert_matches(&matcher, "parser/mod.rs", &patterns, &[]);
        assert_matches(&matcher, "src/parser/subdir/thing.rs", &patterns, &[3]);
    }

    #[test]
    fn test_trailing_wildcards() {
        let patterns = ["/mammals/*", "/fish/*/"];
        let matcher = matcher_for_patterns(&patterns);

        assert_matches(&matcher, "mammals", &patterns, &[]);
        assert_matches(&matcher, "mammals/equus", &patterns, &[0]);
        assert_matches(&matcher, "mammals/equus/zebra", &patterns, &[]);

        assert_matches(&matcher, "fish", &patterns, &[]);
        assert_matches(&matcher, "fish/gaddus", &patterns, &[]);
        assert_matches(&matcher, "fish/gaddus/cod", &patterns, &[1]);
    }

    #[test]
    fn test_complex_patterns() {
        let patterns = ["/src/parser/*.rs", "/src/p*/*.*"];
        let matcher = matcher_for_patterns(&patterns);

        assert_matches(&matcher, "src/parser/mod.rs", &patterns, &[0, 1]);
        assert_matches(&matcher, "src/p/lib.go", &patterns, &[1]);
        assert_matches(&matcher, "src/parser/README", &patterns, &[]);
    }

    #[test]
    fn test_leading_double_stars() {
        let patterns = ["/**/baz", "/**/bar/baz"];
        let matcher = matcher_for_patterns(&patterns);

        assert_matches(&matcher, "x/y/baz", &patterns, &[0]);
        assert_matches(&matcher, "x/bar/baz", &patterns, &[0, 1]);
        assert_matches(&matcher, "baz", &patterns, &[0]);
    }

    #[test]
    fn test_infix_double_stars() {
        let patterns = ["/foo/**/qux", "/foo/qux"];
        let matcher = matcher_for_patterns(&patterns);

        assert_matches(&matcher, "foo/qux", &patterns, &[0, 1]);
        assert_matches(&matcher, "foo/bar/qux", &patterns, &[0]);
        assert_matches(&matcher, "foo/bar/baz/qux", &patterns, &[0]);
        assert_matches(&matcher, "foo/bar", &patterns, &[]);
        assert_matches(&matcher, "bar/qux", &patterns, &[]);
    }

    #[test]
    fn test_trailing_double_stars() {
        let patterns = ["foo/**", "**"];
        let matcher = matcher_for_patterns(&patterns);

        assert_matches(&matcher, "foo", &patterns, &[1]);
        assert_matches(&matcher, "bar", &patterns, &[1]);
        assert_matches(&matcher, "foo/bar", &patterns, &[0, 1]);
        assert_matches(&matcher, "x/y/baz", &patterns, &[1]);
        assert_matches(&matcher, "foo/bar/baz", &patterns, &[0, 1]);
    }

    #[test]
    fn test_escape_sequences() {
        let patterns = ["f\\*o", "a*b\\??", "\\*qux", "bar\\*", "\\*"];
        let matcher = matcher_for_patterns(&patterns);

        assert_matches(&matcher, "f*o", &patterns, &[0]);
        assert_matches(&matcher, "foo", &patterns, &[]);
        assert_matches(&matcher, "axb?!", &patterns, &[1]);
        assert_matches(&matcher, "axb?", &patterns, &[]);
        assert_matches(&matcher, "axbc!", &patterns, &[]);
        assert_matches(&matcher, "*qux", &patterns, &[2]);
        assert_matches(&matcher, "xqux", &patterns, &[]);
        assert_matches(&matcher, "bar*", &patterns, &[3]);
        assert_matches(&matcher, "bar", &patterns, &[]);
        assert_matches(&matcher, "*", &patterns, &[4]);
        assert_matches(&matcher, "a", &patterns, &[]);
    }

    fn assert_matches(matcher: &Matcher, path: &str, patterns: &[&str], expected: &[usize]) {
        assert_eq!(
            HashSet::<usize>::from_iter(matcher.matching_patterns(path).into_iter()),
            HashSet::from_iter(expected.iter().copied()),
            "expected {:?} to match {:?}",
            path,
            expected.iter().map(|&i| patterns[i]).collect::<Vec<_>>(),
        );
    }

    fn matcher_for_patterns(patterns: &[&str]) -> Matcher {
        let mut builder = Builder::new();
        for pattern in patterns {
            builder.add(pattern);
        }
        builder.build()
    }
}
