use std::{fs::File, io::Read, path::Path};

use crate::ruleset::{self, Owner};

/// Parse a CODEOWNERS file from a string, returning a `ParseResult` containing
/// the parsed rules and any errors encountered.
pub fn parse(source: &str) -> ParseResult {
    Parser::new(source).parse()
}

/// Parse a CODEOWNERS file from a file path, reading the contents of the file
/// and returning a `ParseResult` containing the parsed rules and any errors
/// encountered.
pub fn parse_file(path: &Path) -> std::io::Result<ParseResult> {
    let mut file = File::open(path)?;
    let mut source = String::new();
    file.read_to_string(&mut source)?;
    Ok(parse(&source))
}

/// The result of parsing a CODEOWNERS file. Contains a `Vec` of parsed rules
/// and a `Vec` of errors encountered during parsing. If the `Vec` of errors is
/// non-empty, the `Vec` of rules may be incomplete. If the `Vec` of errors is
/// empty, the file was parsed successfully.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseResult {
    pub rules: Vec<Rule>,
    pub errors: Vec<ParseError>,
}

impl ParseResult {
    /// Convert the `ParseResult` into a `RuleSet`. If the `ParseResult` contains
    /// any errors, they are ignored.
    pub fn into_ruleset(self: ParseResult) -> ruleset::RuleSet {
        ruleset::RuleSet::new(self.rules.into_iter().map(|r| r.into()).collect())
    }
}

/// A parsed CODEOWNERS rule. Contains a pattern and a list of owners, along
/// with any comments that were found before or after the rule. All fields are
/// wrapped in `Spanned` to preserve the original source location.
///
/// For most uses, the `Rule` type should be converted into a `ruleset::Rule`
/// using the `From` trait or the `into_ruleset` method on `ParseResult`. This
/// will remove the `Spanned` wrappers and discard any comments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    pub pattern: Spanned<String>,
    pub owners: Vec<Spanned<Owner>>,
    pub leading_comments: Vec<Spanned<String>>,
    pub trailing_comment: Option<Spanned<String>>,
}

impl Rule {
    fn new(pattern: Spanned<String>, owners: Vec<Spanned<Owner>>) -> Rule {
        Rule {
            pattern,
            owners,
            leading_comments: Vec::new(),
            trailing_comment: None,
        }
    }
}

impl From<Rule> for ruleset::Rule {
    fn from(rule: Rule) -> Self {
        ruleset::Rule {
            pattern: rule.pattern.0,
            owners: rule.owners.into_iter().map(|o| o.0).collect(),
        }
    }
}

/// An error encountered while parsing a CODEOWNERS file. Contains a message
/// describing the error and a `Span` indicating the location of the error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub message: String,
    pub span: Span,
}

impl ParseError {
    fn new(message: impl Into<String>, span: impl Into<Span>) -> ParseError {
        ParseError {
            message: message.into(),
            span: span.into(),
        }
    }
}

/// A span of text in a CODEOWNERS file. Contains the start and end byte offsets
/// of the span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span(pub usize, pub usize);

impl From<(usize, usize)> for Span {
    fn from((start, end): (usize, usize)) -> Self {
        Span(start, end)
    }
}

/// A wrapper around a value that preserves the original source location of the
/// value. Contains the value and a `Span` indicating the location of the value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spanned<T>(pub T, pub Span);

impl<T> Spanned<T> {
    fn new(val: impl Into<T>, span: impl Into<Span>) -> Spanned<T> {
        Spanned(val.into(), span.into())
    }
}

struct Parser<'a> {
    source: &'a str,
    pos: usize,
    errors: Vec<ParseError>,
}

impl<'a> Parser<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            source,
            pos: 0,
            errors: Vec::new(),
        }
    }

    fn parse(mut self) -> ParseResult {
        let mut rules = Vec::new();
        let mut leading_comments = Vec::new();

        // Recoverable errors are added to self.errors during parsing,
        // unrecoverable errors are passed via results
        self.skip_whitespace();
        while let Some(c) = self.peek() {
            match c {
                '\r' | '\n' => {
                    self.next();
                }
                '#' => {
                    let comment = self.parse_comment();
                    leading_comments.push(comment);
                }
                _ => {
                    match self.parse_rule() {
                        Ok(mut rule) => {
                            rule.leading_comments = leading_comments;
                            rules.push(rule)
                        }
                        Err(e) => {
                            self.errors.push(e);
                            break;
                        }
                    }
                    leading_comments = Vec::new();
                }
            }
            self.skip_whitespace();
        }

        ParseResult {
            rules,
            errors: self.errors,
        }
    }

    fn parse_comment(&mut self) -> Spanned<String> {
        let start = self.pos;
        let mut comment = String::new();
        loop {
            match self.peek() {
                Some('\r' | '\n') => break,
                Some(c) => {
                    self.next();
                    comment.push(c);
                }
                None => break,
            }
        }
        Spanned::new(comment, (start, self.pos))
    }

    fn parse_rule(&mut self) -> Result<Rule, ParseError> {
        let pattern = self.parse_pattern();
        if pattern.0.is_empty() {
            return Err(ParseError::new("expected pattern", (self.pos, self.pos)));
        }

        let mut owners = Vec::new();
        loop {
            self.skip_whitespace();
            let Some(owner) = self.parse_owner() else {
                break;
            };
            owners.push(owner);
        }

        // Find pattern terminator (newline, EOF, or #)
        match self.peek() {
            Some('\r' | '\n') | None => Ok(Rule::new(pattern, owners)),
            Some('#') => {
                let trailing_comment = Some(self.parse_comment());
                Ok(Rule {
                    pattern,
                    owners,
                    leading_comments: vec![],
                    trailing_comment,
                })
            }
            _ => Err(ParseError::new("expected newline", (self.pos, self.pos))),
        }
    }

    fn parse_pattern(&mut self) -> Spanned<String> {
        let start = self.pos;
        let mut pattern = String::new();
        let mut escaped = false;
        loop {
            match self.peek() {
                Some('\\') if !escaped => {
                    escaped = true;
                    self.next();
                }
                Some(' ' | '\t' | '#' | '\r' | '\n') if !escaped => break,
                Some(c) => {
                    if c == '\0' {
                        self.errors.push(ParseError::new(
                            "patterns cannot contain null bytes",
                            (self.pos, self.pos + 1),
                        ));
                    }
                    pattern.push(c);
                    self.next();
                    escaped = false;
                }
                None => break,
            }
        }
        Spanned::new(pattern, (start, self.pos))
    }

    fn parse_owner(&mut self) -> Option<Spanned<Owner>> {
        let start = self.pos;
        let mut owner_str = String::new();
        loop {
            match self.peek() {
                Some(' ' | '\t' | '#' | '\r' | '\n') => break,
                Some(c) => {
                    owner_str.push(c);
                    self.next();
                }
                None => break,
            }
        }

        if owner_str.is_empty() {
            return None;
        }

        match Owner::try_from(owner_str) {
            Ok(owner) => Some(Spanned::new(owner, (start, self.pos))),
            Err(err) => {
                self.errors.push(ParseError {
                    message: err.to_string(),
                    span: (start, self.pos).into(),
                });
                None
            }
        }
    }

    fn skip_whitespace(&mut self) {
        while let Some(' ' | '\t') = self.peek() {
            self.next();
        }
    }

    fn peek(&self) -> Option<char> {
        self.source[self.pos..].chars().next()
    }

    fn next(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos += c.len_utf8();
        Some(c)
    }
}

#[cfg(test)]
mod tests {
    use super::ruleset::OwnerKind;
    use super::*;

    #[test]
    fn test_parser() {
        let examples = vec![
            (
                "foo",
                vec![Rule::new(Spanned::new("foo", (0, 3)), vec![])],
                vec![],
            ),
            (
                "foo\\  ",
                vec![Rule::new(Spanned::new("foo ", (0, 5)), vec![])],
                vec![],
            ),
            (
                " foo ",
                vec![Rule::new(Spanned::new("foo", (1, 4)), vec![])],
                vec![],
            ),
            (
                "foo\nbar\r\n \nbaz",
                vec![
                    Rule::new(Spanned::new("foo", (0, 3)), vec![]),
                    Rule::new(Spanned::new("bar", (4, 7)), vec![]),
                    Rule::new(Spanned::new("baz", (11, 14)), vec![]),
                ],
                vec![],
            ),
            (
                "f\0oo",
                vec![Rule::new(Spanned::new("f\0oo", (0, 4)), vec![])],
                vec![ParseError::new(
                    "patterns cannot contain null bytes",
                    (1, 2),
                )],
            ),
            (
                "foo bar",
                vec![Rule::new(Spanned::new("foo", (0, 3)), vec![])],
                vec![ParseError::new("invalid owner: bar", (4, 7))],
            ),
            (
                "foo#abc",
                vec![Rule {
                    pattern: Spanned::new("foo", (0, 3)),
                    owners: Default::default(),
                    leading_comments: Default::default(),
                    trailing_comment: Some(Spanned::new("#abc", (3, 7))),
                }],
                vec![],
            ),
            (
                "foo @bar",
                vec![Rule::new(
                    Spanned::new("foo", (0, 3)),
                    vec![Spanned::new(
                        Owner::new("@bar".to_string(), OwnerKind::User),
                        (4, 8),
                    )],
                )],
                vec![],
            ),
            (
                "a/b @c/d e@f.co",
                vec![Rule::new(
                    Spanned::new("a/b", (0, 3)),
                    vec![
                        Spanned::new(Owner::new("@c/d".to_string(), OwnerKind::Team), (4, 8)),
                        Spanned::new(Owner::new("e@f.co".to_string(), OwnerKind::Email), (9, 15)),
                    ],
                )],
                vec![],
            ),
            (
                "\n foo @bar# baz \n",
                vec![Rule {
                    pattern: Spanned::new("foo", (2, 5)),
                    owners: vec![Spanned::new(
                        Owner::new("@bar".to_string(), OwnerKind::User),
                        (6, 10),
                    )],
                    leading_comments: Default::default(),
                    trailing_comment: Some(Spanned::new("# baz ", (10, 16))),
                }],
                vec![],
            ),
            (
                "# a\nfoo # b\n# c\n# d\n\nbar\n",
                vec![
                    Rule {
                        pattern: Spanned::new("foo", (4, 7)),
                        owners: vec![],
                        leading_comments: vec![Spanned::new("# a", (0, 3))],
                        trailing_comment: Some(Spanned::new("# b", (8, 11))),
                    },
                    Rule {
                        pattern: Spanned::new("bar", (21, 24)),
                        owners: vec![],
                        leading_comments: vec![
                            Spanned::new("# c", (12, 15)),
                            Spanned::new("# d", (16, 19)),
                        ],
                        trailing_comment: None,
                    },
                ],
                vec![],
            ),
        ];

        for (source, rules, errors) in examples {
            assert_eq!(
                Parser::new(source).parse(),
                ParseResult { rules, errors },
                "result mismatch for `{}`",
                source
            );
        }
    }
}
