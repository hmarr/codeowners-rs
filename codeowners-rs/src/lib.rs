pub mod parser;
mod patternset;
mod ruleset;

use std::{fs::File, io, path::Path};

pub use ruleset::{RuleSet, RuleSetBuilder};

pub fn from_path(path: impl AsRef<Path>) -> io::Result<RuleSet> {
    let file = File::open(path)?;
    let rules = parser::parse_rules(file);
    let mut builder = RuleSetBuilder::new();
    for rule in rules {
        builder.add(rule);
    }
    Ok(builder.build())
}
