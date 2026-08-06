//! Interactive resolution for ambiguous match cases.
//!
//! This module provides a terminal-based interactive resolution loop that
//! presents each ambiguous match case side-by-side and lets the user pick
//! the correct candidate or mark it as unresolved.
//!
//! # Feature gate
//!
//! This module is gated behind the `resolve` feature, which depends on
//! `crossterm` for terminal manipulation.

use std::collections::HashMap;
use std::fs;
use std::io::{self, Write};
use std::path::Path;

use crate::report::AmbiguousCase;

use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use crossterm::{cursor, execute, terminal};

/// The result of resolving all ambiguous cases.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ResolvedMatches {
    /// Map of A-handle → resolved match for successfully resolved cases.
    pub resolved: HashMap<String, ResolvedMatch>,
    /// Handles from graph A that were left unresolved.
    pub unresolved: Vec<String>,
}

/// A single resolved match for an ambiguous case.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ResolvedMatch {
    /// The matched handle in graph B.
    pub handle_b: String,
    /// Confidence score from the matcher (0.0–1.0).
    pub confidence: f64,
}

/// Run the interactive resolution loop.
///
/// Presents each ambiguous case from `cases` in a terminal TUI, showing
/// side-by-side details of the item from graph A and each candidate from
/// graph B. The user can:
///
/// - Enter a number (1-indexed) to select that candidate
/// - Enter `s` to skip this case (leave unresolved)
/// - Enter `q` to quit early (remaining cases are left unresolved)
///
/// Results are saved incrementally to `resolutions_file` after each decision.
/// If `resolutions_file` already exists, it is loaded and any previously
/// resolved cases are skipped.
///
/// # Panics
///
/// Panics if terminal operations fail (e.g., not a TTY).
pub fn run_interactive_resolution(
    cases: &[AmbiguousCase],
    resolutions_file: &str,
) -> ResolvedMatches {
    // Try to load existing resolutions
    let mut resolved: HashMap<String, ResolvedMatch> = HashMap::new();
    let mut unresolved: Vec<String> = Vec::new();

    if Path::new(resolutions_file).exists() {
        if let Ok(loaded) = load_resolutions(resolutions_file) {
            resolved = loaded.resolved;
            unresolved = loaded.unresolved;
        }
    }

    let mut stdout = io::stdout();
    let stdin = io::stdin();

    // Enable raw mode for cleaner terminal interaction
    enable_raw_mode().expect("enable raw mode");
    let _cleanup = RawModeGuard;

    for (i, case) in cases.iter().enumerate() {
        // Skip already-resolved cases
        if resolved.contains_key(&case.handle_a) || unresolved.contains(&case.handle_a) {
            continue;
        }

        // Clear screen and show header
        execute!(
            stdout,
            terminal::Clear(terminal::ClearType::All),
            cursor::MoveTo(0, 0)
        )
        .expect("clear screen");

        writeln!(
            stdout,
            "Ambiguous case {}/{} — {} ({})",
            i + 1,
            cases.len(),
            case.context_a.display_name,
            case.item_type_a
        )
        .expect("write header");

        writeln!(stdout, "{}", "─".repeat(80)).expect("write separator");

        // Show item A details
        writeln!(stdout, "\nItem A (source):").expect("write section header");
        writeln!(stdout, "  Handle: {}", case.handle_a).expect("write handle");
        writeln!(stdout, "  Display: {}", case.context_a.display_name).expect("write display name");
        if !case.context_a.related_items.is_empty() {
            writeln!(stdout, "  Related items:").expect("write related header");
            for rel in &case.context_a.related_items {
                writeln!(
                    stdout,
                    "    • {} ({}) — {}",
                    rel.display_name, rel.relationship, rel.handle
                )
                .expect("write related item");
            }
        }

        // Show candidates
        writeln!(stdout, "\nCandidates (graph B):").expect("write candidates header");
        if case.candidates.is_empty() {
            writeln!(stdout, "  (no candidates)").expect("write no candidates");
        } else {
            for (j, candidate) in case.candidates.iter().enumerate() {
                writeln!(
                    stdout,
                    "\n  [{idx}] Score: {score:.2}",
                    idx = j + 1,
                    score = candidate.score
                )
                .expect("write candidate number");
                writeln!(stdout, "       Handle: {}", candidate.handle_b)
                    .expect("write candidate handle");
                writeln!(
                    stdout,
                    "       Display: {}",
                    candidate.context_b.display_name
                )
                .expect("write candidate display");
                if !candidate.context_b.related_items.is_empty() {
                    writeln!(stdout, "       Related items:").expect("write related");
                    for rel in &candidate.context_b.related_items {
                        writeln!(
                            stdout,
                            "         • {} ({}) — {}",
                            rel.display_name, rel.relationship, rel.handle
                        )
                        .expect("write related item");
                    }
                }
            }
        }

        // Prompt for user input
        writeln!(stdout, "\n{}", "─".repeat(80)).expect("write separator");
        write!(
            stdout,
            "Select candidate [1-{n}], s=skip, q=quit: ",
            n = case.candidates.len()
        )
        .expect("write prompt");
        stdout.flush().expect("flush stdout");

        // Read user input
        let mut input = String::new();
        stdin.read_line(&mut input).expect("read input");
        let input = input.trim().to_lowercase();

        match input.as_str() {
            "q" => {
                // Quit — mark remaining as unresolved
                unresolved.push(case.handle_a.clone());
                // All remaining cases after this one are also unresolved
                for remaining in &cases[i + 1..] {
                    if !resolved.contains_key(&remaining.handle_a)
                        && !unresolved.contains(&remaining.handle_a)
                    {
                        unresolved.push(remaining.handle_a.clone());
                    }
                }
                break;
            }
            "s" => {
                unresolved.push(case.handle_a.clone());
            }
            num_str => {
                if let Ok(idx) = num_str.parse::<usize>() {
                    if idx >= 1 && idx <= case.candidates.len() {
                        let candidate = &case.candidates[idx - 1];
                        resolved.insert(
                            case.handle_a.clone(),
                            ResolvedMatch {
                                handle_b: candidate.handle_b.clone(),
                                confidence: candidate.score,
                            },
                        );
                    } else {
                        // Invalid number — skip
                        unresolved.push(case.handle_a.clone());
                    }
                } else {
                    // Unrecognized input — skip
                    unresolved.push(case.handle_a.clone());
                }
            }
        }

        // Save incrementally
        save_resolutions(
            resolutions_file,
            &ResolvedMatches {
                resolved: resolved.clone(),
                unresolved: unresolved.clone(),
            },
        );
    }

    ResolvedMatches {
        resolved,
        unresolved,
    }
}

/// Load resolutions from a JSON file.
pub fn load_resolutions(path: &str) -> Result<ResolvedMatches, String> {
    let content = fs::read_to_string(path).map_err(|e| format!("cannot read {path}: {e}"))?;
    serde_json::from_str(&content).map_err(|e| format!("cannot parse {path}: {e}"))
}

/// Save resolutions to a JSON file.
pub fn save_resolutions(path: &str, matches: &ResolvedMatches) {
    if let Some(parent) = Path::new(path).parent() {
        let _ = fs::create_dir_all(parent);
    }
    let json = serde_json::to_string_pretty(matches).expect("serialize resolutions");
    fs::write(path, json).expect("write resolutions file");
}

/// RAII guard to disable raw mode on drop.
struct RawModeGuard;

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::report::{AmbiguousContext, RelatedItem};

    #[test]
    fn resolve_roundtrip_empty() {
        let matches = ResolvedMatches {
            resolved: HashMap::new(),
            unresolved: vec![],
        };

        let json = serde_json::to_string(&matches).expect("serialize");
        let deserialized: ResolvedMatches = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(matches, deserialized);
    }

    #[test]
    fn resolve_roundtrip_with_data() {
        let mut resolved = HashMap::new();
        resolved.insert(
            "H001".into(),
            ResolvedMatch {
                handle_b: "H010".into(),
                confidence: 0.85,
            },
        );
        resolved.insert(
            "H002".into(),
            ResolvedMatch {
                handle_b: "H020".into(),
                confidence: 0.92,
            },
        );

        let matches = ResolvedMatches {
            resolved,
            unresolved: vec!["H003".into(), "H004".into()],
        };

        let json = serde_json::to_string_pretty(&matches).expect("serialize");
        let deserialized: ResolvedMatches = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(matches, deserialized);
    }

    #[test]
    fn load_resolutions_nonexistent_file() {
        let result = load_resolutions("/nonexistent/path/.gramps-diff-resolve.json");
        assert!(result.is_err());
    }

    #[test]
    fn load_resolutions_invalid_json() {
        let mut dir = std::env::temp_dir();
        dir.push("gramps_diff_resolve_test");
        let _ = std::fs::create_dir_all(&dir);
        let mut path = dir.clone();
        path.push("invalid.json");
        let mut file = std::fs::File::create(&path).expect("create file");
        file.write_all(b"not json").expect("write");

        let result = load_resolutions(path.to_str().unwrap());
        assert!(result.is_err());

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn save_resolutions_creates_file() {
        let mut dir = std::env::temp_dir();
        dir.push("gramps_diff_resolve_test");
        let _ = std::fs::create_dir_all(&dir);
        let mut path = dir.clone();
        path.push("save_test.json");

        let matches = ResolvedMatches {
            resolved: HashMap::new(),
            unresolved: vec!["H001".into()],
        };

        save_resolutions(path.to_str().unwrap(), &matches);

        let loaded = load_resolutions(path.to_str().unwrap()).expect("load saved");
        assert_eq!(matches, loaded);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn resolve_matches_serde_roundtrip() {
        // Verify that ResolvedMatches and ResolvedMatch are serializable
        // with the same types used in the report module.
        let matches = ResolvedMatches {
            resolved: [(
                "A001".to_string(),
                ResolvedMatch {
                    handle_b: "B001".to_string(),
                    confidence: 0.95,
                },
            )]
            .into_iter()
            .collect(),
            unresolved: vec![],
        };

        let json = serde_json::to_string(&matches).unwrap();
        let back: ResolvedMatches = serde_json::from_str(&json).unwrap();
        assert_eq!(matches, back);
    }

    #[test]
    fn resolve_matches_contains_handle_a_keys() {
        let matches = ResolvedMatches {
            resolved: [(
                "A001".to_string(),
                ResolvedMatch {
                    handle_b: "B001".to_string(),
                    confidence: 0.95,
                },
            )]
            .into_iter()
            .collect(),
            unresolved: vec!["A002".to_string()],
        };

        assert!(matches.resolved.contains_key("A001"));
        assert!(!matches.resolved.contains_key("A002"));
        assert!(matches.unresolved.contains(&"A002".to_string()));
    }

    /// Verify the types are compatible with the report module's AmbiguousCase.
    #[test]
    fn resolve_accepts_ambiguous_case() {
        // Construct an AmbiguousCase (from the report module) to verify
        // the types match what run_interactive_resolution expects.
        let case = AmbiguousCase {
            handle_a: "A001".into(),
            item_type_a: "Person".into(),
            context_a: AmbiguousContext {
                display_name: "John Smith".into(),
                related_items: vec![RelatedItem {
                    handle: "F001".into(),
                    relationship: "spouse".into(),
                    display_name: "Jane Smith".into(),
                }],
            },
            candidates: vec![
                crate::report::Candidate {
                    handle_b: "B001".into(),
                    score: 0.85,
                    context_b: AmbiguousContext {
                        display_name: "Johnny Smith".into(),
                        related_items: vec![],
                    },
                },
                crate::report::Candidate {
                    handle_b: "B002".into(),
                    score: 0.72,
                    context_b: AmbiguousContext {
                        display_name: "Jon Smith".into(),
                        related_items: vec![],
                    },
                },
            ],
        };

        // Verify the case has the expected shape
        assert_eq!(case.handle_a, "A001");
        assert_eq!(case.candidates.len(), 2);
        assert_eq!(case.candidates[0].handle_b, "B001");
        assert_eq!(case.candidates[0].score, 0.85);
    }
}
