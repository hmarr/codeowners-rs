use std::{io, path::Path};

use crate::{
    parser::{parse_rules, Rule},
    patternset,
};

/// `RuleSet` is a collection of CODEOWNERS rules that can be matched together
/// against a given path. It is constructed by passing a `Vec` of `Rule` structs
/// to `RuleSet::new`. For convenience, `RuleSet::from_reader` can be used to
/// parse a CODEOWNERS file and construct a `RuleSet` from it.
///
/// # Example
/// ```
/// use codeowners_rs::{RuleSet, parse_rules};
///
/// let reader = std::io::Cursor::new("*.rs @rustacean");
/// let ruleset = RuleSet::new(parse_rules(reader));
/// assert_eq!(ruleset.owners("main.rs"), Some(&["@rustacean".to_string()][..]));
/// ```
#[derive(Clone)]
pub struct RuleSet {
    rules: Vec<Rule>,
    matcher: patternset::Matcher,
}

impl RuleSet {
    /// Construct a `RuleSet` from a `Vec` of `Rule`s.
    pub fn new(rules: Vec<Rule>) -> Self {
        let mut builder = patternset::Builder::new();
        for rule in &rules {
            builder.add(&rule.pattern);
        }
        let matcher = builder.build();
        Self { rules, matcher }
    }

    /// Construct a `RuleSet` from a `Read`er that contains a CODEOWNERS file.
    pub fn from_reader(reader: impl io::Read) -> Self {
        Self::new(parse_rules(reader))
    }

    /// Returns the rule (along with its index) that matches the given path. If
    /// multiple rules match, the last one is returned.
    pub fn matching_rules(&self, path: impl AsRef<Path>) -> Vec<(usize, &Rule)> {
        self.matcher
            .matching_patterns(path)
            .iter()
            .map(|&idx| (idx, &self.rules[idx]))
            .collect()
    }

    /// Returns the owners for the given path.
    pub fn owners(&self, path: impl AsRef<Path>) -> Option<&[String]> {
        self.matcher
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
