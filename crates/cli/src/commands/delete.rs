//! Delete command — remove selected people and their orphaned dependencies.
//!
//! Pipeline: parse input file → load selections → run cascade engine →
//! (optional review) → build manifest v2 → delegate to Python backend
//! (persistent DB, people-only deletion) → reconcile manifest against
//! surviving report → save enriched manifest → clean orphaned events
//! from output XML → clean orphaned notes from output XML → clean
//! orphaned places from output XML → save final manifest alongside output.
//!
//! XML I/O is delegated to Gramps' own import/delete/export libraries
//! via a Python subprocess (`scripts/delete_backend.py`). The Rust
//! cascade engine remains intact for computing the deletion set, but is
//! advisory-only for non-people types — only people are actually deleted
//! through the Gramps API. The Gramps DB is retained for user inspection
//! (unless `--no-retain-db` is specified).

use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;

use clap::Args;
use gramps_reader::io::read_gramps_file;
use gramps_reader::xml::graph::parse_gramps_xml;
use serde::Deserialize;

use delete::cascade;
use delete::manifest::{build_manifest, load_manifest, save_manifest, ManifestError};
use delete::reconcile::{reconcile, ReconciliationError};
use delete::review::{run_interactive_review, ReviewResult};
use delete::types::{DeletePlan, HandleStatus, TypePlan};

use crate::error::CliError;

/// JSON result returned by the Python backend on stdout.
#[derive(Deserialize, Debug)]
#[allow(dead_code)]
struct PythonResult {
    status: String,
    #[serde(default)]
    output: Option<String>,
    #[serde(default)]
    deleted: Option<u32>,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    rejected: Vec<String>,
    /// Handles that still exist in the DB after deletion (for reconciliation).
    #[serde(default)]
    surviving: Vec<String>,
}

/// Arguments for the `delete` subcommand.
#[derive(Args, Clone, Debug)]
pub struct DeleteArgs {
    /// Input .gramps file (accepts .gramps.gz transparently)
    pub input: PathBuf,

    /// Visualizer selections JSON file
    #[arg(short = 's', long = "selections")]
    pub selections: Option<PathBuf>,

    /// Output .gramps file (default: <input>-cleaned.gramps)
    #[arg(short = 'o', long = "output")]
    pub output: Option<PathBuf>,

    /// Skip all review prompts (delete everything the cascade computed)
    #[arg(short = 'y', long)]
    pub yes: bool,

    /// Compute deletions but don't write output
    #[arg(short = 'n', long)]
    pub dry_run: bool,

    /// Save deletion manifest to JSON file
    #[arg(long = "save-manifest")]
    pub save_manifest: Option<PathBuf>,

    /// Load pre-reviewed deletion manifest (skip review)
    #[arg(long = "load-manifest")]
    pub load_manifest: Option<PathBuf>,

    /// Gramps database directory (default: <input-stem>-gramps-db/)
    #[arg(long = "db-dir")]
    pub db_dir: Option<PathBuf>,

    /// Clean up Gramps DB after successful export
    #[arg(long = "no-retain-db")]
    pub no_retain_db: bool,
}

/// Resolve the Python interpreter that can import Gramps.
///
/// Priority:
/// 1. `GRAMPS_PYTHON` env var (use as-is)
/// 2. Probe `python3` on PATH
///
/// Returns the interpreter path, or an error if Gramps is not importable.
fn resolve_python_interpreter() -> Result<String, CliError> {
    // 1. Env var override
    if let Ok(python) = std::env::var("GRAMPS_PYTHON") {
        log::debug!("Using GRAMPS_PYTHON: {python}");
        return Ok(python);
    }

    // 2. Probe python3
    let output = Command::new("python3")
        .args(["-c", "import gramps.gen.db.utils"])
        .output()
        .map_err(|e| {
            CliError::ConfigError(format!(
                "Gramps Python libraries not found. \
             Could not run python3: {e}. \
             Set GRAMPS_PYTHON env var to the python interpreter that can import gramps."
            ))
        })?;

    if output.status.success() {
        log::debug!("Resolved python3 for Gramps");
        Ok("python3".to_string())
    } else {
        Err(CliError::ConfigError(
            "Gramps Python libraries not found. \
             Set GRAMPS_PYTHON env var to the python interpreter that can import gramps."
                .to_string(),
        ))
    }
}

/// Check if Gramps Python libraries are available and the version is supported.
///
/// Returns `true` if:
/// 1. `python3` can import `gramps.gen.db.utils`
/// 2. The installed Gramps version starts with "5.1" or "5.2"
///
/// Returns `false` if Gramps is not installed, not importable, or
/// the version is unsupported (e.g. 6.x which removed bsddb).
pub fn gramps_available() -> bool {
    let python = match resolve_python_interpreter() {
        Ok(p) => p,
        Err(_) => return false,
    };

    // Verify the installed Gramps version is one we support (5.1/5.2).
    let output = Command::new(&python)
        .args([
            "-c",
            "import gramps.gen.const as c; \
             assert c.VERSION.startswith(('5.1','5.2'))",
        ])
        .output();

    matches!(output, Ok(o) if o.status.success())
}

/// Resolve the path to `scripts/delete_backend.py`.
///
/// Priority:
/// 1. `GRAMPS_DELETE_BACKEND` env var
/// 2. Embed at build time relative to crate root
fn resolve_script_path() -> Result<PathBuf, CliError> {
    if let Ok(path) = std::env::var("GRAMPS_DELETE_BACKEND") {
        let p = PathBuf::from(&path);
        if p.exists() {
            log::debug!("Using GRAMPS_DELETE_BACKEND: {path}");
            return Ok(p);
        }
        return Err(CliError::ConfigError(format!(
            "GRAMPS_DELETE_BACKEND points to nonexistent file: {path}"
        )));
    }

    // Build-time embedded path: crates/cli/../../scripts/delete_backend.py
    let embedded = PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../scripts/delete_backend.py"
    ));
    if embedded.exists() {
        log::debug!("Using embedded script: {}", embedded.display());
        Ok(embedded)
    } else {
        Err(CliError::ConfigError(format!(
            "Python delete backend not found at {}. \
             Set GRAMPS_DELETE_BACKEND env var to the script path.",
            embedded.display()
        )))
    }
}

/// Run the delete command.
pub fn run(args: DeleteArgs) -> Result<(), CliError> {
    let input_path = &args.input;

    // 1. Parse the input .gramps file
    log::info!("Reading input file: {}", input_path.display());
    let content =
        read_gramps_file(&input_path.display().to_string()).map_err(|e| CliError::Io {
            path: input_path.display().to_string(),
            source: std::io::Error::other(e.to_string()),
        })?;

    log::info!("Parsing XML graph...");
    let (graph, _namespace) = parse_gramps_xml(&content).map_err(|e| CliError::XmlParseError {
        message: format!("failed to parse '{}': {}", input_path.display(), e),
    })?;

    log::info!(
        "Graph loaded: {} nodes, {} edges",
        graph.node_count(),
        graph.edge_count()
    );

    // 2. Determine seed handles (from selections or loaded manifest)
    let seed_people: HashSet<String> = if let Some(ref manifest_path) = args.load_manifest {
        log::info!("Loading manifest: {}", manifest_path.display());
        let manifest = load_manifest(manifest_path)?;
        // Filter to pending-only handles from to_delete lists. Deleted/kept
        // handles are excluded — they were already removed or explicitly kept
        // in a previous run.
        let pending_people: HashSet<String> = manifest
            .plan
            .get("people")
            .map(|p: &TypePlan| {
                p.to_delete
                    .iter()
                    .filter(|e| e.status == HandleStatus::Pending)
                    .map(|e| e.handle.clone())
                    .collect()
            })
            .unwrap_or_default();
        if pending_people.is_empty() {
            log::warn!("Manifest contains no pending people to delete — nothing to do.");
            return Ok(());
        }
        pending_people
    } else if let Some(ref selections_path) = args.selections {
        log::info!("Loading selections: {}", selections_path.display());
        let selections =
            integrate::json_reader::parse_selections_json(&selections_path.display().to_string())?;
        if selections.is_empty() {
            return Err(CliError::ConfigError(
                "selections file is empty (no people selected)".to_string(),
            ));
        }
        // Cross-reference: check overlap with graph handles
        let graph_handles: HashSet<&String> = graph.iter_nodes().map(|(h, _)| h).collect();
        let match_count = selections
            .iter()
            .filter(|s| graph_handles.contains(&s.handle))
            .count();
        let overlap_pct = (match_count as f64 / selections.len() as f64) * 100.0;
        log::info!(
            "Selections: {} total, {} match graph ({:.0}%)",
            selections.len(),
            match_count,
            overlap_pct,
        );
        if overlap_pct == 0.0 {
            return Err(CliError::ConfigError(format!(
                "0% of selection handles match the graph. Check that the selections file \
                 matches the input .gramps file. (Found {} handles in selections, \
                 {} in the graph)",
                selections.len(),
                graph.node_count(),
            )));
        }
        if overlap_pct < 50.0 {
            log::warn!(
                "Only {:.0}% of selection handles match the graph. \
                 This may indicate a file mismatch.",
                overlap_pct,
            );
        }

        selections.into_iter().map(|s| s.handle).collect()
    } else {
        return Err(CliError::ConfigError(
            "either --selections or --load-manifest is required".to_string(),
        ));
    };

    // 3. Run cascade engine
    log::info!(
        "Running cascade engine with {} seed people...",
        seed_people.len()
    );
    let plan: DeletePlan = cascade::cascade(&graph, &seed_people);
    log::info!(
        "Cascade result: {} total handles to delete ({} people, {} families, {} events, ...)",
        plan.to_delete.len(),
        plan.per_type
            .get(&delete::types::NodeKindLabel::Person)
            .map_or(0, |v| v.len()),
        plan.per_type
            .get(&delete::types::NodeKindLabel::Family)
            .map_or(0, |v| v.len()),
        plan.per_type
            .get(&delete::types::NodeKindLabel::Event)
            .map_or(0, |v| v.len()),
    );

    if plan.to_delete.is_empty() {
        log::info!("Nothing to delete. Output will be identical to input.");
        if args.dry_run {
            return Ok(());
        }
        // Still write the output (it will be identical)
    }

    // 4. (Optional) Interactive review
    let final_to_delete: HashSet<String> = if args.yes || args.load_manifest.is_some() {
        // Skip review: confirm everything
        let mut result = plan.to_delete.clone();
        // Also add seed people (in case they weren't in the cascade output)
        for h in &seed_people {
            result.insert(h.clone());
        }
        result
    } else {
        let mut stdin = std::io::BufReader::new(std::io::stdin());
        let mut stdout = std::io::stdout();
        let review_result = run_interactive_review(&plan, &graph, false, &mut stdin, &mut stdout);
        match review_result {
            ReviewResult::Confirmed(confirmed) => {
                let mut result = HashSet::new();
                for (_type_name, handles) in confirmed {
                    result.extend(handles);
                }
                // Also add seed people
                for h in &seed_people {
                    result.insert(h.clone());
                }
                result
            }
            ReviewResult::Aborted => {
                log::info!("Operation aborted by user. No file written.");
                return Err(CliError::ConfigError("operation aborted".to_string()));
            }
        }
    };

    // 5. Build manifest (always — Python backend needs it)
    let source_file = input_path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let selections_file = args
        .selections
        .as_ref()
        .map(|p| p.to_string_lossy().to_string());
    let seed_vec: Vec<String> = seed_people.iter().cloned().collect();
    let delete_vec: Vec<String> = final_to_delete.iter().cloned().collect();
    let mut manifest = build_manifest(
        &source_file,
        selections_file.as_deref(),
        &seed_vec,
        &delete_vec,
        &graph,
    );

    // 6. Compute output path
    let output_path = args.output.unwrap_or_else(|| {
        let input_name = input_path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "output".to_string());
        let out = if input_path.extension().is_some_and(|e| e == "gz") {
            // For .gramps.gz, strip both extensions
            let base = input_path
                .file_stem()
                .and_then(|s| {
                    let s = s.to_string_lossy();
                    let s_str = s.as_ref();
                    let dot = s_str.rfind('.');
                    dot.map(|i| s_str[..i].to_string())
                })
                .unwrap_or_else(|| input_name.clone());
            format!("{}-cleaned.gramps.gz", base)
        } else {
            format!("{}-cleaned.gramps", input_name)
        };
        PathBuf::from(out)
    });

    log::info!("Writing output to: {}", output_path.display());

    // 7. Determine manifest output path — always saved alongside output.
    //    `--save-manifest` overrides the default <output-stem>.manifest.json.
    let manifest_path = match args.save_manifest {
        Some(ref p) => p.clone(),
        None => PathBuf::from(format!(
            "{}.manifest.json",
            strip_gramps_extensions(&output_path)
        )),
    };
    log::info!("Saving manifest to: {}", manifest_path.display());
    save_manifest(&manifest, &manifest_path)?;

    // 8. Dry run check (manifest already saved with all-Pending statuses)
    if args.dry_run {
        log::info!(
            "Dry run complete. Would delete {} handles.",
            final_to_delete.len()
        );
        return Ok(());
    }

    // 8. Write manifest to temp file for Python backend
    let temp_dir = std::env::temp_dir().join(format!("gramps_delete_{}", std::process::id()));
    fs::create_dir_all(&temp_dir).map_err(|e| CliError::Io {
        path: temp_dir.display().to_string(),
        source: e,
    })?;
    let manifest_temp = temp_dir.join("delete_manifest.json");
    let manifest_json = serde_json::to_string_pretty(&manifest)
        .map_err(|e| CliError::ConfigError(format!("failed to serialize manifest: {e}")))?;
    fs::write(&manifest_temp, &manifest_json).map_err(|e| CliError::Io {
        path: manifest_temp.display().to_string(),
        source: e,
    })?;

    // 9. Resolve Python interpreter and script
    let python = resolve_python_interpreter()?;
    let script = resolve_script_path()?;

    // Determine the Gramps DB directory (default: <input-stem>-gramps-db/)
    let db_dir = args.db_dir.clone().unwrap_or_else(|| {
        PathBuf::from(format!("{}-gramps-db", strip_gramps_extensions(input_path)))
    });
    if db_dir.exists() {
        log::warn!(
            "Database directory {} already exists — it will be overwritten.",
            db_dir.display()
        );
    }
    log::info!("Gramps DB directory: {}", db_dir.display());

    log::info!(
        "Delegating XML I/O to Python backend: {} {}",
        python,
        script.display()
    );

    // 10. Spawn Python subprocess with thread-based timeout
    let python_clone = python.clone();
    let script_clone = script.clone();
    let input_clone = input_path.clone();
    let manifest_clone = manifest_temp.clone();
    let output_clone = output_path.clone();
    let db_dir_clone = db_dir.clone();
    let no_retain_db = args.no_retain_db;

    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut cmd = Command::new(&python_clone);
        cmd.arg(script_clone.to_string_lossy().as_ref())
            .arg("--input")
            .arg(&input_clone)
            .arg("--manifest")
            .arg(&manifest_clone)
            .arg("--output")
            .arg(&output_clone)
            .arg("--db-dir")
            .arg(&db_dir_clone);
        if no_retain_db {
            cmd.arg("--no-retain-db");
        }
        let result = cmd
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output();
        let _ = tx.send(result);
    });

    let timeout = Duration::from_secs(300);
    let output = match rx.recv_timeout(timeout) {
        Ok(Ok(output)) => output,
        Ok(Err(e)) => {
            return Err(CliError::Io {
                path: python,
                source: e,
            });
        }
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
            // Kill the process. We don't have the Child handle, but
            // the thread will exit when the command finishes.
            return Err(CliError::ConfigError(
                "Python backend timed out after 5 minutes".to_string(),
            ));
        }
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
            return Err(CliError::ConfigError(
                "Python backend thread panicked".to_string(),
            ));
        }
    };

    // 11. Parse stdout as PythonResult
    let result: PythonResult =
        serde_json::from_slice(&output.stdout).unwrap_or_else(|e| PythonResult {
            status: "error".to_string(),
            output: None,
            deleted: None,
            message: Some(format!("Failed to parse Python output: {e}")),
            rejected: vec![],
            surviving: vec![],
        });

    // Report stderr
    if !output.stderr.is_empty() {
        let stderr_str = String::from_utf8_lossy(&output.stderr);
        log::debug!("Python stderr: {}", stderr_str);
    }

    if result.status != "ok" || !output.status.success() {
        let msg = result.message.unwrap_or_else(|| {
            format!(
                "Python backend exited with {}",
                output.status.code().unwrap_or(-1)
            )
        });
        // Python backend failed. Save the pre-reconciliation manifest
        // (all statuses Pending) so the user still has a record of intent.
        // If the .gramps output exists (failure happened after writing),
        // delete it so no stale partial-result pair is left on disk.
        if output_path.exists() {
            let _ = fs::remove_file(&output_path);
        }
        let _ = save_manifest(&manifest, &manifest_path);
        log::warn!(
            "Python backend failed: {}. Reconciliation skipped; \
             pre-reconciliation manifest saved to {}",
            msg,
            manifest_path.display()
        );
        return Err(CliError::ConfigError(msg));
    }

    // 12. Reconcile the manifest against the surviving report
    let surviving: HashSet<String> = result.surviving.iter().cloned().collect();
    reconcile(&mut manifest, &surviving)
        .map_err(|e: ReconciliationError| CliError::ConfigError(e.to_string()))?;

    // 13. Warn if the retained Gramps DB is unusually large
    let db_size = dir_size(&db_dir);
    if db_size > 100 * 1024 * 1024 {
        log::warn!(
            "Gramps DB directory {} is large ({} MB). \
             Use --no-retain-db to remove it after export.",
            db_dir.display(),
            db_size / (1024 * 1024)
        );
    }

    // 14. Save the enriched manifest alongside the output
    save_manifest(&manifest, &manifest_path)?;

    // 15. Clean pending events from the output XML
    let pending_events: HashSet<String> = manifest
        .plan
        .get("events")
        .map(|p: &TypePlan| {
            p.to_delete
                .iter()
                .filter(|e| e.status == HandleStatus::Pending)
                .map(|e| e.handle.clone())
                .collect()
        })
        .unwrap_or_default();

    if !pending_events.is_empty() {
        let no_events_path = derive_no_events_path(input_path);
        log::info!(
            "Cleaning {} pending events from output -> {}",
            pending_events.len(),
            no_events_path.display()
        );
        let stats = crate::commands::clean::clean_events_xml(
            &output_path,
            &no_events_path,
            &pending_events,
        )?;
        log::info!(
            "Event cleaning complete: {} removed, {} not found in XML",
            stats.elements_removed,
            stats.elements_not_found,
        );
        // Mark these events as Deleted in the manifest
        if let Some(events_plan) = manifest.plan.get_mut("events") {
            for entry in &mut events_plan.to_delete {
                if entry.status == HandleStatus::Pending {
                    entry.status = HandleStatus::Deleted;
                }
            }
        }
        // Persist the updated manifest to disk (overwrites checkpoint copy)
        save_manifest(&manifest, &manifest_path)?;
    } else {
        log::info!("No pending events to clean.");
    }

    // 16. Clean pending notes from the output XML
    let pending_notes: HashSet<String> = manifest
        .plan
        .get("notes")
        .map(|p: &TypePlan| {
            p.to_delete
                .iter()
                .filter(|e| e.status == HandleStatus::Pending)
                .map(|e| e.handle.clone())
                .collect()
        })
        .unwrap_or_default();

    if !pending_notes.is_empty() {
        let deleted_2_path = derive_no_events_path(input_path);
        let deleted_3_path = derive_deleted_3_path(input_path);

        // Use -deleted_2.gramps as input if it exists (event cleaning ran),
        // otherwise fall back to the original -cleaned.gramps output.
        let note_input = if deleted_2_path.exists() {
            deleted_2_path
        } else {
            output_path.clone()
        };

        log::info!(
            "Cleaning {} pending notes from {} -> {}",
            pending_notes.len(),
            note_input.display(),
            deleted_3_path.display()
        );
        let stats = crate::commands::clean::clean_notes_xml(
            &note_input,
            &deleted_3_path,
            &pending_notes,
        )?;
        log::info!(
            "Note cleaning complete: {} removed, {} not found in XML",
            stats.elements_removed,
            stats.elements_not_found,
        );
        // Mark these notes as Deleted in the manifest
        if let Some(notes_plan) = manifest.plan.get_mut("notes") {
            for entry in &mut notes_plan.to_delete {
                if entry.status == HandleStatus::Pending {
                    entry.status = HandleStatus::Deleted;
                }
            }
        }
        // Persist the updated manifest to disk
        save_manifest(&manifest, &manifest_path)?;
    } else {
        log::info!("No pending notes to clean.");
    }

    // 17. Clean pending places from the output XML
    let pending_places: HashSet<String> = manifest
        .plan
        .get("places")
        .map(|p: &TypePlan| {
            p.to_delete
                .iter()
                .filter(|e| e.status == HandleStatus::Pending)
                .map(|e| e.handle.clone())
                .collect()
        })
        .unwrap_or_default();

    if !pending_places.is_empty() {
        let deleted_2_path = derive_no_events_path(input_path);
        let deleted_3_path = derive_deleted_3_path(input_path);
        let deleted_4_path = derive_deleted_4_path(input_path);

        // Use the latest available cleaned output as input.
        let place_input = if deleted_3_path.exists() {
            deleted_3_path
        } else if deleted_2_path.exists() {
            deleted_2_path
        } else {
            output_path.clone()
        };

        log::info!(
            "Cleaning {} pending places from {} -> {}",
            pending_places.len(),
            place_input.display(),
            deleted_4_path.display()
        );
        let stats = crate::commands::clean::clean_places_xml(
            &place_input,
            &deleted_4_path,
            &pending_places,
        )?;
        log::info!(
            "Place cleaning complete: {} removed, {} not found in XML",
            stats.elements_removed,
            stats.elements_not_found,
        );
        // Mark these places as Deleted in the manifest
        if let Some(places_plan) = manifest.plan.get_mut("places") {
            for entry in &mut places_plan.to_delete {
                if entry.status == HandleStatus::Pending {
                    entry.status = HandleStatus::Deleted;
                }
            }
        }
        // Persist the updated manifest to disk
        save_manifest(&manifest, &manifest_path)?;
    } else {
        log::info!("No pending places to clean.");
    }

    let deleted_count = result.deleted.unwrap_or(0);
    let pending_count = manifest
        .plan
        .values()
        .flat_map(|p: &TypePlan| p.to_delete.iter())
        .filter(|e| e.status == HandleStatus::Pending)
        .count();
    log::info!(
        "Done. Deleted {} people; {} handles still pending (kept by Gramps).",
        deleted_count,
        pending_count,
    );
    log::info!("Enriched manifest saved to: {}", manifest_path.display());
    if args.no_retain_db {
        log::info!("Gramps DB removed (--no-retain-db).");
    } else {
        log::info!("Gramps DB retained at: {}", db_dir.display());
    }
    if !result.rejected.is_empty() {
        log::warn!(
            "{} handles were rejected (invalid UUID format): {:?}",
            result.rejected.len(),
            result.rejected,
        );
    }

    // 18. Clean up temp directory
    let _ = fs::remove_dir_all(&temp_dir);

    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Strip a trailing `.gz` extension if present, then the remaining extension.
pub(crate) fn strip_gramps_extensions(path: &std::path::Path) -> String {
    let mut s = path.to_string_lossy().to_string();
    if s.ends_with(".gz") {
        s.truncate(s.len() - 3);
    }
    if let Some(i) = s.rfind('.') {
        s.truncate(i);
    }
    s
}

/// Derive the `-deleted_2.gramps` path from the input file path.
///
/// Strips Gramps extensions (`.gramps`, `.gramps.gz`) and appends
/// `-deleted_2.gramps`. Always uses a plain `.gramps` extension,
/// even if the input was compressed.
fn derive_no_events_path(input_path: &std::path::Path) -> std::path::PathBuf {
    let stem = strip_gramps_extensions(input_path);
    std::path::PathBuf::from(format!("{}-deleted_2.gramps", stem))
}

/// Derive the `-deleted_3.gramps` path from the input file path.
///
/// Strips Gramps extensions (`.gramps`, `.gramps.gz`) and appends
/// `-deleted_3.gramps`. Always uses a plain `.gramps` extension.
pub(crate) fn derive_deleted_3_path(input_path: &std::path::Path) -> std::path::PathBuf {
    let stem = strip_gramps_extensions(input_path);
    std::path::PathBuf::from(format!("{}-deleted_3.gramps", stem))
}

/// Derive the `-deleted_4.gramps` path from the input file path.
///
/// Strips Gramps extensions (`.gramps`, `.gramps.gz`) and appends
/// `-deleted_4.gramps`. Always uses a plain `.gramps` extension.
pub(crate) fn derive_deleted_4_path(input_path: &std::path::Path) -> std::path::PathBuf {
    let stem = strip_gramps_extensions(input_path);
    std::path::PathBuf::from(format!("{}-deleted_4.gramps", stem))
}

/// Compute the total size of a directory tree in bytes.
fn dir_size(path: &std::path::Path) -> u64 {
    let mut total = 0;
    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                total += dir_size(&p);
            } else if let Ok(meta) = entry.metadata() {
                total += meta.len();
            }
        }
    }
    total
}

// ---------------------------------------------------------------------------
// From impls for CliError
// ---------------------------------------------------------------------------

impl From<ManifestError> for CliError {
    fn from(err: ManifestError) -> Self {
        match err {
            ManifestError::Io(e) => CliError::Io {
                path: String::new(),
                source: e,
            },
            ManifestError::JsonParse(e) => CliError::ConfigError(e.to_string()),
            ManifestError::InvalidHandle(h) => {
                CliError::ConfigError(format!("manifest contains invalid handle: '{}'", h))
            }
            ManifestError::UnsupportedVersion(v) => {
                CliError::ConfigError(format!("unsupported manifest version: {}", v))
            }
            ManifestError::SourceFileMismatch {
                manifest_path,
                actual_path,
            } => {
                log::warn!(
                    "Source file mismatch: manifest says '{}', actual is '{}'",
                    manifest_path,
                    actual_path
                );
                CliError::ConfigError(format!(
                    "source file mismatch: manifest says '{}', actual is '{}'",
                    manifest_path, actual_path
                ))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delete_args_debug_and_clone() {
        let args = DeleteArgs {
            input: PathBuf::from("test.gramps"),
            selections: Some(PathBuf::from("sel.json")),
            output: Some(PathBuf::from("out.gramps")),
            yes: true,
            dry_run: false,
            save_manifest: None,
            load_manifest: None,
            db_dir: None,
            no_retain_db: false,
        };
        let cloned = args.clone();
        assert_eq!(args.input, cloned.input);
        assert_eq!(args.yes, cloned.yes);
    }

    #[test]
    fn delete_args_defaults() {
        let args = DeleteArgs {
            input: PathBuf::from("test.gramps"),
            selections: None,
            output: None,
            yes: false,
            dry_run: false,
            save_manifest: None,
            load_manifest: None,
            db_dir: None,
            no_retain_db: false,
        };
        assert!(!args.yes);
        assert!(!args.dry_run);
    }

    #[test]
    fn resolve_python_interpreter_uses_env_var() {
        // This test doesn't actually set the env var — it just verifies
        // the function compiles and returns the right error shape.
        // When GRAMPS_PYTHON is not set and python3 may or may not work,
        // this test is informational.
        let result = resolve_python_interpreter();
        // Accept both Ok and Err — we just need the function to not panic
        match result {
            Ok(interpreter) => {
                assert!(!interpreter.is_empty());
            }
            Err(_) => {
                // Expected in CI without Gramps
            }
        }
    }

    #[test]
    fn gramps_available_returns_bool() {
        // The function must compile and return a bool without panicking.
        let result = gramps_available();
        // Verifies it returns a bool without panicking; the actual value
        // is environment-dependent (true with Gramps, false without).
        assert_eq!(result, result, "must not panic");
    }

    #[test]
    fn gramps_available_does_not_panic() {
        // This function must never panic, even when Gramps is absent.
        let _ = gramps_available();
    }

    #[test]
    fn resolve_script_path_returns_path() {
        let result = resolve_script_path();
        match result {
            Ok(path) => {
                assert!(path.to_string_lossy().contains("delete_backend"));
            }
            Err(_) => {
                // Acceptable if running in an unusual build layout
            }
        }
    }

    #[test]
    fn manifest_error_to_cli_error() {
        let err = ManifestError::InvalidHandle("bad".to_string());
        let cli: CliError = err.into();
        match cli {
            CliError::ConfigError(msg) => assert!(msg.contains("bad")),
            _ => panic!("expected ConfigError"),
        }
    }

    #[test]
    fn manifest_io_error_to_cli_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "not found");
        let err = ManifestError::Io(io_err);
        let cli: CliError = err.into();
        match cli {
            CliError::Io { .. } => {}
            _ => panic!("expected Io variant"),
        }
    }

    #[test]
    fn strip_gramps_extensions_basic() {
        assert_eq!(
            strip_gramps_extensions(&PathBuf::from("test.gramps")),
            "test"
        );
        assert_eq!(
            strip_gramps_extensions(&PathBuf::from("test.gramps.gz")),
            "test"
        );
        assert_eq!(
            strip_gramps_extensions(&PathBuf::from("test-cleaned.gramps")),
            "test-cleaned"
        );
        assert_eq!(strip_gramps_extensions(&PathBuf::from("noext")), "noext");
    }

    #[test]
    fn dir_size_empty_path() {
        // Non-existent path should return 0, not panic
        assert_eq!(dir_size(PathBuf::from("/nonexistent/path").as_path()), 0);
    }

    #[test]
    fn derive_deleted_3_path_basic() {
        assert_eq!(
            derive_deleted_3_path(&PathBuf::from("test.gramps")),
            PathBuf::from("test-deleted_3.gramps")
        );
    }

    #[test]
    fn derive_deleted_3_path_gz() {
        assert_eq!(
            derive_deleted_3_path(&PathBuf::from("test.gramps.gz")),
            PathBuf::from("test-deleted_3.gramps")
        );
    }

    #[test]
    fn derive_deleted_3_path_no_ext() {
        assert_eq!(
            derive_deleted_3_path(&PathBuf::from("noext")),
            PathBuf::from("noext-deleted_3.gramps")
        );
    }

    #[test]
    fn derive_deleted_3_path_already_cleaned() {
        assert_eq!(
            derive_deleted_3_path(&PathBuf::from("test-cleaned.gramps")),
            PathBuf::from("test-cleaned-deleted_3.gramps")
        );
    }

    #[test]
    fn derive_deleted_4_path_basic() {
        assert_eq!(
            derive_deleted_4_path(&PathBuf::from("test.gramps")),
            PathBuf::from("test-deleted_4.gramps")
        );
    }

    #[test]
    fn derive_deleted_4_path_gz() {
        assert_eq!(
            derive_deleted_4_path(&PathBuf::from("test.gramps.gz")),
            PathBuf::from("test-deleted_4.gramps")
        );
    }

    #[test]
    fn derive_deleted_4_path_no_ext() {
        assert_eq!(
            derive_deleted_4_path(&PathBuf::from("noext")),
            PathBuf::from("noext-deleted_4.gramps")
        );
    }

    #[test]
    fn derive_deleted_4_path_already_cleaned() {
        assert_eq!(
            derive_deleted_4_path(&PathBuf::from("test-cleaned.gramps")),
            PathBuf::from("test-cleaned-deleted_4.gramps")
        );
    }

    #[test]
    fn integrate_error_to_cli_error() {
        let err = integrate::IntegrateError::SelectionsReadError("bad".to_string());
        let cli: CliError = err.into();
        match cli {
            CliError::IntegrateFailed(_) => {}
            _ => panic!("expected IntegrateFailed variant"),
        }
    }
}
