# codeowners-rs

A fast Rust library and CLI for GitHub's [CODEOWNERS file](https://docs.github.com/en/github/creating-cloning-and-archiving-repositories/about-code-owners#codeowners-syntax).

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
