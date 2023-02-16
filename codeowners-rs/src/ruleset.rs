use std::path::Path;

use crate::patternset;

/// `RuleSet` is a collection of CODEOWNERS rules that can be matched together
/// against a given path. It is constructed by passing a `Vec` of `Rule` structs
/// to `RuleSet::new`. For convenience, `RuleSet::from_reader` can be used to
/// parse a CODEOWNERS file and construct a `RuleSet` from it.
///
/// # Example
/// ```
/// use codeowners_rs::{RuleSet, parse};
///
/// let ruleset = parse("*.rs rustacean@example.com").into_ruleset();
/// assert_eq!(format!("{:?}", ruleset.owners("main.rs")), "Some([Owner { value: \"rustacean@example.com\", kind: Email }])");
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
    pub fn owners(&self, path: impl AsRef<Path>) -> Option<&[Owner]> {
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

// `Rule` is an individual CODEOWNERS rule. It contains a pattern and a list of
// owners.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    pub pattern: String,
    pub owners: Vec<Owner>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Owner {
    pub value: String,
    pub kind: OwnerKind,
}

impl Owner {
    pub fn new(value: String, kind: OwnerKind) -> Self {
        Self { value, kind }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OwnerKind {
    User,
    Team,
    Email,
}
