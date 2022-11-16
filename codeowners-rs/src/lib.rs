pub mod parser;
mod patternset;
mod ruleset;

use std::{fs::File, io, path::Path};

pub use patternset::{GlobsetBuilder, GlobsetMatcher};
pub use patternset::{NfaBuilder, NfaMatcher, PatternSetMatcher};
pub use ruleset::{RuleSet, RuleSetBuilder};

pub fn from_path(path: impl AsRef<Path>) -> io::Result<RuleSet<NfaMatcher>> {
    let file = File::open(path)?;
    let rules = parser::parse_rules(file);
    let mut builder = RuleSetBuilder::<NfaBuilder>::new();
    for rule in rules {
        builder.add(rule);
    }
    Ok(builder.build())
}
