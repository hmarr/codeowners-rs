// Newtype for a state index in the NFA.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub(crate) struct StateId(pub u32);

impl From<StateId> for usize {
    fn from(id: StateId) -> usize {
        id.0 as usize
    }
}

// A state in the NFA.
#[derive(Debug, Clone)]
pub(crate) struct State {
    // Denotes this state as a terminal state for all patterns in the vector.
    pub(crate) terminal_for_patterns: Option<Vec<usize>>,
    // Transitions from this state to other states.
    pub(crate) transitions: Vec<Transition>,
    // Epislon transitions are unconditionally traversed when _entering_ this
    // state. They're used for handling recursive (**) patterns. Note they
    // differ from wildcard transitions, which match any segment, but are
    // considered when _leaving_ this state rather than entering it. As epsilon
    // transitions are unconditional, we only ever need one for a given state.
    pub(crate) epsilon_transition: Option<StateId>,
}

impl State {
    pub(crate) fn new() -> Self {
        Self {
            terminal_for_patterns: None,
            transitions: Vec::new(),
            epsilon_transition: None,
        }
    }

    pub(crate) fn add_transition(&mut self, transition: Transition) {
        self.transitions.push(transition);
    }

    pub(crate) fn mark_as_terminal(&mut self, pattern_id: usize) {
        if let Some(patterns) = &mut self.terminal_for_patterns {
            patterns.push(pattern_id);
        } else {
            self.terminal_for_patterns = Some(vec![pattern_id]);
        }
    }
}

// A nondeterministic finite automaton (NFA) for matching patterns. The
// construction logic lives in the `Builder` struct and the matching logic lives
// in the `Matcher` struct. The `Nfa` struct is a thin wrapper around a vector of
// states.
#[derive(Clone)]
pub(crate) struct Nfa {
    states: Vec<State>,
}

impl Nfa {
    pub(crate) const START_STATE: StateId = StateId(0);

    pub(crate) fn new() -> Self {
        let states = vec![State::new()];
        Self { states }
    }

    // Allocate a new state, returning its unique id in the NFA.
    pub(crate) fn add_state(&mut self) -> StateId {
        let id = self.states.len();

        let state = State::new();
        self.states.push(state);

        StateId(id as u32)
    }

    // Given a state id, get an immutable reference to the state.
    pub(crate) fn state(&self, id: StateId) -> &State {
        &self.states[usize::from(id)]
    }

    // Given a state id, get a mutable reference to the state.
    pub(crate) fn state_mut(&mut self, id: StateId) -> &mut State {
        &mut self.states[usize::from(id)]
    }

    // Get the initial set of state ids, automatically traversing epsilon edges.
    pub(crate) fn initial_states(&self) -> Vec<StateId> {
        let mut states = vec![Self::START_STATE];
        if let Some(epsilon_node_id) = self.state(Self::START_STATE).epsilon_transition {
            states.push(epsilon_node_id);
        }
        states
    }

    // Return an iterator over all transitions from the given state id.
    pub(crate) fn transitions_from(&self, state_id: StateId) -> impl Iterator<Item = &Transition> {
        self.state(state_id).transitions.iter()
    }

    // Get the epsilon transition for the given state id.
    pub(crate) fn epsilon_transitions_from(&self, state_id: StateId) -> Option<StateId> {
        self.state(state_id).epsilon_transition
    }

    // Return an iterater over all states. Only used in tests.
    #[cfg(test)]
    pub(crate) fn states_iter(&self) -> std::slice::Iter<'_, State> {
        self.states.iter()
    }
}

// A transition from one state to another. For each (from_state, path_segment)
// pair, there should only ever be a single transition.
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

// Different types of transitions have different conditions for matching. While
// we could compile every transition into a regex, this kind of specialisation
// lets us create fast paths for simpler patterns.
#[derive(Debug, Clone)]
enum TransitionCondition {
    // A pattern segment that's a single asterisk matches anything.
    Unconditional,
    // Any literal string that requires an exact match.
    Literal,
    // Any literal pattern ends with an asterisk is a prefix match.
    Prefix,
    // Any literal pattern starts with an asterisk is a prefix match.
    Suffix,
    // Any literal pattern starts and ends with an asterisk is a substring match.
    Contains,
    // Anything more complex becomes a regex.
    Regex(regex::Regex),
}

impl TransitionCondition {
    fn new(glob: &str) -> Self {
        if glob == "*" {
            return Self::Unconditional;
        }

        // We need to remove backslashes from the pattern to perform literal
        // comparisons. Calling `replace` and storing the result causes an extra
        // allocation for each path segment. We could use a Cow, but
        // self-referencial structs are tricky. Instead, we assume backslashes
        // appear infrequently and fall back to a regex match.
        if glob.contains('\\') {
            return Self::Regex(pattern_to_regex(glob));
        }

        // Use fast-path literal matches if possible, otherwise fall back to regexes.
        let (leading_star, trailing_star, internal_wildcards) = wildcard_locations(glob);
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
            Self::Prefix => candidate.starts_with(&pattern[0..pattern.len() - 1]),
            Self::Suffix => candidate.ends_with(&pattern[1..]),
            Self::Contains => memchr::memmem::find(
                candidate.as_bytes(),
                &pattern.as_bytes()[1..pattern.len() - 1],
            )
            .is_some(),
            Self::Regex(re) => re.is_match(candidate),
        }
    }
}

// Convert a glob-style pattern to a regular expression.
fn pattern_to_regex(pattern: &str) -> regex::Regex {
    let mut regex = String::new();
    regex.push_str(r#"\A"#);

    let mut escape = false;
    for c in pattern.chars() {
        // The the previous character was a backslash, the current character is
        // a literal rather than a special character.
        if escape {
            if regex_syntax::is_meta_character(c) {
                regex.push('\\');
            }
            regex.push(c);
            escape = false;
            continue;
        }

        match c {
            // * matches any number of characters up to the next path separator
            '*' => regex.push_str(r#"[^/]*"#),
            // * matches exactly one non-path separator character
            '?' => regex.push_str(r#"[^/]"#),
            // \ escapes the next character
            '\\' => escape = true,
            _ => {
                // Make sure we're escaping other regex special characters.
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

// Returns whether there are unescaped wildcards at the (start, end, middle) of
// the pattern.
fn wildcard_locations(pattern: &str) -> (bool, bool, bool) {
    let mut chars = pattern.chars();

    // Extract the first and last characters from the iterator so we can look at
    // the inside of the pattern.
    let first = chars.next();
    let last = chars.next_back();

    // Check for internal wildcards.
    let mut prev = first;
    let mut internal_wildcard = false;
    for c in chars {
        // If the previous character was a backslash, this one is escaped.
        if (c == '*' || c == '?') && prev != Some('\\') {
            internal_wildcard = true;
        }
        prev = Some(c);
    }

    (
        first.map(|c| c == '*' || c == '?').unwrap_or(false),
        last.map(|c| c == '*' || c == '?').unwrap_or(false) && prev != Some('\\'),
        internal_wildcard,
    )
}
