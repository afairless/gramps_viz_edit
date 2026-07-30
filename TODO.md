# Implementation Plan: Fix Schema Version Wiring

Source: `docs/research/fix-schema-version-wiring.md`

| # | Commit message | Logical unit | Key deliverables | Tests |
|---|---|---|---|---|
| 1 | `fix: restore deleted schema-5.2.json from git` | Schema file restoration | `schemas/schema-5.2.json` | — |
| 2 | `fix: wire schema version resolution through full CLI pipeline` | Version-aware pipeline | `crates/cli/src/commands/generate.rs` | Unit, integration |
| 3 | `refactor: deprecate Schema::new() in generated code` | Deprecation annotation | `crates/typed-graph/build.rs` | — |
| 4 | `refactor: migrate typed-graph Schema::new() call sites` | Call site migration | `crates/typed-graph/src/lib.rs`, `graph.rs`, `validate.rs`, `generate/random.rs`, `generate/adversarial.rs`, `generate/builder.rs` | Unit |
| 5 | `refactor: migrate CLI Schema::new() call sites` | Call site migration | `crates/cli/tests/integration.rs` | Unit |
| 6 | `docs: add 5.1 vs 5.2 schema difference report` | Schema diff report | `docs/research/schema-5.1-vs-5.2.md` | — |
| 7 | `feat: make generate_random respect schema valid_enum_values` | Version-aware generation | `crates/typed-graph/src/generate/random.rs` | Unit, property-based |
| 8 | `feat: gate field availability by schema version` (if Phase D reveals differences) | Version-aware generation | `crates/typed-graph/src/generate/random.rs` | Unit |
| 9 | `test: make unit-test version assertions agnostic, annotate E2E assertions` | Version-agnostic tests | `crates/typed-graph/src/lib.rs`, `crates/cli/tests/e2e.rs` | — |
| 10 | `test: add integration tests for version-aware pipeline` | Integration tests | `crates/cli/tests/integration.rs` | Integration, E2E |
