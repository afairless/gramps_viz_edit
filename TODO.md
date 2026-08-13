# Implementation Plan: Output Directory Semantics + DB Retention Default Flip

Source: `docs/research/output-dir-and-retain-db-defaults.md`

## Summary

Two changes to the `gramps-gen delete` command:

1. **`--output` becomes a directory** (not a file). All output files are saved
   inside that directory with hyphenated `-deleted-N.gramps` naming. When omitted,
   defaults to the input file's directory.
2. **DB retention default flips**: the Berkeley DB is **not saved** by default.
   A new `--retain-db` flag opts in to keeping it. The old `--no-retain-db` flag
   is removed.

| # | Commit message | Logical unit | Key deliverables | Tests |
|---|---|---|---|---|
| 1 | `refactor: replace path helpers with hyphenated \`-deleted-N\` naming` | Path helper functions | `crates/cli/src/commands/delete.rs` — add `deleted_1_path`, `deleted_2_path`, `deleted_3_path`, `deleted_4_path`; remove `derive_no_events_path`, `derive_deleted_3_path`, `derive_deleted_4_path`; update pipeline call sites to use new helpers | Unit |
| 2 | `feat: change \`--output\` to directory, add \`--retain-db\` flag` | Args struct + output directory resolution | `crates/cli/src/commands/delete.rs` — rename `output`→`output_dir` in `DeleteArgs`, replace `no_retain_db` with `retain_db`, add output directory resolution + `create_dir_all`, update manifest path derivation, add "File exists" error mapping | Unit |
| 3 | `feat: invert DB retention default to temp-dir + cleanup` | DB retention default flip | `crates/cli/src/commands/delete.rs` — default `db_dir` to timestamped temp dir, invert `--no-retain-db` pass-through to `delete_backend.py`, add conditional temp cleanup, remove `dir_size()` function + its unit test, update module docstring and final log lines | Unit |
| 4 | `test: update e2e tests for new output directory and retention semantics` | E2E test suite update | `crates/cli/tests/e2e_delete.rs` — add `temp_dir` helper, replace all `--output <file>` with `--output <dir>`, retarget file reads to `<dir>/<stem>-deleted-N.gramps`, add tests for old-usage error, default DB cleanup, `--retain-db` | Integration |
| 5 | `docs: rewrite delete-tool.md for new CLI semantics` | User documentation | `docs/delete-tool.md` — update all examples, tables, argument descriptions, add "Which file is the final output?" callout | — |
| 6 | `docs: update ARCHITECTURE.md delete pipeline section` | Architecture documentation | `docs/ARCHITECTURE.md` — update architecture diagram/footnotes for new output naming and DB retention default | — |

## Notes

- `scripts/delete_backend.py` and `scripts/test_delete_backend.py` require **no changes**.
- `crates/cli/src/main.rs` requires **no changes** (confirmed by inspection — no mention of `--no-retain-db` or output file semantics).
- The Python backend's `--no-retain-db` flag stays as-is; only the Rust call site inverts the pass-through (step 3).
- Each step includes its unit tests alongside the code changes, following incremental-development principles. E2E tests (step 4) are updated separately since they span the full pipeline.
