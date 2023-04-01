//! This is a library for working with GitHub [CODEOWNERS
//! files](https://docs.github.com/en/repositories/managing-your-repositorys-settings-and-features/customizing-your-repository/about-code-owners).
//! CODEOWNERS files declare which GitHub users or teams are responsible for
//! files in a repository. The pattern syntax is loosely based on the glob-style
//! patterns used in .gitignore files.
//!
//! ## Parsing CODEOWNERS files
//! The [`parse`] and [`parse_file`] functions can be used to parse a CODEOWNERS
//! file into a [`parser::ParseResult`], which wraps a vec of [`parser::Rule`]s
//! and a vec of any [`parser::ParseError`]s that were encountered. The
//! [`parser::Rule`] struct contains full syntactic information about the rule,
//! including the leading and trailing comments, and the byte offsets for each
//! component of the rule. This is useful when you care about the CODEOWNERS
//! syntax, for instance when writing a syntax highlighter, but it's not very
//! ergonomic for most use cases.
//!
//! The [`RuleSet`] struct provides a more ergonomic interface for working with
//! CODEOWNERS files. It can be constructed by calling
//! [`into_ruleset`](fn@parser::ParseResult::into_ruleset) on a
//! [`parser::ParseResult`].
//!
//! ## Matching paths against CODEOWNERS files
//! The [`RuleSet`] struct provides a [`owners`](fn@RuleSet::owners) method that
//! can be used to match a path against a CODEOWNERS file. To get the matching
//! rule rather than just the owners, use the
//! [`matching_rule`](fn@RuleSet::matching_rule) method.
//!
//!
//! ## Example
//! ```
//! use codeowners_rs::{parse, RuleSet};
//!
//! let ruleset = parse("
//! *.rs @github/rustaceans
//! /docs/**/*.md @github/docs-team
//! ").into_ruleset();
//!
//! for path in &["src/main.rs", "docs/README.md", "README.md"] {
//!    let owners = ruleset.owners(path);
//!    println!("{}: {:?}", path, owners);
//! }
//! ```
//!
//! ## Command line interface
//! There is a companion binary crate that provides a simple CLI for matching
//! paths against a CODEOWNERS file.

pub mod parser;
pub mod patternset;
mod ruleset;

pub use parser::{parse, parse_file};
pub use ruleset::{Owner, Rule, RuleSet};
