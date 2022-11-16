use std::path::Path;

use super::{PatternSetBuilder, PatternSetMatcher};

#[derive(Clone)]
pub struct GlobsetMatcher(globset::GlobSet);

impl PatternSetMatcher for GlobsetMatcher {
    fn matching_patterns(&self, path: impl AsRef<Path>) -> Vec<usize> {
        self.0.matches(path.as_ref()).into_iter().collect()
    }
}

pub struct GlobsetBuilder(globset::GlobSetBuilder);

impl PatternSetBuilder for GlobsetBuilder {
    type Matcher = GlobsetMatcher;

    fn new() -> Self {
        Self(globset::GlobSetBuilder::new())
    }

    fn add(&mut self, pattern: &str) {
        let mut glob_str = String::new();
        if pattern.starts_with('/') {
            glob_str.push_str(pattern.strip_prefix('/').unwrap());
        } else {
            glob_str.push_str("**/");
            glob_str.push_str(pattern);
        }

        if pattern.ends_with('/') {
            glob_str.push_str("**");
        } else {
            glob_str.push_str("/**");
        }

        let glob = globset::GlobBuilder::new(&glob_str)
            .literal_separator(true)
            .build()
            .unwrap();
        self.0.add(glob);
    }

    fn build(self) -> Self::Matcher {
        GlobsetMatcher(self.0.build().unwrap())
    }
}
