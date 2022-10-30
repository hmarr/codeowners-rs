use std::{collections::HashMap, fs::File};

use anyhow::Result;
use nfa::PatternNFA;

mod nfa;
mod parser;

fn main() -> Result<()> {
    let rules = parser::parse_rules(File::open("./CODEOWNERS")?);

    let mut nfa = PatternNFA::new();
    let rule_ids = rules
        .iter()
        .enumerate()
        .map(|(i, rule)| (nfa.add_pattern(&rule.pattern), i))
        .collect::<HashMap<_, _>>();

    let root = ".";
    for entry in walk_files(root) {
        let path = entry
            .path()
            .strip_prefix(".") // TODO strip root?
            .unwrap_or_else(|_| entry.path());

        match nfa.matching_patterns(path.to_str().unwrap()).iter().max() {
            Some(id) => {
                let rule = &rules[*rule_ids.get(id).unwrap()];
                println!("{:<70}  {}", path.display(), rule.owners.join(" ")) // TODO join alloc?
            }
            None => println!("{:<70}  (unowned)", path.display()),
        }
    }

    Ok(())
}

fn walk_files(root: &str) -> impl Iterator<Item = walkdir::DirEntry> {
    walkdir::WalkDir::new(root)
        .min_depth(1)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|entry| entry.file_type().is_file())
        .filter(|entry| !entry.path().starts_with("./.git"))
}
