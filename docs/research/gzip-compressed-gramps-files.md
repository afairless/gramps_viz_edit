# Gzip-Compressed .gramps File Support

**Date:** 2026-08-06
**Status:** Research plan

## Problem

Gramps can export family trees as compressed `.gramps` files. These files are
standard gzip-compressed XML, but the tooling (`gramps-gen stats`, `gramps-gen
validate`, and `gramps-gen-visualize`) currently uses `std::fs::read_to_string`
which fails on gzip data because it contains raw binary bytes.

The user provided a test file at:
`/home/tr/Documents/gramps01/gramps-ui-gen01.gramps`

## File Format Analysis

The test file is **pure gzip** wrapping standard Gramps XML:

```
$ file gramps-ui-gen01.gramps
gzip compressed data, was "gramps-ui-gen01.gramps", last modified: Wed Aug  5 15:46:48 2026

$ xxd | head -1
00000000: 1f8b 0808 ...    ← gzip magic bytes (0x1f 0x8b)

$ zcat | head -c 200
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE database PUBLIC "-//Gramps//DTD Gramps XML 1.7.1//EN"
"http://gramps-project.org/xml/1.7.1/grampsxml.dtd">
<database xmlns="http://gramps-project.org/xml/1.7.1/">
```

Key characteristics:

- **Compressed size:** 729 bytes
- **Decompressed size:** 2223 bytes
- **Format:** Standard gzip (RFC 1952), no wrapper, no ZIP archive
- **Content:** Pure Gramps XML with the same document structure as uncompressed files
- **Extension:** `.gramps` (same as uncompressed — no way to distinguish by extension alone)

The `.gramps` extension is used for **both** compressed and uncompressed files.
Detection must be content-based, not extension-based.

## Design Decisions

All decisions were confirmed with the user:

| Decision | Choice | Rationale |
|---|---|---|
| **Detection method** | Magic bytes (0x1f 0x8b) | Fast, reliable, no false positives. The standard approach used by `file(1)` and GNU `gunzip`. |
| **Gzip library** | `flate2` | De facto standard Rust gzip/zlib crate. Pure Rust by default (miniz_oxide backend), well-maintained, used by hundreds of crates. |
| **Extension handling** | Auto-detect by content | No extension-based logic. Works for `.gramps`, `.gramps.gz`, or any other extension. |
| **Centralization** | `gramps-reader` crate | One function called by all consumers. Single place to add, test, and maintain. |

## Implementation Plan

### Step 1: Add `flate2` dependency to `gramps-reader`

**File:** `crates/gramps-reader/Cargo.toml`

Add `flate2` to `[dependencies]`. The `rle` decompression feature is not needed —
only standard gzip decompression.

### Step 2: Add error variants to `gramps_reader::Error` and update `CliError`

**Files:** `crates/gramps-reader/src/error.rs`, `crates/cli/src/error.rs`

Add `IoError` and `GzipError` variants to `gramps_reader::Error`, update
`Display` and `source()`, and add the corresponding `From` conversion in
`CliError` so the downstream consumers can use `?` immediately.

**`crates/gramps-reader/src/error.rs`** — expanded `Error` enum:

```rust
#[derive(Debug)]
pub enum Error {
    /// XML parse error with descriptive message.
    XmlParseError {
        message: String,
    },
    /// File I/O error (file not found, permission denied, etc.).
    IoError {
        path: String,
        source: std::io::Error,
    },
    /// Gzip decompression error (corrupted or truncated gzip data).
    GzipError {
        path: String,
        source: std::io::Error,
    },
}
```

Update `Display` to cover all variants:

```rust
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::XmlParseError { message } => write!(f, "XML parse error: {}", message),
            Error::IoError { path, source } => write!(f, "I/O error for '{}': {}", path, source),
            Error::GzipError { path, source } => {
                write!(f, "gzip decompression error for '{}': {}", path, source)
            }
        }
    }
}
```

Implement `source()` for proper error chaining:

```rust
impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::IoError { source, .. } | Error::GzipError { source, .. } => Some(source),
            Error::XmlParseError { .. } => None,
        }
    }
}
```

**`crates/cli/src/error.rs`** — add a `GzipError` variant so the CLI error
taxonomy stays accurate (a corrupt gzip file should not be reported as an
"XML parse error"):

```rust
#[derive(Debug)]
pub enum CliError {
    // ... existing variants ...
    /// Gzip decompression error (corrupted or truncated gzip data).
    GzipError {
        path: String,
        source: std::io::Error,
    },
}
```

Update the `From<gramps_reader::Error>` conversion to handle all variants:

```rust
impl From<gramps_reader::Error> for CliError {
    fn from(e: gramps_reader::Error) -> Self {
        match e {
            gramps_reader::Error::IoError { path, source } => CliError::Io { path, source },
            gramps_reader::Error::GzipError { path, source } => CliError::GzipError { path, source },
            gramps_reader::Error::XmlParseError { message } => CliError::XmlParseError { message },
        }
    }
}
```

Update `CliError::Display` for the new `GzipError` variant:

```rust
CliError::GzipError { path, source } => {
    write!(f, "gzip decompression error for '{}': {}", path, source)
}
```

Update `CliError::source()` for the new variant:

```rust
CliError::GzipError { source, .. } => Some(source),
```

### Step 3: Add `read_gramps_file` function to `gramps-reader`

**File:** `crates/gramps-reader/src/lib.rs` (or a new module, e.g. `src/io.rs`)

New function:

```rust
/// Read a `.gramps` file, decompressing if gzip-compressed.
///
/// Detects compression by reading the first two bytes (gzip magic:
/// 0x1f 0x8b). If the file is gzip-compressed, it is decompressed
/// transparently. If it is plain XML, it is read as-is.
///
/// Returns `Err(Error::IoError(...))` on I/O failure or
/// `Err(Error::GzipError(...))` on decompression failure.
pub fn read_gramps_file(path: &str) -> Result<String, Error> {
    // 1. Open the file
    // 2. Read the first 2 bytes (gzip magic)
    // 3. If gzip magic: decompress with flate2::read::GzDecoder
    // 4. If plain: seek back to start and read_to_string
}
```

**Implementation details:**

```rust
use std::fs::File;
use std::io::Read;
use flate2::read::GzDecoder;

pub fn read_gramps_file(path: &str) -> Result<String, Error> {
    let mut file = File::open(path).map_err(|e| Error::IoError {
        path: path.to_string(),
        source: e,
    })?;

    // Read the first two bytes to detect gzip magic.
    let mut magic = [0u8; 2];
    let n = file.read(&mut magic).map_err(|e| Error::IoError {
        path: path.to_string(),
        source: e,
    })?;

    if n < 2 {
        // Empty file (or 1-byte file) — return empty string.
        return Ok(String::new());
    }

    if magic == [0x1f, 0x8b] {
        // Gzip-compressed: decompress via GzDecoder.
        // Use seek(SeekFrom::Start(0)) to rewind instead of reopening
        // the file handle, avoiding a second open + stat call.
        use std::io::SeekFrom;
        file.seek(SeekFrom::Start(0)).map_err(|e| Error::IoError {
            path: path.to_string(),
            source: e,
        })?;
        let mut decoder = GzDecoder::new(file);
        let mut content = String::new();
        decoder.read_to_string(&mut content).map_err(|e| Error::GzipError {
            path: path.to_string(),
            source: e,
        })?;
        Ok(content)
    } else {
        // Plain XML: seek back to start and read the full file.
        use std::io::SeekFrom;
        file.seek(SeekFrom::Start(0)).map_err(|e| Error::IoError {
            path: path.to_string(),
            source: e,
        })?;
        let mut content = String::new();
        file.read_to_string(&mut content).map_err(|e| Error::IoError {
            path: path.to_string(),
            source: e,
        })?;
        Ok(content)
    }
}
```

**Optimization note:** Using `seek(SeekFrom::Start(0))` instead of reopening
the file avoids a redundant open + stat call on the same file. This is a
minor improvement but keeps the code simpler by reusing the same `File` handle.

### Step 4: Update consumers

All consumers currently use `std::fs::read_to_string(path)` directly. Replace
with `gramps_reader::read_gramps_file(path)?`.

#### 4a. `visualize/src/lib.rs`

Two functions to update:

- **`load_graph_data_with_stats()`** — line `let content = std::fs::read_to_string(path)...`
- **`get_stats()`** — line `let content = std::fs::read_to_string(path)...`

Both become:

```rust
let content = gramps_reader::read_gramps_file(path)?;
```

Note: `load_graph_data_with_stats` currently returns `Result<LoadedGraph, String>`
(user-friendly error messages). The `read_gramps_file` function returns
`Result<String, gramps_reader::Error>`, so we need to convert the error:

```rust
let content = gramps_reader::read_gramps_file(path)
    .map_err(|e| format!("Cannot read file '{}': {}", path, e))?;
```

This integrates naturally with the existing error formatting.

#### 4b. `cli/src/commands/stats/mod.rs`

The `run()` function uses `std::fs::read_to_string(file_path)`. Replace with:

```rust
let content = gramps_reader::read_gramps_file(file_path)?;
```

The `From<gramps_reader::Error> for CliError` conversion (added in Step 2)
handles the error type automatically — `IoError` maps to `CliError::Io`,
`GzipError` to `CliError::GzipError`, and `XmlParseError` to `CliError::XmlParseError`.

#### 4c. `cli/src/commands/validate.rs`

Same pattern as stats — replace `std::fs::read_to_string(file_path)` with:

```rust
let content = gramps_reader::read_gramps_file(file_path)?;
```

The `From<gramps_reader::Error>` conversion in `CliError` handles the error
type automatically.

### Step 5: (Merged into Step 2)

The `Error` enum variants, `Display`, `source()`, `CliError::GzipError`, and
`From<gramps_reader::Error>` conversion are all handled in Step 2 above.
No additional work in this step.

### Step 6: Tests

#### Unit tests for `read_gramps_file`

Add tests in `gramps-reader` (either in `lib.rs` or a new `tests/io.rs`):

- **Compressed file:** Read the test file at
  `/home/tr/Documents/gramps01/gramps-ui-gen01.gramps` and verify:
  - Returns `Ok(String)` containing valid XML
  - Content starts with `<?xml`
  - Contains expected elements (e.g., `<event handle="...">`, `<person handle="...">`)
  
- **Uncompressed file (twin fixture):** A plain XML version of the same
  dataset exists at `/home/tr/Documents/gramps01/gramps-ui-gen01.xml`.
  Run `read_gramps_file` on it and verify:
  - Returns the exact same content as `std::fs::read_to_string` on the `.xml`
  - Content is identical to the decompressed output of the `.gramps` file
    (round-trip check)

- **Nonexistent file:** Verify `Err(Error::IoError { .. })` is returned

- **Empty file (0 bytes):** Verify `Ok("")` is returned

- **1-byte file:** Verify `Ok("")` is returned (n < 2 code path).
  Creates a temp file with a single byte `[0x00]`.

- **Corrupted gzip:** Write non-gzip binary data starting with `0x1f 0x8b`
  (e.g., `[0x1f, 0x8b, 0x00, 0x00, ...]`) and verify `Err(Error::GzipError { .. })`
  is returned.

#### `CliError` conversion tests

Add tests in `crates/cli/src/error.rs` (alongside existing `cli_error_*` tests):

- **`IoError` → `CliError::Io`:** Construct a `gramps_reader::Error::IoError`
  and verify the `From` conversion produces `CliError::Io { path, source }`
  with matching fields.

- **`GzipError` → `CliError::GzipError`:** Construct a
  `gramps_reader::Error::GzipError` and verify the conversion produces
  `CliError::GzipError` with matching fields.

- **`source()` chaining:** Verify that `CliError::GzipError` and
  `CliError::Io` return `Some(source)` via `std::error::Error::source()`.

#### `source()` tests for `gramps_reader::Error`

Add tests in `crates/gramps-reader/src/error.rs` (alongside existing
`xml_parse_error_source_is_none`):

- **`IoError` source:** Verify `Error::IoError { .. }` returns `Some(source)`
  via `source()`.

- **`GzipError` source:** Verify `Error::GzipError { .. }` returns
  `Some(source)` via `source()`.

#### Integration tests

- **Visualizer loads compressed file:** Use `load_graph_data()` with the test
  compressed file and verify nodes, links, and family groups are populated
  correctly.

- **Stats on compressed file:** Use `count_gramps_xml` on decompressed content
  and verify counts match expectations.

### Step 7: Verify the full pipeline

1. `gramps-gen stats /home/tr/Documents/gramps01/gramps-ui-gen01.gramps`
   — should produce stats without error
2. `gramps-gen visualize /home/tr/Documents/gramps01/gramps-ui-gen01.gramps`
   — should launch the visualizer with the loaded data
3. `cargo test --workspace` — all existing tests still pass

## Dependency Graph Impact

```
gramps-reader (new: flate2)
├── cli (no change)
├── visualize (no change)
└── typed-graph, output (no change)
```

Only `gramps-reader` gains a new dependency. All consumers are updated to use
the new `read_gramps_file` function, but their dependency sets stay the same.

## File Change Summary

| File | Action |
|---|---|
| `crates/gramps-reader/Cargo.toml` | Add `flate2` dependency |
| `crates/gramps-reader/src/error.rs` | Add `IoError` and `GzipError` variants |
| `crates/gramps-reader/src/lib.rs` | Add `read_gramps_file` function, re-export it |
| `crates/gramps-reader/src/io.rs` | (optional) New module for `read_gramps_file` |
| `crates/cli/src/error.rs` | Add `CliError::GzipError` variant, update `From<gramps_reader::Error>` conversion, update `Display` and `source()` |
| `crates/cli/src/commands/stats/mod.rs` | Use `read_gramps_file` instead of `read_to_string` |
| `crates/cli/src/commands/validate.rs` | Use `read_gramps_file` instead of `read_to_string` |
| `crates/visualize/src/lib.rs` | Use `read_gramps_file` in both `load_graph_data_with_stats` and `get_stats` |

## Open Questions (resolved)

- ~~Detection method: magic bytes vs try/fallback~~ → **Magic bytes** (user confirmed)
- ~~Gzip library: flate2 vs miniz_oxide~~ → **flate2** (user confirmed)
- ~~Extension handling: auto-detect vs .gramps.gz~~ → **Auto-detect by content** (user confirmed)
- ~~Centralization: gramps-reader vs each consumer~~ → **gramps-reader** (user confirmed)
