//! This is a library for working with GitHub [CODEOWNERS files](https://docs.github.com/en/repositories/managing-your-repositorys-settings-and-features/customizing-your-repository/about-code-owners).
//! CODEOWNERS files declare which GitHub users or teams are responsible for
//! files in a repository. The pattern syntax is loosely based on the glob-style
//! patterns used in .gitignore files.
//!
//! ## Warning: this is a work in progress
//! This project is in an early state, and most of the work has gone into making
//! the matching performant so that large numbers of files and rules can be
//! processed quickly. A CODEOWNERS file parser is included, but it's terrible
//! and needs a rewrite—it accepts invalid files without error, and fails to
//! parse some valid files.
//!
//! ## Command line interface
//! There is a companion binary crate that provides a simple CLI for matching
//! paths against a CODEOWNERS file.
//!
//! ## Example
//! ```
//! use codeowners_rs::{parse_rules, RuleSet};
//!
//! let rules = parse_rules(std::io::Cursor::new("
//! *.rs @github/rustaceans
//! /docs/**/*.md @github/docs-team
//! "));
//! let ruleset = RuleSet::new(rules);
//!
//! for path in &["src/main.rs", "docs/README.md", "README.md"] {
//!    let owners = ruleset.owners(path);
//!    println!("{}: {:?}", path, owners);
//! }
//! ```

mod parser;
pub mod patternset;
mod ruleset;

pub use parser::{parse_rules, Rule};
pub use ruleset::RuleSet;
