mod matcher;
mod nfa;

pub use self::matcher::Matcher;

pub struct Builder {
    nfa: nfa::Nfa,
}

impl Builder {
    pub fn new() -> Self {
        Self {
            nfa: nfa::Nfa::new(),
        }
    }

    pub fn add(&mut self, pattern: &str) {
        self.nfa.add(pattern);
    }

    pub fn build(self) -> Matcher {
        Matcher::new(self.nfa)
    }
}

impl Default for Builder {
    fn default() -> Self {
        Self::new()
    }
}
