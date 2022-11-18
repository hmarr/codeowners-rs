use std::io::{BufRead, BufReader, Read};

#[derive(Debug, Clone)]
pub struct Rule {
    pub pattern: String,
    pub owners: Vec<String>,
}

// TODO: Replace with a more robust parser. Currently this parser accepts
// various invalid inputs, fails to parse some valid inputs, and ignores any
// errors.
pub fn parse_rules(reader: impl Read) -> Vec<Rule> {
    BufReader::new(reader)
        .lines()
        .map(|line| line.unwrap())
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let pattern = parts.next()?.to_owned();
            let owners = parts.map(|s| s.to_owned()).collect();

            Some(Rule { pattern, owners })
        })
        .collect()
}
