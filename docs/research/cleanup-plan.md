# Plan: Architecture Cleanup (3 Issues)

> Based on [architecture-cli-review.md](architecture-cli-review.md)
> Issues addressed: #2 (DRY strategy_from_name), #4 (dead extract-schema stub), #5 (dual file parse)

---

## Summary

| Issue | Approach | Files changed |
|---|---|---|
| #2: Duplicate `strategy_from_name` | Move to `AdversarialStrategy::from_name` on the type itself | 3 Rust files |
| #4: Dead `extract-schema` stub | Remove from CLI surface entirely | 3 Rust files |
| #5: Dual file parse in visualize | Read file once, run both `count_gramps_xml` + extraction, return combined result | 4 Rust files, 1 TypeScript file |

All three are independent and can be implemented in any order.

---

## Issue 2: Deduplicate `strategy_from_name`

### Current state

Two identical private functions (same match arms, same default parameters, same aliases):

- `crates/cli/src/commands/generate.rs` line ~398: `fn strategy_from_name(name: &str) -> Option<AdversarialStrategy>`
- `crates/cli/src/scenario.rs` line ~124: `fn strategy_from_name(name: &str) -> Option<AdversarialStrategy>`

Both import `AdversarialStrategy` from `typed_graph::generate`.

### Target state

One canonical location: a constructor on `AdversarialStrategy` itself, in the `typed-graph` crate where the enum is defined.

### Steps

1. **Add `AdversarialStrategy::from_name`** to `crates/typed-graph/src/generate/adversarial.rs`

   Add an `impl` method (or extend the existing `impl AdversarialStrategy` block) with:

   ```rust
   /// Parse a strategy from a human-readable name.
   ///
   /// Accepts hyphenated and underscored aliases for convenience.
   /// Returns `None` for unrecognized names.
   pub fn from_name(name: &str) -> Option<Self> {
       match name {
           "one_parent" | "one-parent" | "one_parent_families" => {
               Some(Self::OneParentFamilies(0.5))
           }
           "missing_events" | "missing-events" => Some(Self::MissingEvents(0.3)),
           "solo" | "solo_persons" | "solo-persons" => Some(Self::SoloPersons(0.2)),
           "many_names" | "many-names" | "many_alternate_names" => {
               Some(Self::ManyAlternateNames(0.3))
           }
           "disconnected" | "disconnected_subgraphs" | "disconnected-subgraphs" => {
               Some(Self::DisconnectedSubgraphs)
           }
           "deep_nesting" | "deep-nesting" => Some(Self::DeepNesting),
           "max_ref_chains" | "max-ref-chains" => Some(Self::MaxRefChains),
           "orphaned" | "orphaned_references" | "orphaned-references" => {
               Some(Self::OrphanedReferences)
           }
           "double_gender" | "double-gender" => Some(Self::DoubleGender(0.2)),
           _ => None,
       }
   }
   ```

2. **Replace call site in `generate.rs`**

   - Delete the private `fn strategy_from_name` (lines ~398–420).
   - In `parse_adversarial_flag`, replace `strategy_from_name(s)` with `AdversarialStrategy::from_name(s)`.
   - In the `ok_or_else` error message for unrecognized strategies, replace the call to the removed private function with `AdversarialStrategy::from_name`. Currently the closure calls `strategy_from_name(s)` on the same input and reports it as unrecognized; the replacement is mechanical: `AdversarialStrategy::from_name(s).ok_or_else(|| …)`.

3. **Replace call site in `scenario.rs`**

   - Delete the private `fn strategy_from_name` (lines ~124–148).
   - In `Scenario::to_adversarial_config`, replace `strategy_from_name(s)` with `AdversarialStrategy::from_name(s)`.

4. **Add unit tests for `from_name`** in `crates/typed-graph/src/generate/adversarial.rs`

   Following the None-One-Many principle:

   ```rust
   #[test]
   fn from_name_unrecognized_returns_none() {
       assert_eq!(AdversarialStrategy::from_name("nonexistent"), None);
       assert_eq!(AdversarialStrategy::from_name(""), None);
   }

   #[test]
   fn from_name_single_hyphenated() {
       assert_eq!(
           AdversarialStrategy::from_name("one-parent"),
           Some(AdversarialStrategy::OneParentFamilies(0.5))
       );
   }

   #[test]
   fn from_name_single_underscored() {
       assert_eq!(
           AdversarialStrategy::from_name("missing_events"),
           Some(AdversarialStrategy::MissingEvents(0.3))
       );
   }

   #[test]
   fn from_name_aliases_map_to_same_variant() {
       let expected = AdversarialStrategy::DisconnectedSubgraphs;
       assert_eq!(AdversarialStrategy::from_name("disconnected"), Some(expected.clone()));
       assert_eq!(AdversarialStrategy::from_name("disconnected_subgraphs"), Some(expected.clone()));
       assert_eq!(AdversarialStrategy::from_name("disconnected-subgraphs"), Some(expected.clone()));
   }

   #[test]
   fn from_name_default_parameters_preserved() {
       // Verify that strategies with fraction parameters use the documented defaults.
       assert_eq!(
           AdversarialStrategy::from_name("one_parent"),
           Some(AdversarialStrategy::OneParentFamilies(0.5))
       );
       assert_eq!(
           AdversarialStrategy::from_name("solo_persons"),
           Some(AdversarialStrategy::SoloPersons(0.2))
       );
   }
   ```

5. **Run tests**

   ```bash
   cargo test -p typed-graph
   cargo test -p cli
   ```

   Existing tests for `parse_adversarial_flag` and scenario parsing catch regressions at the call sites; the new `from_name` tests cover the canonical implementation.

### Risk

**Low.** The function is a pure string→enum mapping. No behavior change, no API change for consumers outside the two call sites.

---

## Issue 4: Remove `extract-schema` Stub

### Current state

`crates/cli/src/commands/extract_schema.rs` contains:

```rust
pub struct ExtractSchemaArgs { pub path: String }

pub fn run(args: ExtractSchemaArgs) -> Result<(), crate::CliError> {
    eprintln!("Extract-schema command stub: path={}", args.path);
    Ok(())
}
```

It is wired into the CLI as `gramps-gen extract-schema <path>` and appears in `--help`.

### Target state

Command removed from the CLI surface. The file is deleted. Users see no mention of `extract-schema` in `--help`.

### Steps

1. **Delete the module file**

   ```bash
   rm crates/cli/src/commands/extract_schema.rs
   ```

2. **Remove from `commands/mod.rs`**

   Delete the line:

   ```rust
   pub mod extract_schema;
   ```

3. **Remove from `main.rs`**

   - Delete the import:

     ```rust
     use cli::commands::extract_schema;
     ```

   - Delete the type alias:

     ```rust
     pub type ExtractSchemaArgs = cli::commands::extract_schema::ExtractSchemaArgs;
     ```

   - Delete the `Command::ExtractSchema` variant from the `Command` enum.
   - Delete the match arm:

     ```rust
     Command::ExtractSchema(args) => extract_schema::run(args)?,
     ```

4. **Run tests and verify**

   ```bash
   cargo test -p cli
   cargo build --release
   # Confirm the command is gone:
   ./target/release/gramps-gen --help | grep -q extract-schema && echo 'FAIL' || echo 'OK'
   ```

   Add an automated regression test in `crates/cli/tests/e2e.rs` to prevent the stub from being reintroduced:

   ```rust
   #[test]
   fn extract_schema_not_in_help() {
       let output = std::process::Command::new("./target/release/gramps-gen")
           .arg("--help")
           .output()
           .unwrap();
       let stdout = String::from_utf8_lossy(&output.stdout);
       assert!(!stdout.contains("extract-schema"), "extract-schema should not appear in --help");
       assert!(!stdout.contains("extract_schema"), "extract_schema should not appear in --help");
   }
   ```

### Risk

**None.** The command does nothing useful. No other code references it.

---

## Issue 5: Eliminate Dual File Parse in Visualize

### Current state

When the user opens `gramps-gen visualize file.gramps`, the following happens:

```
Frontend                    Rust (Tauri IPC)
───────                     ────────────────
invoke("load_graph", ...) → load_graph()
                              1. read_to_string(path)    ← first read
                              2. extract_persons(&str)
                              3. extract_families(&str)
                              4. extract_events(&str)
                              5. build_graph_data(…)
                              6. impute_dates(…)
                              → GraphData

invoke("get_stats", ...)  → get_stats()
                              1. read_to_string(path)    ← second read (redundant)
                              2. count_gramps_xml(&str)  ← fourth XML scan
                              → StatsReport
```

The file is read from disk twice. The XML is scanned four times (count + persons + families + events). The OS page cache makes the second read cheap, but it's still logically redundant.

Also: `count_gramps_xml` internally runs DSU + generation layering, duplicating work that `build_graph_data` (via `compute_generations`) also does.

### Target state

The file is read once. Both `GraphData` and `StatsReport` are produced from the same in-memory `String`. The frontend receives stats alongside the graph data in the initial load, eliminating the need for a separate `get_stats` IPC call (though the IPC command can stay for standalone use).

```
Frontend                    Rust (Tauri IPC)
───────                     ────────────────
invoke("load_graph", ...) → load_graph()
                              1. read_to_string(path)    ← only read
                              2. count_gramps_xml(&str)  ← stats from same content
                              3. extract_persons(&str)
                              4. extract_families(&str)
                              5. extract_events(&str)
                              6. build_graph_data(…)
                              7. impute_dates(…)
                              → LoadedGraph { graph_data, stats }

(no second invoke needed)
```

### Approach (Option A)

Rather than refactoring `count_gramps_xml` to accept pre-extracted data (which would lose counts for non-person/family/event types), run `count_gramps_xml` on the same in-memory `&str` during `load_graph_data`. This adds one more XML scan (count) but eliminates the second file I/O — the real bottleneck.

The DSU/generation work is still duplicated between `count_gramps_xml` and `build_graph_data`, but fixing that is a deeper optimization deferred to a future pass.

### Steps

Each sub-step is independently testable and a single commit.

#### Sub-step 5a: Add `LoadedGraph` struct and `load_graph_data_with_stats` (Rust only)

1. **Verify serde derives** — Confirm that `gramps_reader::StatsReport`, `PrimaryTypeCounts`, etc. already derive `Serialize` and `Deserialize`. (They must — `get_stats` already returns `StatsReport` over Tauri IPC. If not, add the derives.)

2. **Add `LoadedGraph` struct** in `crates/visualize/src/lib.rs`

   ```rust
   /// Combined result of loading a .gramps file: graph data for
   /// rendering plus summary statistics for the stats panel.
   #[derive(Debug, Clone, Serialize, Deserialize)]
   pub struct LoadedGraph {
       pub graph_data: GraphData,
       pub stats: gramps_reader::StatsReport,
   }
   ```

   Re-export from `lib.rs`.

3. **Add `load_graph_data_with_stats` function** in `crates/visualize/src/lib.rs`

   ```rust
   pub fn load_graph_data_with_stats(
       path: &str,
       no_impute: bool,
       generation_gap: u32,
   ) -> Result<LoadedGraph, String> {
       let content = std::fs::read_to_string(path)
           .map_err(|e| format!("Cannot read file '{}': {}", path, e))?;

       // Compute stats from the same in-memory content.
       let stats = gramps_reader::count_gramps_xml(&content)
           .map_err(|e| format!("Failed to parse Gramps XML: {}", e))?;

       // Existing extraction pipeline (unchanged).
       let mut persons = gramps_reader::extract_persons(&content)
           .map_err(|e| format!("Not a valid Gramps XML file: {}", e))?;
       let events = gramps_reader::extract_events(&content)
           .map_err(|e| format!("Not a valid Gramps XML file: {}", e))?;
       let families = gramps_reader::extract_families(&content)
           .map_err(|e| format!("Not a valid Gramps XML file: {}", e))?;

       gramps_reader::resolve_event_refs(&mut persons, &events);

       if persons.is_empty() {
           return Err("No people found in the Gramps file".to_string());
       }

       let mut gd = graph_data::build_graph_data(&persons, &families);

       let imputed = dates::impute_dates(&gd.nodes, &gd.links, generation_gap, no_impute);
       for node in &mut gd.nodes {
           if let Some(Some(year)) = imputed.get(&node.handle) {
               if node.birth_year != Some(*year) {
                   node.birth_year = Some(*year);
                   node.is_imputed = true;
               }
           }
       }

       Ok(LoadedGraph {
           graph_data: gd,
           stats,
       })
   }
   ```

   > **Note**: `load_graph_data` (without stats) delegates to `load_graph_data_with_stats` internally, discarding the stats. This keeps the existing public API backward-compatible, eliminates pipeline duplication, and makes `load_graph_data` trivially testable via delegation:
   >
   > ```rust
   > pub fn load_graph_data(
   >     path: &str,
   >     no_impute: bool,
   >     generation_gap: u32,
   > ) -> Result<GraphData, String> {
   >     load_graph_data_with_stats(path, no_impute, generation_gap)
   >         .map(|loaded| loaded.graph_data)
   > }
   > ```

#### Sub-step 5b: Wire into Tauri IPC

1. **Update Tauri `load_graph` IPC command** in `crates/visualize/src/main.rs`

   Change return type from `Result<GraphData, String>` to `Result<LoadedGraph, String>` and call `load_graph_data_with_stats` instead of `load_graph_data`:

   ```rust
   #[tauri::command]
   fn load_graph(
       path: &str,
       no_impute: bool,
       generation_gap: u32,
   ) -> Result<visualize::LoadedGraph, String> {
       visualize::load_graph_data_with_stats(path, no_impute, generation_gap)
   }
   ```

2. **Keep `get_stats` IPC command** in `crates/visualize/src/main.rs`

   It remains available for standalone use (e.g., refreshing stats without reloading the graph). It still reads the file independently, which is acceptable for its use case.

#### Sub-step 5c: Update frontend to consume `LoadedGraph`

1. **Add `LoadedGraph` to `types.ts`** — Only the new wrapper interface is needed. The `StatsReport`, `PrimaryTypeCounts`, `FamilySizeDistribution`, and `FamilyGroupDistribution` types already exist in `types.ts` (lines 74–118). Add:

   ```typescript
   export interface LoadedGraph {
     graph_data: GraphData;
     stats: StatsReport;
   }
   ```

2. **Update `openAndRenderFile` and `openAndRenderFileFromPath`** in `main.ts` — Both functions call `invoke('load_graph', ...)` and pass the result to `renderGraphFromData`. Change them to destructure `graph_data` and `stats` from the `LoadedGraph` response and pass `stats` through:

   ```typescript
   // In openAndRenderFile:
   const loadedGraph: LoadedGraph = await tauri.invoke('load_graph', {
     path: selected,
     noImpute: noImpute,
     generationGap: gap,
   });
   renderGraphFromData(container, appEl, loadedGraph.graph_data, selected, loadedGraph.stats);

   // In openAndRenderFileFromPath:
   const loadedGraph: LoadedGraph = await tauri.invoke('load_graph', {
     path: filePath,
     noImpute: noImpute,
     generationGap: gap,
   });
   renderGraphFromData(container, appEl, loadedGraph.graph_data, filePath, loadedGraph.stats);
   ```

3. **Update `renderGraphFromData`** to accept an optional `StatsReport` parameter and use it directly instead of calling `fetchAndRenderStats`:

   ```typescript
   function renderGraphFromData(
     container: HTMLElement,
     appEl: HTMLElement,
     graphData: GraphData,
     filePath?: string,
     statsReport?: StatsReport,
   ): void {
     // ... existing graph rendering code ...

     // Stats: use preloaded report when available, otherwise fetch.
     if (statsReport) {
       statsPanel.render(statsReport);
     } else if (filePath) {
       fetchAndRenderStats(filePath);  // fallback for dev mode / standalone
     }
   }
   ```

   The `fetchAndRenderStats` function is kept for standalone `get_stats` use and as a fallback for dev mode (where `window.__GRAPH_DATA__` is injected without stats).

#### Sub-step 5d: Add tests

1. **Update tests** in `crates/visualize/src/lib.rs`

   Existing `load_graph_data` tests continue to pass (the function now delegates to `load_graph_data_with_stats` internally). Add tests for `load_graph_data_with_stats`:

   ```rust
   #[test]
   fn load_graph_data_with_stats_valid_file() {
       // Reuse the existing test XML fixture pattern from load_graph_data_valid_file.
       // Assert that graph_data matches load_graph_data output and stats are populated.
       let loaded = load_graph_data_with_stats(path, false, 25).unwrap();
       assert_eq!(loaded.graph_data.nodes.len(), 3);
       assert!(loaded.stats.counts.people > 0);
       assert!(loaded.stats.counts.families > 0);
   }

   #[test]
   fn load_graph_data_with_stats_nonexistent_file() {
       let result = load_graph_data_with_stats("/nonexistent/path.gramps", false, 25);
       match result {
           Err(msg) => assert!(msg.contains("Cannot read file"), "got: {}", msg),
           Ok(_) => panic!("expected error for nonexistent file"),
       }
   }

   #[test]
   fn load_graph_data_with_stats_malformed_xml() {
       // Malformed XML should produce a stats parse error.
       let result = load_graph_data_with_stats(path, false, 25);
       match result {
           Err(msg) => assert!(msg.contains("Failed to parse Gramps XML"), "got: {}", msg),
           Ok(_) => panic!("expected error for malformed XML"),
       }
   }

   #[test]
   fn load_graph_data_with_stats_empty_file() {
       // Empty file: count_gramps_xml returns default StatsReport (zero counts),
       // but extract_persons finds nothing → "No people found".
       let result = load_graph_data_with_stats(path, false, 25);
       match result {
           Err(msg) => assert!(msg.contains("No people found"), "got: {}", msg),
           Ok(_) => panic!("expected error for empty file"),
       }
   }

   #[test]
   fn load_graph_data_delegates_to_with_stats() {
       // Verify backward compatibility: load_graph_data produces the same
       // GraphData as load_graph_data_with_stats.graph_data.
       let from_old = load_graph_data(path, false, 25).unwrap();
       let from_new = load_graph_data_with_stats(path, false, 25).unwrap();
       assert_eq!(from_old.nodes.len(), from_new.graph_data.nodes.len());
       assert_eq!(from_old.links.len(), from_new.graph_data.links.len());
       assert_eq!(from_old.family_groups.len(), from_new.graph_data.family_groups.len());
   }
   ```

2. **Run full test suite**

   ```bash
   cargo test -p visualize
   cargo test -p gramps-reader
   cargo test -p cli
   cargo build --release --features visualize
   ```

### Risk

**Low-Medium.** The Rust changes are straightforward — calling an existing function on already-loaded data. The frontend changes require updating TypeScript types and data flow, but the IPC contract change (returning a wrapper struct instead of `GraphData` directly) is the only breaking change. The visualizer should be manually smoke-tested after the change.

---

## Implementation Order

All three issues are independent. Recommended order (easiest first):

1. **Issue #4** (remove stub) — trivial, zero risk, builds confidence
2. **Issue #2** (dedup) — small change, tests catch regressions
3. **Issue #5** (dual parse) — most involved, frontend changes needed.

   Commit as 4 sub-steps:
   - 5a: `LoadedGraph` struct + `load_graph_data_with_stats` (Rust only, independently testable)
   - 5b: Wire into Tauri `load_graph` IPC command (Rust only, compile-check)
   - 5c: Frontend TypeScript changes (consume `LoadedGraph`, pass stats to panel)
   - 5d: Tests (delegation, error propagation, backward compatibility)

Each can be a separate commit following conventional commit conventions (e.g., `refactor: deduplicate strategy_from_name into AdversarialStrategy::from_name`).

---

## Dependencies

- Issue #5 depends on `serde` derive macros for the new `LoadedGraph` struct — already available in the visualize crate via workspace dependencies.
- No new dependencies are needed for any issue.
- No changes to `Cargo.toml` files are required.
