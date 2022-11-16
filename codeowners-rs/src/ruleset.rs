use std::path::Path;

use crate::{
    parser::Rule,
    patternset::{NfaBuilder, NfaMatcher, PatternSetBuilder, PatternSetMatcher},
};

#[derive(Clone)]
pub struct RuleSet<M: PatternSetMatcher = NfaMatcher> {
    rules: Vec<Rule>,
    pattern_set: M,
}

impl<M: PatternSetMatcher> RuleSet<M> {
    pub fn matching_rules(&self, path: impl AsRef<Path>) -> Vec<(usize, &Rule)> {
        self.pattern_set
            .matching_patterns(path)
            .iter()
            .map(|&idx| (idx, &self.rules[idx]))
            .collect()
    }

    pub fn owners(&self, path: impl AsRef<Path>) -> Option<&[String]> {
        self.pattern_set
            .matching_patterns(path)
            .iter()
            .max()
            .and_then(|&idx| {
                if self.rules[idx].owners.is_empty() {
                    None
                } else {
                    Some(self.rules[idx].owners.as_ref())
                }
            })
    }
}

pub struct RuleSetBuilder<B: PatternSetBuilder = NfaBuilder> {
    rules: Vec<Rule>,
    pattern_set_builder: B,
}

impl<B: PatternSetBuilder> RuleSetBuilder<B> {
    pub fn new() -> Self {
        Self {
            rules: Vec::new(),
            pattern_set_builder: B::new(),
        }
    }

    pub fn add(&mut self, rule: Rule) {
        self.pattern_set_builder.add(&rule.pattern);
        self.rules.push(rule);
    }

    pub fn build(self) -> RuleSet<B::Matcher> {
        RuleSet {
            rules: self.rules,
            pattern_set: self.pattern_set_builder.build(),
        }
    }
}

impl<B: PatternSetBuilder> Default for RuleSetBuilder<B> {
    fn default() -> Self {
        Self::new()
    }
}
