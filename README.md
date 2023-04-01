# codeowners-rs

A fast Rust library and CLI for GitHub's [CODEOWNERS file](https://docs.github.com/en/github/creating-cloning-and-archiving-repositories/about-code-owners#codeowners-syntax).

[![crates.io](https://img.shields.io/crates/v/codeowners-rs.svg)](https://crates.io/crates/codeowners-rs)
[![docs.rs](https://img.shields.io/badge/docs.rs-codeowners--rs-blue?logo=docs.rs)](https://docs.rs/codeowners-rs)
[![CI](https://github.com/hmarr/codeowners-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/hmarr/codeowners-rs/actions/workflows/ci.yml)

Includes a fast, hand-written parser for CODEOWNERS files. The resulting parse tree includes comments and byte offsets for all syntax components, making it suitable for writing syntax highlighters or providing syntax-aware diagnostic information.

The matcher works by building an NFA from the rules, which makes this library highly performant for large rulesets and matching large numbers of paths.

## Example usage

```rust
use codeowners_rs::{parse, RuleSet};

let ruleset = parse("
*.rs @github/rustaceans
/docs/**/*.md @github/docs-team
").into_ruleset();

for path in &["src/main.rs", "docs/README.md", "README.md"] {
   let owners = ruleset.owners(path);
   println!("{}: {:?}", path, owners);
}
```

See the full documentation on [docs.rs](https://docs.rs/codeowners-rs).

## CLI usage

```
$ codeowners --help
Usage: codeowners [OPTIONS] [PATHS]...

Arguments:
  [PATHS]...

Options:
  -f, --file <CODEOWNERS_FILE>
          Path to a CODEOWNERS file. If omitted, the following locations will be tried: ./CODEOWNERS, ./.github/CODEOWNERS
  -p, --paths-from <PATHS_FROM_FILE>
          Match paths from this file rather than walking the directory tree
  -o, --owners <OWNERS>
          Filter results to files owned by this owner. May be used multiple times to match multiple owners
  -u, --unowned
          Filter results to show unowned files. May be used with -o
  -t, --threads <THREADS>
          Concurrency. If set to 0, a sensible value based on CPU count will be used [default: 0]
  -h, --help
          Print help information
  -V, --version
          Print version information
```
