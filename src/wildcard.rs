pub fn matches(pattern: &str, candidate: &str) -> bool {
    let mut pattern_idx = 0;
    let mut candidate_idx = 0;
    let mut reset_indices: Option<(usize, usize)> = None;
    while pattern_idx < pattern.len() || candidate_idx < candidate.len() {
        if let Some(pattern_char) = char_at(pattern, pattern_idx) {
            match pattern_char {
                '*' => {
                    let inc = char_at(candidate, candidate_idx)
                        .map(|c| c.len_utf8())
                        .unwrap_or(1);
                    reset_indices = Some((pattern_idx, candidate_idx + inc));
                    pattern_idx += pattern_char.len_utf8();
                    continue;
                }
                '?' => {
                    if let Some(candidate_char) = char_at(candidate, candidate_idx) {
                        pattern_idx += pattern_char.len_utf8();
                        candidate_idx += candidate_char.len_utf8();
                        continue;
                    }
                }
                _ => {
                    if let Some(candidate_char) = char_at(candidate, candidate_idx) {
                        if pattern_char == candidate_char {
                            pattern_idx += pattern_char.len_utf8();
                            candidate_idx += candidate_char.len_utf8();
                            continue;
                        }
                    }
                }
            }
        }

        if let Some((new_pattern_idx, new_candidate_idx)) = reset_indices {
            if new_candidate_idx <= candidate.len() {
                pattern_idx = new_pattern_idx;
                candidate_idx = new_candidate_idx;
                continue;
            }
        }

        return false;
    }

    true
}

fn char_at(s: &str, idx: usize) -> Option<char> {
    // This should be faster than s.chars().nth(idx) (constant time vs linear time),
    // but it'll panic if idx isn't a unicode character boundary.
    s[idx..].chars().next()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_literal() {
        assert!(matches("", ""));
        assert!(matches("foo", "foo"));
        assert!(!matches("fo", "foo"));
        assert!(!matches("foo", "fo"));
    }

    #[test]
    fn test_question_mark() {
        assert!(matches("?", "a"));
        assert!(matches("f??", "foo"));
        assert!(!matches("b??", "foo"));
        assert!(!matches("?b", "foo"));
    }

    #[test]
    fn test_star() {
        assert!(matches("*", ""));
        assert!(matches("*", "foo"));
        assert!(matches("f*", "foo"));
        assert!(matches("*o", "foo"));
        assert!(matches("f*o", "foo"));
        assert!(matches("fo*o", "foo"));
        assert!(matches("foo*", "foo"));
        assert!(matches("*foo", "foo"));
        assert!(!matches("b*", "foo"));
        assert!(!matches("*b", "a"));
        assert!(!matches("*b", "foo"));
    }

    #[test]
    fn test_exponential_match() {
        let mut pat = "a*".repeat(10);
        pat.push('b');
        let mut cand = "a".repeat(100);
        assert!(!matches(&pat, &cand));
    }
}
