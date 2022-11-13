use codeowners_rs::{parser::Rule, RuleSet, RuleSetBuilder};
use criterion::{criterion_group, criterion_main, Criterion};

const TEST_PATHS: &[&str] = &[
    "file-a",
    "dir-a/file-a",
    "dir-a/dir-c/file-a",
    "dir-a/dir-c/file-b",
    "dir-b/file-a",
    "dir-b/dir-d/dir-e/dir-f/dir-g/file-a",
];

const TEST_PATTERNS: &[&str] = &[
    "*",
    "*-a",
    "file-*",
    "/dir-b",
    "dir-a/dir-b",
    "**/dir-*/file-*",
    "dir-*/*",
    "dir-b/dir-d/dir-e/dir-f/dir-g/file-a",
];

fn build_patternset(patterns: &[&str]) -> RuleSet {
    let mut builder = RuleSetBuilder::new();
    for pattern in patterns {
        builder.add(Rule {
            pattern: pattern.to_string(),
            owners: vec![],
        });
    }
    builder.build()
}

fn patternset_benchmark(c: &mut Criterion) {
    c.bench_function("building", |b| b.iter(|| build_patternset(TEST_PATTERNS)));

    let patternset = build_patternset(TEST_PATTERNS);
    c.bench_function("matching", |b| {
        b.iter(|| {
            for p in TEST_PATHS {
                patternset.matching_rules(p);
            }
        })
    });
}

criterion_group!(benches, patternset_benchmark);
criterion_main!(benches);
