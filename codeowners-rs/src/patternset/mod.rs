mod matcher;
mod nfa;

pub use self::matcher::PatternSetMatcher;

pub struct PatternSetBuilder {
    nfa: nfa::Nfa,
}

impl PatternSetBuilder {
    pub fn new() -> Self {
        Self {
            nfa: nfa::Nfa::new(),
        }
    }

    pub fn add(&mut self, pattern: &str) {
        self.nfa.add(pattern);
    }

    pub fn build(self) -> PatternSetMatcher {
        PatternSetMatcher::new(self.nfa)
    }
}

impl Default for PatternSetBuilder {
    fn default() -> Self {
        Self::new()
    }
}
