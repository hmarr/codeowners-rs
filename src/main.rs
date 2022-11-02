use std::{
    collections::HashMap,
    fs::File,
    path::{Path, PathBuf},
};

use anyhow::Result;
use clap::Parser;
use rayon::prelude::*;

use nfa::PatternNFA;

mod nfa;
mod parser;

#[derive(Parser)]
#[command(version)]
struct Cli {
    paths: Vec<PathBuf>,

    #[arg(long)]
    all_matching_rules: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let rules = parser::parse_rules(File::open("./CODEOWNERS")?);

    let mut nfa = PatternNFA::new();
    let rule_ids = rules
        .iter()
        .enumerate()
        .map(|(i, rule)| (nfa.add_pattern(&rule.pattern), i))
        .collect::<HashMap<_, _>>();

    let root_paths = if cli.paths.is_empty() {
        vec![PathBuf::from(".")]
    } else {
        cli.paths.clone()
    };

    for root_path in root_paths {
        if !root_path.exists() {
            eprintln!("error: path does not exist: {}", root_path.display());
            continue;
        }

        let tl = thread_local::ThreadLocal::new();
        if root_path.is_dir() {
            walk_files(root_path).par_bridge().for_each(|entry| {
                let thread_nfa = tl.get_or(|| nfa.clone());
                let path = entry
                    .path()
                    .strip_prefix(".")
                    .unwrap_or_else(|_| entry.path());
                print_owners(&cli, path, thread_nfa, &rule_ids, &rules);
            });
        } else {
            print_owners(&cli, &root_path, &nfa, &rule_ids, &rules);
        }
    }

    Ok(())
}

fn print_owners(
    cli: &Cli,
    path: impl AsRef<Path>,
    nfa: &PatternNFA,
    rule_ids: &HashMap<usize, usize>,
    rules: &[parser::Rule],
) {
    let path = path
        .as_ref()
        .strip_prefix(".")
        .unwrap_or_else(|_| path.as_ref());
    let matches = nfa.matching_patterns(path.to_str().unwrap());
    if cli.all_matching_rules {
        for match_id in &matches {
            let rule_id = rule_ids[match_id];
            let rule = &rules[rule_id];
            eprintln!(
                "{} matched rule #{}: {}  {}",
                path.display(),
                rule_id + 1,
                rule.pattern,
                rule.owners.join(" ")
            );
        }
    }

    let owners = match matches.iter().max() {
        Some(id) => {
            let owners = &rules[*rule_ids.get(id).unwrap()].owners;
            if owners.is_empty() {
                None
            } else {
                Some(owners)
            }
        }
        None => None,
    };
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
