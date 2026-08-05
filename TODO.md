# Implementation Plan: Gzip-Compressed .gramps File Support

Source: `docs/research/gzip-compressed-gramps-files.md`

## Overview

Add transparent gzip decompression support for `.gramps` files across all
consumers (`stats`, `validate`, `visualize`). Detection is content-based
(magic bytes `0x1f 0x8b`), centralized in a new `gramps_reader::read_gramps_file`
function, and uses the `flate2` crate for decompression.

| # | Commit message | Logical unit | Key deliverables | Tests |
|---|---|---|---|---|
| 1 | `chore: add flate2 dependency to gramps-reader` | Dependency | `crates/gramps-reader/Cargo.toml` | — |
| 2 | `feat: add IoError and GzipError to gramps_reader::Error` | Error types | `crates/gramps-reader/src/error.rs`, `crates/cli/src/error.rs` | Unit |
| 3 | `feat: add read_gramps_file with gzip auto-detection and decompression` | File reader | `crates/gramps-reader/src/io.rs`, `crates/gramps-reader/src/lib.rs` | Unit |
| 4 | `refactor: route file reads through gramps_reader::read_gramps_file` | Consumer updates | `crates/visualize/src/lib.rs`, `crates/cli/src/commands/stats/mod.rs`, `crates/cli/src/commands/validate.rs` | Unit |
| 5 | `test: add integration tests for gzip-compressed .gramps files` | Integration tests | `crates/gramps-reader/tests/` | Integration |

## Step Details

### Step 1 — chore: add flate2 dependency to gramps-reader

**Files:**

- `crates/gramps-reader/Cargo.toml` — add `flate2 = "1"` to `[dependencies]`

**Tests:** Build verification only (`cargo build -p gramps-reader`).

### Step 2 — feat: add IoError and GzipError to gramps_reader::Error

**Files:**

- `crates/gramps-reader/src/error.rs` — add `IoError` and `GzipError` variants to `Error` enum, each with `path: String` and `source: std::io::Error` fields. Update `Display` and implement `source()` for proper error chaining.
- `crates/cli/src/error.rs` — add `CliError::GzipError { path, source }` variant, update `From<gramps_reader::Error>` to handle all three variants (`IoError`, `GzipError`, `XmlParseError`), update `Display` and `source()`.

**Tests (unit):**

- `gramps_reader::Error::IoError` source returns `Some(source)` via `source()`
- `gramps_reader::Error::GzipError` source returns `Some(source)` via `source()`
- `CliError::GzipError` Display includes path and source message
- `From<gramps_reader::Error>` conversion: `IoError` → `CliError::Io`, `GzipError` → `CliError::GzipError`, `XmlParseError` → `CliError::XmlParseError`
- `CliError::GzipError` source returns `Some(source)` via `source()`

### Step 3 — feat: add read_gramps_file with gzip auto-detection and decompression

**Files:**

- `crates/gramps-reader/src/io.rs` — new module with `read_gramps_file(path: &str) -> Result<String, Error>`
  - Open file, read first 2 bytes for gzip magic (`0x1f 0x8b`)
  - If magic matches: seek back to start, decompress via `flate2::read::GzDecoder`
  - If plain/mismatch: seek back to start, read as plain text
  - Empty file (n < 2): return `Ok(String::new())`
  - I/O errors → `Error::IoError`, decompression errors → `Error::GzipError`
- `crates/gramps-reader/src/lib.rs` — declare `mod io;` and re-export `pub use io::read_gramps_file;`

**Tests (unit — all use `tempfile` for isolation):**

- Empty file (0 bytes) → `Ok("")`
- 1-byte file → `Ok("")` (n < 2 code path)
- Nonexistent file → `Err(Error::IoError { .. })`
- Corrupted gzip (magic bytes + bad data) → `Err(Error::GzipError { .. })`
- Plain XML file → returns content identical to `std::fs::read_to_string`

### Step 4 — refactor: route file reads through gramps_reader::read_gramps_file

**Files:**

- `crates/visualize/src/lib.rs` — replace `std::fs::read_to_string(path)` with `gramps_reader::read_gramps_file(path)` in both `load_graph_data_with_stats()` and `get_stats()`. Error conversion: `.map_err(|e| format!("Cannot read file '{}': {}", path, e))`.
- `crates/cli/src/commands/stats/mod.rs` — replace `std::fs::read_to_string(file_path)` with `gramps_reader::read_gramps_file(file_path)?`. The `From<gramps_reader::Error>` conversion (Step 2) handles error type automatically.
- `crates/cli/src/commands/validate.rs` — same replacement as stats.

**Tests (unit):**

- Existing `load_graph_data_nonexistent_file` still passes (error message wording unchanged)
- Existing `load_graph_data_malformed_file` still passes (XML parse errors still flow through)
- Existing `get_stats_nonexistent_file` still passes
- Existing `stats_file_not_found_returns_io_error` still passes
- Existing `validate_command_file_not_found` still passes
- (New) `stats` + `validate` run against a gzip-compressed temp file return success

### Step 5 — test: add integration tests for gzip-compressed .gramps files

**Files:**

- `crates/gramps-reader/tests/` — new integration test directory

**Tests (integration):**

- Copy the real compressed fixture (`gramps-ui-gen01.gramps`) into the repo as `crates/gramps-reader/tests/fixtures/gramps-ui-gen01.gramps`
- **Round-trip:** Read the compressed file → decompressed content matches the twin XML file (`gramps-ui-gen01.xml`)
- **Stats pipeline:** `count_gramps_xml` on decompressed content produces expected counts
- **Visualize pipeline:** `load_graph_data_with_stats` on compressed file returns populated `GraphData` with correct nodes/links
- **Full CLI:** `gramps-gen stats` on the compressed file exits successfully

## Notes

- **Fixture files:** The test files at `/home/tr/Documents/gramps01/gramps-ui-gen01.gramps` (729 bytes, gzip) and `/home/tr/Documents/gramps01/gramps-ui-gen01.xml` (2223 bytes, plain) are currently outside the repo. Step 5 proposes copying the compressed fixture into the repo under `crates/gramps-reader/tests/fixtures/`. This is a 729-byte binary file — negligible size impact — and makes the tests self-contained and portable.
