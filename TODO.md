# Implementation Plan: `gramps-gen stats` subcommand

Source: `docs/research/gramps-file-stats.md`

| # | Commit message | Logical unit | Key deliverables | Tests |
|---|---|---|---|---|
| 1 | `feat: add XmlParseError variant to CliError` | XmlParseError variant | `crates/cli/src/error.rs` — add `XmlParseError` variant, Display impl, `std::error::Error::source`, unit tests | Unit |
| 2 | `feat: add StatsArgs, command skeleton, and CLI wiring` | Command skeleton | `crates/cli/src/commands/stats/mod.rs` (StatsArgs, stub `run()`), `crates/cli/src/commands/stats/count.rs` (stub StatsReport, stub `count_gramps_xml`), `crates/cli/src/commands/mod.rs` (pub mod stats), `crates/cli/src/main.rs` (Command::Stats) | Unit — file-not-found → Io, malformed XML → XmlParseError, empty content → zeroed report |
| 3 | `feat: implement streaming counters for all 10 primary types` | Primary-type counters | `crates/cli/src/commands/stats/count.rs` — full streaming scan for `person`, `family`, `event`, `place`, `source`, `citation`, `repository`, `object`, `note`, `tag`; namespace-prefix stripping | Unit — all 10 types counted, zero counts in empty `<database/>`, self-closing `<person/>`, namespace-prefixed input |
| 4 | `feat: implement family-size histogram and people-not-in-family` | Family-size & ref tracking | `crates/cli/src/commands/stats/count.rs` — `HashSet<String>` tracking for person handles, family-ref union, per-family size histogram, `people_not_in_family`, `dangling_refs` | Unit — example scenario (families of 10,10,3,3,3,3,3 + 8 isolated), duplicate refs, empty family (size 0), dangling refs |
| 5 | `feat: implement human-readable report formatting and --json output` | Report formatting | `crates/cli/src/commands/stats/mod.rs` — text formatter (aligned columns, plural-aware, ascending histogram); `StatsReport` derives Serialize/Deserialize; `run()` branches on `args.json` | Unit — exact text output for a small report, JSON round-trip through serde, `--json` includes all expected fields |
| 6 | `feat: add integration and E2E tests for stats command` | Integration & E2E tests | `crates/cli/tests/integration.rs` — build graph with GraphBuilder, serialize with GraphXmlWriter, run `count_gramps_xml` on bytes, assert report; `crates/cli/tests/e2e.rs` — subprocess `gramps-gen stats` against generated file, text and `--json` | Integration, Smoke |
| 7 | `docs: update README and AGENTS.md for stats command` | Documentation | `README.md` — new command in usage section; `AGENTS.md` — workspace structure mentions `stats`, CLI table gains command | — |
