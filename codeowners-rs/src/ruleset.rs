use std::path::Path;

use once_cell::sync::Lazy;
use regex::Regex;

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

static EMAIL_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\A[A-Z0-9a-z\._'%\+\-]+@[A-Za-z0-9\.\-]+\.[A-Za-z]{2,6}\z").unwrap());
static USERNAME_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"\A@[a-zA-Z0-9\-_]+\z").unwrap());
static TEAM_REGEX: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\A@[a-zA-Z0-9\-]+/[a-zA-Z0-9\-_]+\z").unwrap());

#[derive(Debug, Clone)]
pub struct InvalidOwnerError {
    value: String,
}

impl std::fmt::Display for InvalidOwnerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid owner: {}", self.value)
    }
}

impl std::error::Error for InvalidOwnerError {}

impl TryFrom<String> for Owner {
    type Error = InvalidOwnerError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        if EMAIL_REGEX.is_match(&value) {
            Ok(Self::new(value, OwnerKind::Email))
        } else if USERNAME_REGEX.is_match(&value) {
            Ok(Self::new(value, OwnerKind::User))
        } else if TEAM_REGEX.is_match(&value) {
            Ok(Self::new(value, OwnerKind::Team))
        } else {
            Err(InvalidOwnerError { value })
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OwnerKind {
    User,
    Team,
    Email,
}
