use std::{
    fs::File,
    io::{BufRead, Read},
    path::{Path, PathBuf},
};

use anyhow::{anyhow, Context, Result};
use clap::Parser;
#[cfg(feature = "rayon")]
use rayon::prelude::*;

use codeowners_rs::{self, Owner, RuleSet};

#[derive(Parser)]
#[command(version)]
struct Cli {
    paths: Vec<PathBuf>,

    /// Path to a CODEOWNERS file. If omitted, the following locations will be tried:
    /// ./CODEOWNERS, ./.github/CODEOWNERS
    #[clap(short = 'f', long = "file")]
    codeowners_file: Option<PathBuf>,

    /// Match paths from this file rather than walking the directory tree
    #[clap(short = 'p', long = "paths-from")]
    paths_from_file: Option<PathBuf>,

    /// Filter results to files owned by this owner. May be used multiple times to
    /// match multiple owners
    #[clap(short = 'o', long = "owners")]
    owners: Vec<String>,

    /// Filter results to show unowned files. May be used with -o.
    #[clap(short = 'u', long = "unowned")]
    unowned: bool,

    /// Concurrency. If set to 0, a sensible value based on CPU count will be used.
    #[clap(short = 't', long = "threads", default_value_t = 0)]
    threads: usize,

    #[cfg(debug_assertions)]
    #[arg(long)]
    all_matching_rules: bool,
}

impl Cli {
    const DEFAULT_PATHS: &'static [&'static str] = &["./CODEOWNERS", ".github/CODEOWNERS"];

    fn codeowners_path(&self) -> Option<PathBuf> {
        match &self.codeowners_file {
            Some(path) => Some(path.clone()),
            None => Self::DEFAULT_PATHS
                .iter()
                .map(PathBuf::from)
                .find(|p| p.exists()),
        }
    }

    fn root_paths(&self) -> Vec<PathBuf> {
        if self.paths.is_empty() {
            vec![PathBuf::from(".")]
        } else {
            self.paths.clone()
        }
    }

    // Return an iterator over all files to be checked. If --paths-from is set,
    // return an iterator over the paths in that file. Otherwise, return an
    // iterator over all files in the root paths. If multiple root paths are
    // given, the iterator will return files from all of them.
    fn paths_iter(&self) -> Result<Box<dyn Iterator<Item = PathBuf> + Send>> {
        if let Some(paths_from_file) = &self.paths_from_file {
            let file = File::open(paths_from_file)
                .map_err(|e| anyhow!("reading {:?}: {}", paths_from_file, e))?;
            let reader = std::io::BufReader::new(file);
            Ok(Box::new(
                reader.lines().filter_map(|l| l.ok()).map(PathBuf::from),
            ))
        } else {
            Ok(self.root_paths().into_iter().map(walk_files).fold(
                Box::new(std::iter::empty()) as Box<dyn Iterator<Item = _> + Send>,
                |a, b| Box::new(a.chain(b)),
            ))
        }
    }

    fn matches_owners_filters(&self, file_owners: Option<&[Owner]>) -> bool {
        if let Some(file_owners) = file_owners {
            // Owned files. If Some, slice will be non-empty.
            if self.owners.is_empty() && !self.unowned {
                // No filters applied
                return true;
            }

            for owner in file_owners {
                if self.owners.contains(&owner.value) {
                    return true;
                }
            }

            // No filters matched
            false
        } else {
            // Unowned files
            self.unowned || self.owners.is_empty()
        }
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    #[cfg(feature = "rayon")]
    rayon::ThreadPoolBuilder::new()
        .num_threads(cli.threads)
        .build_global()?;

    let Some(codeowners_path) = cli.codeowners_path() else {
        eprintln!("error: no CODEOWNERS file found");
        std::process::exit(1);
    };

    let mut file =
        File::open(&codeowners_path).with_context(|| format!("opening {:?}", codeowners_path))?;
    let mut source = String::new();
    file.read_to_string(&mut source)
        .with_context(|| format!("reading {:?}", codeowners_path))?;

    let parse_result = codeowners_rs::parse(&source);
    if !parse_result.errors.is_empty() {
        for (i, error) in parse_result.errors.iter().enumerate() {
            print_parse_error(&codeowners_path, &source, error);
            if i < parse_result.errors.len() - 1 {
                println!();
            }
        }
        std::process::exit(1);
    }
    let ruleset = parse_result.into_ruleset();

    for root_path in cli.root_paths() {
        if !root_path.exists() {
            eprintln!("error: path does not exist: {}", root_path.display());
            continue;
        }
    }

    let paths = cli.paths_iter()?;
    #[cfg(feature = "rayon")]
    let paths = paths.par_bridge();

    let tl = thread_local::ThreadLocal::new();
    paths.for_each(|path| {
        let thread_local_ruleset = tl.get_or(|| ruleset.clone());
        let path = path.strip_prefix(".").unwrap_or(&path);
        print_owners(&cli, path, thread_local_ruleset);
    });

    Ok(())
}

fn print_owners(cli: &Cli, path: impl AsRef<Path>, ruleset: &RuleSet) {
    let path = path
        .as_ref()
        .strip_prefix(".")
        .unwrap_or_else(|_| path.as_ref());

    #[cfg(debug_assertions)]
    if cli.all_matching_rules {
        let matches = ruleset.all_matching_rules(Path::new(path.to_str().unwrap()));
        for (i, rule) in &matches {
            eprintln!(
                "{} matched rule #{}: {}  {}",
                path.display(),
                i + 1,
                rule.pattern,
                rule.owners
                    .iter()
                    .map(|o| o.value.as_str())
                    .collect::<Vec<&str>>()
                    .join(" ")
            );
        }
    }

    let owners = ruleset.owners(path);
    if cli.matches_owners_filters(owners) {
        match owners {
            Some(owners) => {
                println!(
                    "{:<70}  {}",
                    path.display(),
                    owners
                        .iter()
                        .map(|o| o.value.as_str())
                        .collect::<Vec<&str>>()
                        .join(" ")
                )
            }
            None => {
                println!("{:<70}  (unowned)", path.display())
            }
        }
    }
}

fn walk_files(root: impl AsRef<Path>) -> impl Iterator<Item = PathBuf> {
    walkdir::WalkDir::new(root)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|entry| !entry.file_type().is_dir())
        .filter(|entry| !entry.path().starts_with("./.git"))
        .map(|entry| entry.into_path())
}

fn print_parse_error(path: &Path, source: &str, error: &codeowners_rs::parser::ParseError) {
    let mut line = 1;
    let mut line_start = 0;
    let mut line_end = 0;
    for l in source.lines() {
        line_end += l.len() + 1;
        if line_end > error.span.0 {
            break;
        }
        line_start = line_end;
        line += 1;
    }

    eprintln!("{} line {}: {}", path.display(), line, error.message);

    let line_prefix = format!("{:4} | ", line);
    let context = &source[line_start..line_end];
    eprint!("{}{}", line_prefix, context);

    let padding = " ".repeat(error.span.0 - line_start + line_prefix.len());
    let underline = "^".repeat(error.span.1 - error.span.0);
    eprintln!("{}{}", padding, underline);
}
