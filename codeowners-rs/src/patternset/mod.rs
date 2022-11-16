mod globset;
mod matcher;
mod nfa;

use std::path::Path;

pub use self::globset::{GlobsetBuilder, GlobsetMatcher};
pub use self::matcher::NfaMatcher;

pub trait PatternSetMatcher: Clone {
    fn matching_patterns(&self, path: impl AsRef<Path>) -> Vec<usize>;
}

pub trait PatternSetBuilder {
    type Matcher: PatternSetMatcher;

    fn new() -> Self;
    fn add(&mut self, pattern: &str);
    fn build(self) -> Self::Matcher;
}

pub struct NfaBuilder {
    nfa: nfa::Nfa,
}

impl PatternSetBuilder for NfaBuilder {
    type Matcher = NfaMatcher;

    fn new() -> Self {
        Self {
            nfa: nfa::Nfa::new(),
        }
    }

    fn add(&mut self, pattern: &str) {
        self.nfa.add(pattern);
    }

    fn build(self) -> NfaMatcher {
        NfaMatcher::new(self.nfa)
    }
}

impl Default for NfaBuilder {
    fn default() -> Self {
        Self::new()
    }
}
