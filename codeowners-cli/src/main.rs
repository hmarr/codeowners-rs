use std::{
    fs::File,
    path::{Path, PathBuf},
};

use anyhow::Result;
use clap::Parser;
#[cfg(feature = "rayon")]
use rayon::prelude::*;

use codeowners_rs::RuleSetBuilder;
use codeowners_rs::{parser, RuleSet};

#[derive(Parser)]
#[command(version)]
struct Cli {
    paths: Vec<PathBuf>,

    #[clap(short = 'f', long = "file")]
    codeowners_file: Option<PathBuf>,

    #[arg(long)]
    all_matching_rules: bool,
}

impl Cli {
    fn codeowners_path(&self) -> PathBuf {
        self.codeowners_file
            .clone()
            .unwrap_or_else(|| PathBuf::from("./CODEOWNERS"))
    }

    fn root_paths(&self) -> Vec<PathBuf> {
        if self.paths.is_empty() {
            vec![PathBuf::from(".")]
        } else {
            self.paths.clone()
        }
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let codeowners_path = cli.codeowners_path();
    let rules = parser::parse_rules(File::open(codeowners_path)?);

    let mut builder = RuleSetBuilder::new();
    for rule in rules {
        builder.add(rule);
    }
    let ruleset = builder.build();

    for root_path in cli.root_paths() {
        if !root_path.exists() {
            eprintln!("error: path does not exist: {}", root_path.display());
            continue;
        }

        let tl = thread_local::ThreadLocal::new();
        if root_path.is_dir() {
            let file_iter = walk_files(root_path);
            #[cfg(feature = "rayon")]
            let file_iter = file_iter.par_bridge();
            file_iter.for_each(|entry| {
                let thread_local_ruleset = tl.get_or(|| ruleset.clone());
                let path = entry
                    .path()
                    .strip_prefix(".")
                    .unwrap_or_else(|_| entry.path());
                print_owners(&cli, path, thread_local_ruleset);
            });
        } else {
            print_owners(&cli, &root_path, &ruleset);
        }
    }

    Ok(())
}

fn print_owners(cli: &Cli, path: impl AsRef<Path>, ruleset: &RuleSet) {
    let path = path
        .as_ref()
        .strip_prefix(".")
        .unwrap_or_else(|_| path.as_ref());
    if cli.all_matching_rules {
        let matches = ruleset.matching_rules(&Path::new(path.to_str().unwrap()));
        for (i, rule) in &matches {
            eprintln!(
                "{} matched rule #{}: {}  {}",
                path.display(),
                i + 1,
                rule.pattern,
                rule.owners.join(" ")
            );
        }
    }

    let owners = ruleset.owners(path);
    match owners {
        Some(owners) => {
            println!("{:<70}  {}", path.display(), owners.join(" "))
        }
        None => println!("{:<70}  (unowned)", path.display()),
    }
}

fn walk_files(root: impl AsRef<Path>) -> impl Iterator<Item = walkdir::DirEntry> {
    walkdir::WalkDir::new(root)
        .min_depth(1)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|entry| !entry.file_type().is_dir())
        .filter(|entry| !entry.path().starts_with("./.git"))
}
