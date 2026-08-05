//! Text similarity scoring functions.
//!
//! All functions return normalized scores in the range `[0.0, 1.0]`, where
//! `1.0` means identical and `0.0` means completely different.

use std::collections::BTreeSet;

/// Normalized Levenshtein edit distance.
///
/// Returns `1.0` for identical strings, `0.0` for completely different.
/// The score is computed as:
///
/// ```text
/// score = 1.0 - (edit_distance / max(len_a, len_b))
/// ```
///
/// When both strings are empty, returns `1.0`.
pub fn levenshtein(a: &str, b: &str) -> f64 {
    let max_len = a.len().max(b.len());
    if max_len == 0 {
        return 1.0;
    }
    let dist = strsim::levenshtein(a, b) as f64;
    1.0 - (dist / max_len as f64)
}

/// Jaccard similarity on word tokens.
///
/// Tokenizes each string by whitespace and computes the Jaccard index
/// of the resulting token sets. Returns `1.0` when both strings are empty.
pub fn token_jaccard(a: &str, b: &str) -> f64 {
    let tokens_a: BTreeSet<&str> = a.split_whitespace().collect();
    let tokens_b: BTreeSet<&str> = b.split_whitespace().collect();

    let intersection = tokens_a.intersection(&tokens_b).count();
    let union = tokens_a.union(&tokens_b).count();

    if union == 0 {
        return 1.0;
    }
    intersection as f64 / union as f64
}

/// Tokenized normalized Levenshtein similarity.
///
/// Tokenizes each string by whitespace, then computes the pairwise
/// normalized Levenshtein score for corresponding tokens. The result
/// is the average of all pairwise scores.
///
/// Returns `1.0` when both strings are empty.
pub fn tokenized_levenshtein(a: &str, b: &str) -> f64 {
    let tokens_a: Vec<&str> = a.split_whitespace().collect();
    let tokens_b: Vec<&str> = b.split_whitespace().collect();

    let max_len = tokens_a.len().max(tokens_b.len());
    if max_len == 0 {
        return 1.0;
    }

    let mut total = 0.0;
    let mut count = 0;

    for i in 0..max_len {
        let ta = tokens_a.get(i).copied().unwrap_or("");
        let tb = tokens_b.get(i).copied().unwrap_or("");
        total += levenshtein(ta, tb);
        count += 1;
    }

    total / count as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- levenshtein ---

    #[test]
    fn levenshtein_identical() {
        assert!((levenshtein("hello", "hello") - 1.0).abs() < 1e-9);
    }

    #[test]
    fn levenshtein_completely_different() {
        assert!((levenshtein("abc", "xyz") - 0.0).abs() < 1e-9);
    }

    #[test]
    fn levenshtein_partial_overlap() {
        let score = levenshtein("hello", "hallo");
        assert!(score > 0.0 && score < 1.0);
        // 1 edit out of 5 → 0.8
        assert!((score - 0.8).abs() < 1e-9);
    }

    #[test]
    fn levenshtein_empty_strings() {
        assert!((levenshtein("", "") - 1.0).abs() < 1e-9);
    }

    #[test]
    fn levenshtein_one_empty() {
        let score = levenshtein("hello", "");
        assert!((score - 0.0).abs() < 1e-9);
    }

    #[test]
    fn levenshtein_unicode() {
        let score = levenshtein("café", "cafe");
        assert!(score > 0.0 && score < 1.0);
    }

    #[test]
    fn levenshtein_score_in_range() {
        for (a, b) in &[
            ("", ""),
            ("a", ""),
            ("", "a"),
            ("abc", "abc"),
            ("abc", "xyz"),
            ("hello world", "hello there"),
            ("", "hello"),
        ] {
            let score = levenshtein(a, b);
            assert!(
                score >= 0.0 && score <= 1.0,
                "levenshtein({a:?}, {b:?}) = {score} not in [0, 1]"
            );
        }
    }

    // --- token_jaccard ---

    #[test]
    fn token_jaccard_identical() {
        assert!((token_jaccard("hello world", "hello world") - 1.0).abs() < 1e-9);
    }

    #[test]
    fn token_jaccard_completely_different() {
        assert!((token_jaccard("foo bar", "baz qux") - 0.0).abs() < 1e-9);
    }

    #[test]
    fn token_jaccard_partial_overlap() {
        // {hello, world} ∩ {hello, there} = {hello} → 1/3 ≈ 0.333
        let score = token_jaccard("hello world", "hello there");
        assert!((score - 1.0 / 3.0).abs() < 1e-9);
    }

    #[test]
    fn token_jaccard_empty_strings() {
        assert!((token_jaccard("", "") - 1.0).abs() < 1e-9);
    }

    #[test]
    fn token_jaccard_one_empty() {
        assert!((token_jaccard("hello world", "") - 0.0).abs() < 1e-9);
    }

    #[test]
    fn token_jaccard_score_in_range() {
        for (a, b) in &[
            ("", ""),
            ("a", ""),
            ("", "a"),
            ("a b", "a b"),
            ("a b", "c d"),
            ("a b c", "a d"),
        ] {
            let score = token_jaccard(a, b);
            assert!(
                score >= 0.0 && score <= 1.0,
                "token_jaccard({a:?}, {b:?}) = {score} not in [0, 1]"
            );
        }
    }

    // --- tokenized_levenshtein ---

    #[test]
    fn tokenized_levenshtein_identical() {
        assert!(
            (tokenized_levenshtein("hello world", "hello world") - 1.0).abs() < 1e-9
        );
    }

    #[test]
    fn tokenized_levenshtein_completely_different() {
        assert!((tokenized_levenshtein("abc", "xyz") - 0.0).abs() < 1e-9);
    }

    #[test]
    fn tokenized_levenshtein_partial_overlap() {
        // "hello world" vs "hallo world" → leven(hello, hallo)=0.8, leven(world, world)=1.0 → avg 0.9
        let score = tokenized_levenshtein("hello world", "hallo world");
        assert!((score - 0.9).abs() < 1e-9);
    }

    #[test]
    fn tokenized_levenshtein_empty_strings() {
        assert!((tokenized_levenshtein("", "") - 1.0).abs() < 1e-9);
    }

    #[test]
    fn tokenized_levenshtein_one_empty() {
        let score = tokenized_levenshtein("hello world", "");
        assert!((score - 0.0).abs() < 1e-9);
    }

    #[test]
    fn tokenized_levenshtein_different_token_counts() {
        let score = tokenized_levenshtein("hello world", "hello");
        // "hello"↔"hello" = 1.0, "world"↔"" = 0.0 → avg 0.5
        assert!((score - 0.5).abs() < 1e-9);
    }

    #[test]
    fn tokenized_levenshtein_score_in_range() {
        for (a, b) in &[
            ("", ""),
            ("a", ""),
            ("", "a"),
            ("a b", "a b"),
            ("a b", "c d"),
            ("a b c", "a d"),
        ] {
            let score = tokenized_levenshtein(a, b);
            assert!(
                score >= 0.0 && score <= 1.0,
                "tokenized_levenshtein({a:?}, {b:?}) = {score} not in [0, 1]"
            );
        }
    }

    // --- property-based ---

    #[test]
    fn property_identical_inputs_score_one() {
        let inputs = ["", "a", "hello", "hello world", "café", "a b c d e"];
        for s in &inputs {
            assert!(
                (levenshtein(s, s) - 1.0).abs() < 1e-9,
                "levenshtein({s:?}, {s:?}) != 1.0"
            );
            assert!(
                (token_jaccard(s, s) - 1.0).abs() < 1e-9,
                "token_jaccard({s:?}, {s:?}) != 1.0"
            );
            assert!(
                (tokenized_levenshtein(s, s) - 1.0).abs() < 1e-9,
                "tokenized_levenshtein({s:?}, {s:?}) != 1.0"
            );
        }
    }

    #[test]
    fn property_scores_in_zero_one() {
        let pairs = [
            ("", ""),
            ("a", "b"),
            ("abc", "def"),
            ("hello world", "goodbye world"),
            ("", "nonempty"),
            ("nonempty", ""),
            ("a b c", "d e f"),
            ("hello", "hallo"),
            ("café", "cafe"),
        ];
        for (a, b) in &pairs {
            for (name, func) in [
                ("levenshtein", levenshtein as fn(&str, &str) -> f64),
                ("token_jaccard", token_jaccard as fn(&str, &str) -> f64),
                (
                    "tokenized_levenshtein",
                    tokenized_levenshtein as fn(&str, &str) -> f64,
                ),
            ] {
                let score = func(a, b);
                assert!(
                    score >= 0.0 && score <= 1.0,
                    "{name}({a:?}, {b:?}) = {score} not in [0, 1]"
                );
            }
        }
    }
}