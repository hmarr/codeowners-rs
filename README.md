# codeowners-rs

A Rust library and CLI for GitHub's [CODEOWNERS file](https://docs.github.com/en/github/creating-cloning-and-archiving-repositories/about-code-owners#codeowners-syntax).

Still a work in progress. The parser is particularly inadequate right now.

The set of patterns is turned into an NFA, which makes matching fast even when there are a large number of patterns.
