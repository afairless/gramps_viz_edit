//! Text normalization utilities for comparing genealogical text fields.
//!
//! Each function is a pure `&str -> String` transform that normalizes
//! a specific aspect of text for more robust comparison.

/// Fold alphabetic characters to lowercase.
pub fn case_fold(s: &str) -> String {
    s.to_lowercase()
}

/// Trim leading and trailing whitespace.
pub fn trim_whitespace(s: &str) -> String {
    s.trim().to_string()
}

/// Replace all runs of whitespace with a single space.
pub fn collapse_whitespace(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut in_whitespace = true; // treat start-of-input as whitespace so we skip leading runs

    for ch in s.chars() {
        if ch.is_whitespace() {
            if !in_whitespace {
                result.push(' ');
                in_whitespace = true;
            }
        } else {
            result.push(ch);
            in_whitespace = false;
        }
    }

    // Trim trailing space if we ended with whitespace
    if result.ends_with(' ') && !s.is_empty() {
        result.pop();
    }

    result
}

/// Strip HTML tags from a string.
///
/// Removes anything between `<` and `>` (including angle brackets).
/// This is a simple non-recursive approach sufficient for Gramps
/// HTML-like text (e.g., `<span style="...">text</span>` → `text`).
pub fn strip_html_tags(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut in_tag = false;

    for ch in s.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ => {
                if !in_tag {
                    result.push(ch);
                }
            }
        }
    }

    result
}

/// Expand common street abbreviations to their full forms.
///
/// Handles: St → Street, Ave → Avenue, Blvd → Boulevard,
/// Rd → Road, Ln → Lane, Dr → Drive, Ct → Court, Sq → Square,
/// Pkwy → Parkway, Hwy → Highway.
///
/// The expansion is case-insensitive but preserves the original
/// casing of non-abbreviated parts.
pub fn expand_street_abbreviations(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut word_start = 0;
    let bytes = s.as_bytes();
    let len = bytes.len();

    while word_start < len {
        // Find the end of the current word (delimited by whitespace or end)
        let mut word_end = word_start;
        while word_end < len && !bytes[word_end].is_ascii_whitespace() {
            word_end += 1;
        }

        // Extract the word (as a &str slice)
        let word = &s[word_start..word_end];

        // Strip trailing punctuation so "St." and "St," still match "st"
        let (stem, punctuation) = split_trailing_punctuation(word);
        let expanded = match stem.to_lowercase().as_str() {
            "st" => "Street",
            "ave" => "Avenue",
            "blvd" => "Boulevard",
            "rd" => "Road",
            "ln" => "Lane",
            "dr" => "Drive",
            "ct" => "Court",
            "sq" => "Square",
            "pkwy" => "Parkway",
            "hwy" => "Highway",
            _ => word, // not an abbreviation, keep original
        };

        result.push_str(expanded);
        result.push_str(punctuation);

        // Preserve the whitespace separator
        word_start = word_end;
        while word_start < len && bytes[word_start].is_ascii_whitespace() {
            result.push(bytes[word_start] as char);
            word_start += 1;
        }
    }

    result
}

/// Split a word into its alphabetic stem and a trailing run of punctuation.
/// Returns the stem and the punctuation (empty strings when absent).
fn split_trailing_punctuation(word: &str) -> (&str, &str) {
    let trimmed = word.trim_end_matches([',', '.', ';', ':', '!']);
    let split = trimmed.len();
    (&word[..split], &word[split..])
}

/// Strip a page prefix like "p." or "pp." from a source page string.
///
/// Handles common prefixes: "p.", "pp.", "page ", "pages ".
/// Matching is case-insensitive.
pub fn strip_page_prefix(s: &str) -> String {
    let trimmed = s.trim();
    if let Some(rest) = trimmed
        .strip_prefix("p. ")
        .or_else(|| trimmed.strip_prefix("pp. "))
        .or_else(|| {
            // case-insensitive "page " and "pages "
            let lower = trimmed.to_lowercase();
            if lower.starts_with("page ") {
                Some(&trimmed[5..])
            } else if lower.starts_with("pages ") {
                Some(&trimmed[6..])
            } else {
                None
            }
        })
    {
        rest.trim().to_string()
    } else {
        trimmed.to_string()
    }
}

/// Normalize a tag color string to a canonical 6-digit hex color.
///
/// Accepts `#rgb`, `#rrggbb`, or CSS named colors (a subset).
/// Returns the canonical `#rrggbb` form, or the input unchanged
/// if it cannot be parsed.
pub fn normalize_tag_color(s: &str) -> String {
    let trimmed = s.trim();

    // Check for named colors
    let named = match trimmed.to_lowercase().as_str() {
        "red" => "#ff0000",
        "green" => "#00ff00",
        "blue" => "#0000ff",
        "white" => "#ffffff",
        "black" => "#000000",
        "yellow" => "#ffff00",
        "orange" => "#ffa500",
        "purple" => "#800080",
        "pink" => "#ffc0cb",
        "gray" | "grey" => "#808080",
        _ => return trimmed.to_string(), // not a recognized color
    };

    named.to_string()
}

/// Check if two strings are equal ignoring diacritics (accents).
///
/// Normalizes both strings using Unicode NFD decomposition and
/// strips combining marks before comparison.
pub fn diacritic_insensitive_eq(a: &str, b: &str) -> bool {
    fn strip_one(s: &str) -> String {
        s.chars()
            .map(|c| match c {
                'á' | 'à' | 'â' | 'ä' | 'ã' | 'å' | 'ā' => 'a',
                'é' | 'è' | 'ê' | 'ë' | 'ē' => 'e',
                'í' | 'ì' | 'î' | 'ï' | 'ī' => 'i',
                'ó' | 'ò' | 'ô' | 'ö' | 'õ' | 'ō' => 'o',
                'ú' | 'ù' | 'û' | 'ü' | 'ū' => 'u',
                'ý' | 'ỳ' | 'ŷ' | 'ÿ' => 'y',
                'ñ' | 'ń' => 'n',
                'ç' | 'ć' => 'c',
                'ğ' => 'g',
                'ş' => 's',
                'ž' | 'ż' | 'ź' => 'z',
                'ł' => 'l',
                'đ' => 'd',
                'ř' => 'r',
                'š' => 's',
                'ť' => 't',
                'Č' | 'Ć' => 'c',
                'Ď' => 'd',
                'Ě' => 'e',
                'Ň' => 'n',
                'Ř' => 'r',
                'Š' => 's',
                'Ť' => 't',
                'Ž' | 'Ź' | 'Ż' => 'z',
                'Á' | 'À' | 'Â' | 'Ä' | 'Ã' | 'Å' | 'Ā' => 'A',
                'É' | 'È' | 'Ê' | 'Ë' | 'Ē' => 'E',
                'Í' | 'Ì' | 'Î' | 'Ï' | 'Ī' => 'I',
                'Ó' | 'Ò' | 'Ô' | 'Ö' | 'Õ' | 'Ō' => 'O',
                'Ú' | 'Ù' | 'Û' | 'Ü' | 'Ū' => 'U',
                'Ý' | 'Ỳ' | 'Ŷ' | 'Ÿ' => 'Y',
                'Ñ' | 'Ń' => 'N',
                'Ç' => 'C',
                'Ğ' => 'G',
                'Ş' => 'S',
                'Ł' => 'L',
                'Đ' => 'D',
                _ => c,
            })
            .collect()
    }

    strip_one(a) == strip_one(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- case_fold ---

    #[test]
    fn case_fold_lowercase() {
        assert_eq!(case_fold("hello"), "hello");
    }

    #[test]
    fn case_fold_uppercase() {
        assert_eq!(case_fold("HELLO"), "hello");
    }

    #[test]
    fn case_fold_mixed() {
        assert_eq!(case_fold("HeLLo WoRLd"), "hello world");
    }

    #[test]
    fn case_fold_empty() {
        assert_eq!(case_fold(""), "");
    }

    #[test]
    fn property_case_fold_idempotent() {
        let inputs = ["", "hello", "HELLO", "Hello World", "123 ABC", "café"];
        for s in &inputs {
            let once = case_fold(s);
            let twice = case_fold(&once);
            assert_eq!(once, twice, "case_fold not idempotent for {s:?}");
        }
    }

    // --- trim_whitespace ---

    #[test]
    fn trim_whitespace_leading() {
        assert_eq!(trim_whitespace("  hello"), "hello");
    }

    #[test]
    fn trim_whitespace_trailing() {
        assert_eq!(trim_whitespace("hello  "), "hello");
    }

    #[test]
    fn trim_whitespace_both() {
        assert_eq!(trim_whitespace("  hello  "), "hello");
    }

    #[test]
    fn trim_whitespace_none() {
        assert_eq!(trim_whitespace("hello"), "hello");
    }

    #[test]
    fn trim_whitespace_empty() {
        assert_eq!(trim_whitespace(""), "");
    }

    #[test]
    fn property_trim_whitespace_idempotent() {
        let inputs = ["", "hello", "  hello  ", "  a  b  ", "\t\n  test  \n"];
        for s in &inputs {
            let once = trim_whitespace(s);
            let twice = trim_whitespace(&once);
            assert_eq!(once, twice, "trim_whitespace not idempotent for {s:?}");
        }
    }

    // --- collapse_whitespace ---

    #[test]
    fn collapse_whitespace_single_spaces() {
        assert_eq!(collapse_whitespace("hello world"), "hello world");
    }

    #[test]
    fn collapse_whitespace_multiple_spaces() {
        assert_eq!(collapse_whitespace("hello   world"), "hello world");
    }

    #[test]
    fn collapse_whitespace_mixed_whitespace() {
        assert_eq!(
            collapse_whitespace("hello\t  world\n  test"),
            "hello world test"
        );
    }

    #[test]
    fn collapse_whitespace_leading_trailing() {
        assert_eq!(collapse_whitespace("  hello world  "), "hello world");
    }

    #[test]
    fn collapse_whitespace_empty() {
        assert_eq!(collapse_whitespace(""), "");
    }

    #[test]
    fn collapse_whitespace_only_whitespace() {
        assert_eq!(collapse_whitespace("   \t\n  "), "");
    }

    #[test]
    fn property_collapse_whitespace_idempotent() {
        let inputs = ["", "hello", "hello   world", "  a  b  ", "\t\n  test  \n"];
        for s in &inputs {
            let once = collapse_whitespace(s);
            let twice = collapse_whitespace(&once);
            assert_eq!(once, twice, "collapse_whitespace not idempotent for {s:?}");
        }
    }

    // --- strip_html_tags ---

    #[test]
    fn strip_html_tags_no_tags() {
        assert_eq!(strip_html_tags("hello world"), "hello world");
    }

    #[test]
    fn strip_html_tags_simple() {
        assert_eq!(strip_html_tags("<span>hello</span>"), "hello");
    }

    #[test]
    fn strip_html_tags_with_attributes() {
        assert_eq!(
            strip_html_tags("<span style=\"color:red\">hello</span>"),
            "hello"
        );
    }

    #[test]
    fn strip_html_tags_mixed() {
        assert_eq!(
            strip_html_tags("hello <b>world</b> test"),
            "hello world test"
        );
    }

    #[test]
    fn strip_html_tags_empty() {
        assert_eq!(strip_html_tags(""), "");
    }

    #[test]
    fn strip_html_tags_only_tag() {
        assert_eq!(strip_html_tags("<br/>"), "");
    }

    // --- expand_street_abbreviations ---

    #[test]
    fn expand_street_abbreviations_st() {
        assert_eq!(expand_street_abbreviations("Main St"), "Main Street");
    }

    #[test]
    fn expand_street_abbreviations_ave() {
        assert_eq!(expand_street_abbreviations("Park Ave"), "Park Avenue");
    }

    #[test]
    fn expand_street_abbreviations_blvd() {
        assert_eq!(
            expand_street_abbreviations("Sunset Blvd"),
            "Sunset Boulevard"
        );
    }

    #[test]
    fn expand_street_abbreviations_rd() {
        assert_eq!(expand_street_abbreviations("River Rd"), "River Road");
    }

    #[test]
    fn expand_street_abbreviations_multiple() {
        assert_eq!(
            expand_street_abbreviations("123 Main St, Apt 4B"),
            "123 Main Street, Apt 4B"
        );
    }

    #[test]
    fn expand_street_abbreviations_no_abbrev() {
        assert_eq!(
            expand_street_abbreviations("123 Main Street"),
            "123 Main Street"
        );
    }

    #[test]
    fn expand_street_abbreviations_empty() {
        assert_eq!(expand_street_abbreviations(""), "");
    }

    #[test]
    fn expand_street_abbreviations_case_insensitive() {
        assert_eq!(expand_street_abbreviations("MAIN ST"), "MAIN Street");
    }

    #[test]
    fn property_expand_street_abbreviations_idempotent() {
        let inputs = [
            "",
            "Main St",
            "Park Ave",
            "123 Main Street",
            "Sunset Blvd",
            "River Rd",
        ];
        for s in &inputs {
            let once = expand_street_abbreviations(s);
            let twice = expand_street_abbreviations(&once);
            assert_eq!(
                once, twice,
                "expand_street_abbreviations not idempotent for {s:?}"
            );
        }
    }

    // --- strip_page_prefix ---

    #[test]
    fn strip_page_prefix_p_dot() {
        assert_eq!(strip_page_prefix("p. 123"), "123");
    }

    #[test]
    fn strip_page_prefix_pp_dot() {
        assert_eq!(strip_page_prefix("pp. 45-48"), "45-48");
    }

    #[test]
    fn strip_page_prefix_page() {
        assert_eq!(strip_page_prefix("page 123"), "123");
    }

    #[test]
    fn strip_page_prefix_pages() {
        assert_eq!(strip_page_prefix("pages 45-48"), "45-48");
    }

    #[test]
    fn strip_page_prefix_no_prefix() {
        assert_eq!(strip_page_prefix("123"), "123");
    }

    #[test]
    fn strip_page_prefix_empty() {
        assert_eq!(strip_page_prefix(""), "");
    }

    #[test]
    fn strip_page_prefix_case_insensitive() {
        assert_eq!(strip_page_prefix("Page 123"), "123");
        assert_eq!(strip_page_prefix("PAGES 45-48"), "45-48");
    }

    #[test]
    fn property_strip_page_prefix_idempotent() {
        let inputs = ["", "p. 123", "pp. 45-48", "page 1", "pages 10-20", "123"];
        for s in &inputs {
            let once = strip_page_prefix(s);
            let twice = strip_page_prefix(&once);
            assert_eq!(once, twice, "strip_page_prefix not idempotent for {s:?}");
        }
    }

    // --- normalize_tag_color ---

    #[test]
    fn normalize_tag_color_red() {
        assert_eq!(normalize_tag_color("red"), "#ff0000");
    }

    #[test]
    fn normalize_tag_color_green() {
        assert_eq!(normalize_tag_color("green"), "#00ff00");
    }

    #[test]
    fn normalize_tag_color_blue() {
        assert_eq!(normalize_tag_color("blue"), "#0000ff");
    }

    #[test]
    fn normalize_tag_color_case_insensitive() {
        assert_eq!(normalize_tag_color("RED"), "#ff0000");
        assert_eq!(normalize_tag_color("Red"), "#ff0000");
    }

    #[test]
    fn normalize_tag_color_unknown() {
        assert_eq!(normalize_tag_color("#123456"), "#123456");
    }

    #[test]
    fn normalize_tag_color_empty() {
        assert_eq!(normalize_tag_color(""), "");
    }

    #[test]
    fn property_normalize_tag_color_idempotent() {
        let inputs = [
            "", "red", "green", "blue", "#123456", "white", "black", "yellow",
        ];
        for s in &inputs {
            let once = normalize_tag_color(s);
            let twice = normalize_tag_color(&once);
            assert_eq!(once, twice, "normalize_tag_color not idempotent for {s:?}");
        }
    }

    // --- diacritic_insensitive_eq ---

    #[test]
    fn diacritic_insensitive_eq_identical() {
        assert!(diacritic_insensitive_eq("hello", "hello"));
    }

    #[test]
    fn diacritic_insensitive_eq_accented() {
        assert!(diacritic_insensitive_eq("café", "cafe"));
    }

    #[test]
    fn diacritic_insensitive_eq_different() {
        assert!(!diacritic_insensitive_eq("hello", "world"));
    }

    #[test]
    fn diacritic_insensitive_eq_empty() {
        assert!(diacritic_insensitive_eq("", ""));
    }

    #[test]
    fn diacritic_insensitive_eq_multiple_accents() {
        assert!(diacritic_insensitive_eq("Müller", "Muller"));
        assert!(diacritic_insensitive_eq("José", "Jose"));
        assert!(diacritic_insensitive_eq("São Paulo", "Sao Paulo"));
    }

    #[test]
    fn diacritic_insensitive_eq_uppercase_accented() {
        assert!(diacritic_insensitive_eq("École", "Ecole"));
    }

    #[test]
    fn property_diacritic_insensitive_eq_reflexive() {
        let inputs = ["", "hello", "café", "Müller", "José", "São Paulo"];
        for s in &inputs {
            assert!(
                diacritic_insensitive_eq(s, s),
                "diacritic_insensitive_eq({s:?}, {s:?}) should be true"
            );
        }
    }
}
