use std::{
    fs::File,
    io::BufRead,
    path::{Path, PathBuf},
};

use anyhow::{anyhow, Result};
use clap::Parser;
#[cfg(feature = "rayon")]
use rayon::prelude::*;

use codeowners_rs::RuleSet;

#[derive(Parser)]
#[command(version)]
struct Cli {
    paths: Vec<PathBuf>,

    #[clap(short = 'f', long = "file")]
    codeowners_file: Option<PathBuf>,

    #[clap(short = 'p', long = "paths-from")]
    paths_from_file: Option<PathBuf>,

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
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let codeowners_path = cli.codeowners_path();
    let ruleset = RuleSet::from_reader(File::open(codeowners_path)?);

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

fn walk_files(root: impl AsRef<Path>) -> impl Iterator<Item = PathBuf> {
    walkdir::WalkDir::new(root)
        .min_depth(1)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|entry| !entry.file_type().is_dir())
        .filter(|entry| !entry.path().starts_with("./.git"))
        .map(|entry| entry.into_path())
}
