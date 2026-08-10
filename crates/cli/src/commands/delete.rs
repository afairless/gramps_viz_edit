//! Delete command — remove selected people and their orphaned dependencies.
//!
//! Pipeline: parse input file → load selections → run cascade engine →
//! (optional review) → validate → write output with filter.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use clap::Args;
use gramps_reader::io::read_gramps_file;
use gramps_reader::xml::graph::parse_gramps_xml;
use output::{GraphXmlWriter, SerializationMap};

use delete::cascade;
use delete::manifest::{build_manifest, load_manifest, save_manifest, ManifestError};
use delete::review::{run_interactive_review, ReviewResult};
use delete::types::DeletePlan;

use crate::error::CliError;

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
    let (graph, namespace) = parse_gramps_xml(&content).map_err(|e| CliError::XmlParseError {
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
        manifest.seed_people.into_iter().collect()
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

    // 5. Save manifest (if requested)
    if let Some(ref manifest_path) = args.save_manifest {
        log::info!("Saving manifest to: {}", manifest_path.display());
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
        let manifest = build_manifest(
            &source_file,
            selections_file.as_deref(),
            &seed_vec,
            &delete_vec,
            &graph,
        );
        save_manifest(&manifest, manifest_path)?;
    }

    // 6. Dry run check
    if args.dry_run {
        log::info!(
            "Dry run complete. Would delete {} handles.",
            final_to_delete.len()
        );
        return Ok(());
    }

    // 7. Write output with filter
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

    // Determine whether to write gzip
    let is_gzip = output_path.extension().is_some_and(|e| e == "gz");

    // Build the writer with filter and namespace preservation
    let serialization_map = SerializationMap::new();
    // Derive the output format version from the input namespace so the
    // serialized content format (flat vs nested) and header match the input
    // rather than always assuming 5.2.
    let gramps_version = output::namespace_to_version(&namespace);
    let writer = GraphXmlWriter::new(serialization_map, &gramps_version)
        .with_filter(final_to_delete.clone())
        .with_namespace(&namespace)
        .with_researcher_note("Cleaned by gramps-gen delete");

    // Validate deletion set before writing
    writer.validate_deletion_set(&graph, &final_to_delete)?;

    if is_gzip {
        write_gzip_output(&writer, &graph, &output_path)?;
    } else {
        let file = fs::File::create(&output_path).map_err(|e| CliError::Io {
            path: output_path.display().to_string(),
            source: e,
        })?;
        let mut writer_io = std::io::BufWriter::new(file);
        writer.write(&graph, &mut writer_io)?;
    }

    log::info!(
        "Done. Deleted {} handles (out of {} total nodes).",
        final_to_delete.len(),
        graph.node_count(),
    );
    Ok(())
}

/// Write graph output through Gzip compression.
fn write_gzip_output(
    writer: &GraphXmlWriter,
    graph: &typed_graph::Graph,
    path: &Path,
) -> Result<(), CliError> {
    let file = fs::File::create(path).map_err(|e| CliError::Io {
        path: path.display().to_string(),
        source: e,
    })?;
    let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::default());
    let mut buf_writer = std::io::BufWriter::new(encoder);
    writer.write(graph, &mut buf_writer)?;
    buf_writer.into_inner().map_err(|_| CliError::Io {
        path: path.display().to_string(),
        source: std::io::Error::other("failed to flush gzip output"),
    })?;
    Ok(())
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

// The From<integrate::IntegrateError> impl already exists in cli/src/error.rs
// so we don't duplicate it here — the trait is used via the `?` operator
// which uses the existing conversion.

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
        };
        assert!(!args.yes);
        assert!(!args.dry_run);
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
    fn integrate_error_to_cli_error() {
        let err = integrate::IntegrateError::SelectionsReadError("bad".to_string());
        let cli: CliError = err.into();
        match cli {
            CliError::IntegrateFailed(_) => {}
            _ => panic!("expected IntegrateFailed variant"),
        }
    }
}
