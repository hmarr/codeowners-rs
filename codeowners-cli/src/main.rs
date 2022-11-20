use std::{
    fs::File,
    path::{Path, PathBuf},
};

use anyhow::Result;
use clap::Parser;

use codeowners_rs::{parse_rules, patternset, RuleSet};

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
    let mut builder = patternset::Builder::new();
    let rules = parse_rules(File::open(codeowners_path)?);
    for rule in &rules {
        builder.add(&rule.pattern);
    }
    let matcher = builder.build_tm();

    for root_path in cli.root_paths() {
        if !root_path.exists() {
            eprintln!("error: path does not exist: {}", root_path.display());
            continue;
        }

        if root_path.is_dir() {
            let file_iter = walk_files(root_path);
            let paths: Vec<String> = file_iter
                .map(|e| {
                    e.path()
                        .strip_prefix(".")
                        .unwrap()
                        .to_string_lossy()
                        .to_string()
                })
                .collect();
            let matches = matcher.matches_for_paths(&paths);
            for path in &paths {
                if let Some(max_pattern_id) = matches.get(path).and_then(|ids| ids.iter().max()) {
                    let rule = &rules[*max_pattern_id];
                    if rule.owners.is_empty() {
                        println!("{:<70}  (unowned)", path);
                    } else {
                        println!("{:<70}  {}", path, rule.owners.join(" "));
                    }
                } else {
                    println!("{:<70}  (unowned)", path);
                }
            }
        } else {
            // print_owners(&cli, &root_path, &ruleset);
        }
    }

    Ok(())
}

fn walk_files(root: impl AsRef<Path>) -> impl Iterator<Item = walkdir::DirEntry> {
    walkdir::WalkDir::new(root)
        .min_depth(1)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|entry| !entry.file_type().is_dir())
        .filter(|entry| !entry.path().starts_with("./.git"))
}
