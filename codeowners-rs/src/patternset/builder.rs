use super::{
    nfa::{Nfa, StateId, Transition},
    Matcher, TreeMatcher,
};

/// Builder for a patternset [`Matcher`]. Calling [`Builder::build`] will
/// consume the builder.
#[derive(Clone)]
pub struct Builder {
    nfa: Nfa,
    next_pattern_id: usize,
}

impl Builder {
    /// Create a new `Builder`.
    pub fn new() -> Self {
        Self {
            nfa: Nfa::new(),
            next_pattern_id: 0,
        }
    }

    /// Build the `Matcher` from the patterns added to the builder. This will
    /// consume the builder.    
    pub fn build(self) -> Matcher {
        Matcher::new(self.nfa)
    }

    // TODO: use a Matcher trait and `build` generic over the matcher type.
    pub fn build_tree_matcher(self) -> TreeMatcher {
        TreeMatcher::new(self.nfa)
    }

    /// Add a pattern to the builder.
    pub fn add(&mut self, pattern: &str) -> usize {
        let pattern_id = self.next_pattern_id;
        self.next_pattern_id += 1;

        let mut start_state_id = Nfa::START_STATE;

        // Remove the leading slash if present. It forces left-anchoring so we
        // need to remember whether it was present or not.
        let (pattern, leading_slash) = match pattern.strip_prefix('/') {
            Some(pattern) => (pattern, true),
            None => (pattern, false),
        };

        // We currently only match files (as opposed to directories), so the trailing slash
        // has no effect except adding an extra empty path component at the end.
        let (pattern, trailing_slash) = match pattern.strip_suffix('/') {
            Some(pattern) => (pattern, true),
            None => (pattern, false),
        };

        // CODEOWNERS files use Unix path separators.
        let segments = pattern.split('/').collect::<Vec<_>>();

        // All patterns are left-anchored unless they're a single component with
        // no leading slash (but a trailing slash is permitted).
        if !leading_slash && segments.len() == 1 {
            start_state_id = self.add_epsilon_transition(Nfa::START_STATE);
        }

        // Add states and transitions for each of the pattern components.
        let mut end_state_id =
            segments
                .iter()
                .fold(start_state_id, |from_id, segment| match *segment {
                    "**" => self.add_epsilon_transition(from_id),
                    _ => self.add_transition(from_id, segment),
                });

        // If the pattern ends with a trailing slash or /**, we match everything
        // under the directory, but not the directory itself, so we need one
        // more segment
        if trailing_slash || segments.last() == Some(&"**") {
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
        self.nfa
            .state_mut(end_state_id)
            .mark_as_terminal(pattern_id);

        pattern_id
    }

    // Add a regular (non-epsilon) transition from a given state via the
    // provided path segment.
    fn add_transition(&mut self, from_id: StateId, segment: &str) -> StateId {
        let existing_transition = self
            .nfa
            .transitions_from(from_id)
            .find(|t| t.path_segment == segment && t.target != from_id);
        if let Some(t) = existing_transition {
            t.target
        } else {
            let state_id = self.nfa.add_state();
            self.nfa
                .state_mut(from_id)
                .add_transition(Transition::new(segment.to_owned(), state_id));
            state_id
        }
    }

    // Add an epsilon transition from a given state to a new state. If an epsilon transition
    // already exists, return the id of that transition.
    fn add_epsilon_transition(&mut self, from_id: StateId) -> StateId {
        // Double star segments match zero or more of anything, so there's never a need to
        // have multiple consecutive double star states. Multiple consecutive double star
        // states mean we require multiple path segments, which violoates the gitignore spec
        let has_existing_transition = self
            .nfa
            .transitions_from(from_id)
            .any(|t| t.path_segment == "*" && t.target == from_id);
        if has_existing_transition {
            return from_id;
        }

        match self.nfa.state(from_id).epsilon_transition {
            // If there's already an epsilon transition, don't create a new one
            // as multiple epsilon transitions coalesce into one
            Some(to_id) => to_id,
            // Otherwise, add a new state and an epsilon transition to it
            None => {
                let state_id = self.nfa.add_state();
                self.nfa
                    .state_mut(state_id)
                    .add_transition(Transition::new("*".to_owned(), state_id));
                self.nfa.state_mut(from_id).epsilon_transition = Some(state_id);
                state_id
            }
        }
    }
}

impl Default for Builder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nfa_builder() {
        let mut builder = Builder::new();

        builder.add("/foo/*");
        assert_eq!(
            transitions_for(&builder.nfa),
            vec![(0, "foo".to_owned(), 1), (1, "*".to_owned(), 2)]
        );

        builder.add("/foo/bar");
        assert_eq!(
            transitions_for(&builder.nfa),
            vec![
                (0, "foo".to_owned(), 1),
                (1, "*".to_owned(), 2),
                (1, "bar".to_owned(), 3),
                (4, "*".to_owned(), 4)
            ]
        );
    }

    #[test]
    fn test_thing() {
        let pat = "/modules/thanos-*/**";
        let mut builder = Builder::new();
        builder.add(pat);
        println!("{}", generate_dot(&builder.nfa));
    }

    #[allow(dead_code)]
    fn generate_dot(nfa: &Nfa) -> String {
        let mut dot = String::from("digraph G {\n  rankdir=\"LR\"\n");
        for (state_id, state) in nfa.states_iter().enumerate() {
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
        nfa.states_iter()
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
