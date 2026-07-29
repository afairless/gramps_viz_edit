# Implementation Plan: Phase 7 — Scenario Runner + CLI

Source: `docs/research/design.md`

## Phase 7 Scope

Phase 7 builds the CLI binary and YAML scenario runner on top of the completed Phases 1–6 (schema extraction/codegen, graph core/validation, GraphBuilder API, XML serializer, random generation, adversarial generation). This phase wires all components together into a user-facing tool.

Key deliverables:

- **`crates/output/`** — XML serialization crate that walks a validated `Graph` and produces Gramps XML (`.gramps`) output using `quick-xml` and a `SerializationMap`.
- **`crates/cli/`** — CLI binary using `clap` with `generate`, `validate`, and `extract-schema` commands.
- **YAML scenario support** — `serde_yaml`-based config file parsing that drives the generation pipeline.
- **Pipeline wiring** — Generate → Validate → Adversarial Transform → Validate → Serialize, wired end-to-end.
- **Error handling** — I/O errors, config parsing errors, validation failures, all with clear user-facing messages.
- **Progress reporting** — Periodic progress output for large generation runs.
- **Documentation and README** — Usage examples, crate-level docs, and installation instructions.

**Key design references**: design §7.5 (Configuration schema), §8 (XML Serialization), §9 (CLI Interface), §10 Phase 7, §11 (Pipeline model), §12 (Error handling), §13 (Dependencies).

| # | Commit message | Logical unit | Key deliverables | Tests |
|---|---|---|---|---|
| 1 | `feat: create output crate with SerializationMap and XML scaffold` | Output crate scaffold | `crates/output/Cargo.toml`, `crates/output/src/lib.rs`, `crates/output/src/xml.rs` | Unit, Smoke |
| 2 | `feat: implement Graph-to-XML serializer for all primary types` | XML serialization — primary types | `crates/output/src/xml.rs`, `crates/output/src/serialization_map.rs` | Unit |
| 3 | `feat: implement Graph-to-XML serializer for embedded refs and mixins` | XML serialization — refs and mixins | `crates/output/src/xml.rs` | Unit |
| 4 | `feat: implement XML output ordering, header, and document structure` | XML document structure | `crates/output/src/xml.rs` | Unit |
| 5 | `feat: create CLI crate with clap argument parsing and subcommands` | CLI scaffold | `crates/cli/Cargo.toml`, `crates/cli/src/main.rs` | Smoke |
| 6 | `feat: implement generate command — wire generation, validation, serialization` | Generate command | `crates/cli/src/commands/generate.rs`, `crates/cli/src/main.rs` | Integration |
| 7 | `feat: implement YAML scenario file parsing for generation config` | YAML scenario runner | `crates/cli/src/scenario.rs`, `crates/cli/src/commands/generate.rs` | Unit |
| 8 | `feat: add --strict flag for plausibility warnings as errors` | Strict mode | `crates/cli/src/commands/generate.rs` | Unit |
| 9 | `feat: add progress reporting for large generation runs` | Progress reporting | `crates/cli/src/progress.rs`, `crates/cli/src/commands/generate.rs` | Unit |
| 10 | `feat: implement validate command for .gramps file validation` | Validate command | `crates/cli/src/commands/validate.rs`, `crates/cli/src/main.rs` | Unit |
| 11 | `feat: add error handling for I/O, config, and validation failures` | Error handling | `crates/cli/src/error.rs`, `crates/cli/src/commands/generate.rs`, `crates/cli/src/commands/validate.rs` | Unit |
| 12 | `docs: add README with installation, usage, and examples` | README and documentation | `README.md`, crate-level docs | — |
| 13 | `test: add end-to-end integration tests for CLI generate and validate` | Integration tests | `tests/cli/e2e.rs` | Integration |

### Step 1 — Output crate scaffold

- Create `crates/output/Cargo.toml`:

  ```toml
  [package]
  name = "output"
  version = "0.1.0"
  edition = "2021"

  [dependencies]
  typed-graph = { path = "../typed-graph" }
  quick-xml = "0.36"
  ```

- Add `crates/output` to workspace `Cargo.toml` members: `members = ["crates/typed-graph", "crates/output"]`.

- Create `crates/output/src/lib.rs` with module declarations:

  ```rust
  //! Gramps XML output for generated genealogy graphs.
  //!
  //! This crate walks a validated [`Graph`] and produces Gramps XML
  //! (`.gramps` format) following the RelaxNG schema.

  pub mod serialization_map;
  pub mod xml;

  pub use xml::GraphXmlWriter;
  ```

- Create `crates/output/src/serialization_map.rs` with the `SerializationMap` struct:

  ```rust
  /// Maps Graph types to their XML element and attribute names.
  ///
  /// This follows the Gramps XML RelaxNG schema. Person → `"person"` element,
  /// family → `"family"` element, etc. Hand-coded initially; the design
  /// (Decision 5) describes extracting this from the RelaxNG schema at
  /// build time in a future iteration.
  pub struct SerializationMap {
      /// Maps primary type name to its XML element info.
      pub type_map: std::collections::HashMap<String, XmlTypeInfo>,
      /// Maps edge variant name to the XML nesting and attributes.
      pub edge_map: std::collections::HashMap<String, XmlEdgeInfo>,
      /// Order in which type sections appear in the XML output.
      pub section_order: Vec<String>,
  }

  pub struct XmlTypeInfo {
      pub element_name: String,
      pub section_name: String,
      pub attributes: Vec<XmlAttribute>,
      pub children: Vec<XmlChild>,
  }

  pub struct XmlEdgeInfo {
      pub parent_element: String,
      pub element_name: String,
      pub attributes: Vec<(String, String)>,
  }

  pub struct XmlAttribute {
      pub field: String,
      pub attr_name: String,
  }

  pub struct XmlChild {
      pub element_name: String,
      pub source: XmlChildSource,
  }

  pub enum XmlChildSource {
      InlineStruct(String),
      Array(String),
      Edge(String),
  }
  ```

- Implement `SerializationMap::new()` that builds the mapping for all 10 primary types. Map each field from the generated data structs to its XML attribute/element name. This is a hand-coded initial version (RelaxNG extraction is deferred to future work per design §8).

- Create `crates/output/src/xml.rs` with:

  ```rust
  /// Writes a Graph to Gramps XML.
  pub struct GraphXmlWriter {
      map: SerializationMap,
  }

  impl GraphXmlWriter {
      pub fn new(map: SerializationMap) -> Self { ... }

      /// Serialize the graph to the given writer.
      pub fn write(&self, graph: &Graph, writer: &mut impl std::io::Write) -> Result<(), SerializationError> { ... }
  }
  ```

- Define `SerializationError` enum:

  ```rust
  /// Errors that can occur during XML serialization.
  #[derive(Debug)]
  pub enum SerializationError {
      Io(std::io::Error),
      UnsupportedType(String),
      MissingRequiredField { handle: String, field: &'static str },
  }

  impl std::fmt::Display for SerializationError { ... }
  impl std::error::Error for SerializationError { ... }
  impl From<std::io::Error> for SerializationError { ... }
  ```

- **Tests**:
  - `serialization_map_new_has_all_primary_types`: `SerializationMap::new()` contains entries for all 10 primary types.
  - `serialization_map_person_mapping`: Person maps to element name `"person"`, section `"people"`, has `handle` and `gramps_id` attributes.
  - `serialization_map_section_order`: Section order follows the Gramps XML schema (tags, events, people, families, citations, sources, places, objects, repositories, notes).
  - `serialization_map_edge_exists`: Edge variants like `PersonFamily` have an entry in the edge map.
  - `serialization_map_display_and_error_traits`: Error types implement Display and Error.
  - `xml_writer_new`: `GraphXmlWriter::new(...)` creates a writer.
  - `xml_writer_empty_graph`: Writing an empty graph produces valid XML with header and empty sections.
  - Smoke: `cargo build --workspace` compiles.

#### Step 2 — XML serialization — primary types

- In `xml.rs`, implement the core serialization logic for all 10 primary type nodes. The serializer walks the graph's nodes grouped by type, emitting XML elements following the `SerializationMap`:

  ```rust
  fn write_section(
      &self,
      graph: &Graph,
      writer: &mut impl std::io::Write,
      type_name: &str,
      section_name: &str,
  ) -> Result<(), SerializationError> { ... }

  fn write_node(
      &self,
      node: &Node,
      type_info: &XmlTypeInfo,
      writer: &mut impl std::io::Write,
  ) -> Result<(), SerializationError> { ... }

  fn write_field_as_attribute(
      &self,
      node: &Node,
      attr: &XmlAttribute,
      writer: &mut impl std::io::Write,
  ) -> Result<(), SerializationError> { ... }

  fn write_children(
      &self,
      graph: &Graph,
      handle: &str,
      children: &[XmlChild],
      writer: &mut impl std::io::Write,
  ) -> Result<(), SerializationError> { ... }
  ```

  **Algorithm for each primary type**:
  - Look up the type's `XmlTypeInfo` in the serialization map.
  - Open the XML element (e.g., `<person handle="..." id="...">`).
  - For each attribute in `type_info.attributes`, write the corresponding field from the node's data struct as an XML attribute.
  - For each child in `type_info.children`, write the corresponding inline elements (e.g., `<name>`, `<eventref>`, `<citationref>`).
  - Handle handle-ref fields by writing `hlink` attributes (e.g., `<father hlink="..."/>`).
  - Close the XML element.

  **Handle `Option<T>` fields** — skip `None` values.

  **String escaping** — rely on `quick-xml`'s built-in XML escaping for all string values. For manual fallback code, escape `&` → `&amp;`, `<` → `&lt;`, `>` → `&gt;`, `"` → `&quot;`, `'` → `&apos;`.

- **Tests**:
  - `serialize_person_element`: A single Person node produces a `<person>` element with handle and gramps_id attributes.
  - `serialize_person_optional_attributes`: Optional attributes (e.g., `gramps_id: None`) are omitted from the output.
  - `serialize_family_element`: A Family node produces a `<family>` element.
  - `serialize_event_element`: An Event node produces an `<event>` element with `<eventtype>` child.
  - `serialize_place_element`: A Place node produces a `<placeobj>` element.
  - `serialize_source_element`: A Source node produces a `<source>` element.
  - `serialize_citation_element`: A Citation node produces a `<citation>` element.
  - `serialize_repository_element`: A Repository node produces a `<repository>` element.
  - `serialize_media_element`: A Media object produces an `<object>` element.
  - `serialize_note_element`: A Note node produces a `<note>` element.
  - `serialize_tag_element`: A Tag node produces a `<tag>` element.
  - `serialize_empty_person_omits_optional_nested`: A Person with no alternate names emits no `<name>` alternatives.

#### Step 3 — XML serialization — embedded refs and mixins

- Implement serialization of **embedded reference** edges (edges with metadata like `PersonEventRef`, `FamilyChildRef`, `PersonPersonRef`, `FamilyEventRef`, `SourceRepoRef`):

  ```rust
  fn write_embedded_ref(
      &self,
      edge: &Edge,
      writer: &mut impl std::io::Write,
  ) -> Result<(), SerializationError> { ... }
  ```

  **Algorithm**:
  - Look up the edge variant in the `SerializationMap`'s edge map.
  - Open the XML element (e.g., `<eventref>`).
  - Write the `hlink` attribute pointing to the target node's handle.
  - Write metadata fields as child elements (e.g., `<role>`, `<attribute>`).
  - Close the XML element.

- Implement serialization of **mixin edges** (CitationRef, NoteRef, MediaRef, TagRef). These appear as child elements within the source node's section:

  ```rust
  fn write_mixin_ref(
      &self,
      edge: &Edge,
      writer: &mut impl std::io::Write,
  ) -> Result<(), SerializationError> {
      // Write <citationref hlink="..."/>, <noteref hlink="..."/>, etc.
  }
  ```

- Handle inline secondary objects (embedded `EventRef`, `ChildRef`, `PersonRef`, `RepoRef` metadata). These carry their own fields (role, relation, etc.) that must be serialized as child elements of the ref element.

- **Tests**:
  - `serialize_person_eventref`: A Person with an EventRef produces `<eventref hlink="...">` with `<role>` child.
  - `serialize_family_childref`: A Family with a ChildRef produces `<childref hlink="...">` with optional `<rel>` attribute.
  - `serialize_person_personref`: A Person with a PersonRef produces `<personref hlink="...">`.
  - `serialize_source_reporef`: A Source with a RepoRef produces `<reporef hlink="...">`.
  - `serialize_citationref`: A mixin CitationRef edge produces `<citationref hlink="..."/>`.
  - `serialize_noteref`: A mixin NoteRef edge produces `<noteref hlink="..."/>`.
  - `serialize_mediaref`: A mixin MediaRef edge produces `<mediaref hlink="..."/>`.
  - `serialize_tagref`: A mixin TagRef edge produces `<tagref hlink="..."/>`.
  - `serialize_multiple_refs`: Multiple refs of the same type are all emitted.

#### Step 4 — XML output ordering, header, and document structure

- Implement the full document structure for Gramps XML. The output follows this ordering (design §8):

  ```xml
  <?xml version="1.0" encoding="UTF-8"?>
  <database xmlns="http://gramps-project.org/xml/1.7.2/">
    <header>
      <created date="..." version="5.2"/>
      <researcher><resname>Generated by gramps-gen</resname></researcher>
    </header>
    <tags>...</tags>
    <events>...</events>
    <people>...</people>
    <families>...</families>
    <citations>...</citations>
    <sources>...</sources>
    <places>...</places>
    <objects>...</objects>
    <repositories>...</repositories>
    <notes>...</notes>
  </database>
  ```

- Implement the `GraphXmlWriter::write()` method to:

  1. Write the XML declaration with encoding and version.
  2. Open the `<database>` element with the Gramps XML namespace.
  3. Write the `<header>` section with generation timestamp and researcher info:

     ```rust
     fn write_header(&self, writer: &mut impl std::io::Write) -> Result<(), SerializationError> {
         // <created date="YYYY-MM-DD" version="5.2"/>
         // <researcher><resname>Generated by gramps-gen</resname></researcher>
     }
     ```

  4. Write each section in `section_order`, skipping empty sections.
  5. Close the `<database>` element.

- **Streaming approach**: Write directly to the `Writer` without building the full XML in memory. Use `quick-xml`'s `Writer` API for proper XML escaping and formatting.

- **Tests**:
  - `xml_document_structure_complete`: Full document output starts with `<?xml`, has `<database>`, `<header>`, closes properly.
  - `xml_header_content`: Header section contains `<created>` with date and version, `<researcher>` with name.
  - `xml_section_order_is_correct`: Sections appear in the expected order per the Gramps XML schema.
  - `xml_empty_sections_omitted`: Empty sections (e.g., no tags) are omitted from output.
  - `xml_escapes_special_characters`: String values with `&`, `<`, `>` are properly escaped.
  - `xml_roundtrip_single_person`: A graph with one Person validates as well-formed XML (parse with a basic XML reader).

#### Step 5 — CLI crate scaffold

- Create `crates/cli/Cargo.toml`:

  ```toml
  [package]
  name = "cli"
  version = "0.1.0"
  edition = "2021"

  [[bin]]
  name = "gramps-gen"
  path = "src/main.rs"

  [dependencies]
  typed-graph = { path = "../typed-graph" }
  output = { path = "../output" }
  clap = { version = "4", features = ["derive"] }
  serde = { workspace = true }
  serde_yaml = "0.9"
  log = "0.4"
  env_logger = "0.11"
  ```

- Add `crates/cli` to workspace `Cargo.toml` members.

- Create `crates/cli/src/main.rs` with clap argument parsing:

  ```rust
  use clap::Parser;
  use clap::Subcommand;

  #[derive(Parser)]
  #[command(name = "gramps-gen", about = "Generate valid Gramps family tree datasets", version)]
  struct Cli {
      #[command(subcommand)]
      command: Command,
  }

  #[derive(Subcommand)]
  enum Command {
      /// Generate a random family tree dataset
      Generate(GenerateArgs),
      /// Validate a .gramps file
      Validate(ValidateArgs),
      /// Extract the schema from a Gramps installation
      ExtractSchema(ExtractSchemaArgs),
  }
  ```

- Define `GenerateArgs`, `ValidateArgs`, and `ExtractSchemaArgs` structs. The `GenerateArgs` matches the CLI spec from design §9:

  ```rust
  #[derive(clap::Args)]
  struct GenerateArgs {
      /// Number of persons to generate
      #[arg(short = 'n', long, default_value = "200")]
      count: usize,

      /// Number of generations
      #[arg(short = 'd', long, default_value = "3")]
      depth: usize,

      /// Output .gramps file
      #[arg(short = 'o', long, default_value = "output.gramps")]
      output: String,

      /// RNG seed for reproducible generation
      #[arg(long)]
      seed: Option<u64>,

      /// Promote plausibility warnings to errors
      #[arg(long)]
      strict: bool,

      /// Comma-separated adversarial strategies, or "all"
      #[arg(long)]
      adversarial: Option<String>,

      /// How often to report generation progress
      #[arg(long, default_value = "100")]
      progress_interval: usize,

      /// YAML scenario file (overrides other options)
      #[arg(short = 'c', long)]
      config: Option<String>,

      // Feature flags
      #[arg(long)]
      with_places: bool,
      #[arg(long)]
      with_citations: bool,
      #[arg(long)]
      with_notes: bool,
      #[arg(long)]
      with_media: bool,
      #[arg(long)]
      with_tags: bool,
  }
  ```

- `ValidateArgs`:

  ```rust
  #[derive(clap::Args)]
  struct ValidateArgs {
      /// Path to a .gramps file to validate
      file: String,

      /// Promote plausibility warnings to errors
      #[arg(long)]
      strict: bool,
  }
  ```

- `ExtractSchemaArgs`:

  ```rust
  #[derive(clap::Args)]
  struct ExtractSchemaArgs {
      /// Path to a local Gramps source repository
      path: String,
  }
  ```

- Implement the main dispatch:

  ```rust
  fn main() {
      env_logger::init();
      let cli = Cli::parse();
      match cli.command {
          Command::Generate(args) => commands::generate::run(args),
          Command::Validate(args) => commands::validate::run(args),
          Command::ExtractSchema(args) => commands::extract_schema::run(args),
      }
  }
  ```

- Create `crates/cli/src/commands/mod.rs` with `pub mod generate; pub mod validate; pub mod extract_schema;`.

- Create stub implementations for each command that print a message (smoke test).

- **Tests**:
  - `cli_parse_generate_short_args`: `gramps-gen -n 50 -d 4 -o test.gramps` parses correctly.
  - `cli_parse_generate_long_args`: `gramps-gen --count 50 --depth 4 --output test.gramps` parses correctly.
  - `cli_parse_generate_defaults`: No args uses defaults (count=200, depth=3, output=output.gramps).
  - `cli_parse_generate_with_seed`: `--seed 42` parses as Some(42).
  - `cli_parse_validate_file`: `gramps-gen validate input.gramps` parses file path.
  - `cli_parse_extract_schema`: `gramps-gen extract-schema ~/src/gramps` parses path.
  - `cli_parse_generate_config_flag`: `-c scenario.yaml` parses config file path.
  - `cli_parse_generate_strict`: `--strict` sets strict flag.
  - `cli_parse_generate_adversarial`: `--adversarial all` sets strategies list.
  - `cli_no_subcommand_prints_help`: Running with no args prints help.
  - Smoke: `cargo build --workspace` compiles.

#### Step 6 — Generate command — wire generation, validation, serialization

- In `crates/cli/src/commands/generate.rs`, implement the full `run()` function that wires the five-stage pipeline:

  ```rust
  use typed_graph::generate::{
      generate_random, AdversarialConfig, AdversarialStrategy, RandomConfig,
  };
  use typed_graph::validate::validate;
  use typed_graph::Schema;

  pub fn run(args: GenerateArgs) -> Result<(), CliError> {
      // 1. Build config (from args or scenario file)
      let (config, adversarial_config, output_path) = build_config(&args)?;

      // 2. Create schema
      let schema = Schema::new();

      // 3. Generate (Stage 1)
      let result = generate_random(&config, &adversarial_config, &schema)?;

      // Report seed
      eprintln!("Generation seed: {}", result.seed);

      // 4. Validate (Gate 1) — generation includes adversarial transforms
      let errors = result.graph.validate(&schema);

      // Check for validation errors (Gate 2 check)
      if !errors.is_empty() {
          if args.strict || errors.iter().any(|e| !matches!(e, typed_graph::ValidationError::PlausibilityWarning { .. })) {
              // Report errors to stderr as JSON Lines
              for error in &errors {
                  eprintln!("{}", serde_json::to_string(error).unwrap());
              }
              return Err(CliError::ValidationFailed(errors));
          }
          // Otherwise report as warnings
          for error in &errors {
              if matches!(error, typed_graph::ValidationError::PlausibilityWarning { .. }) {
                  eprintln!("Warning: {}", error);
              }
          }
      }

      // 5. Serialize (Stage 5)
      let map = output::SerializationMap::new();
      let writer = output::GraphXmlWriter::new(map);
      let file = std::fs::File::create(&output_path)
          .map_err(|e| CliError::Io { path: output_path.clone(), source: e })?;
      writer.write(&result.graph, &mut std::io::BufWriter::new(file))?;

      // Report summary
      let stats = &result.stats;
      eprintln!(
          "Generated {} persons, {} families, {} events ({} edges) → {}",
          stats.person_count, stats.family_count, stats.event_count,
          stats.edge_count, output_path
      );

      // Report warnings
      for warning in &result.warnings {
          eprintln!("Warning: {}", warning);
      }

      Ok(())
  }
  ```

- Implement the `build_config` helper:

  ```rust
  fn build_config(args: &GenerateArgs) -> Result<(RandomConfig, AdversarialConfig, String), CliError> { ... }
  ```

  - If `args.config` is `Some(path)`, load the YAML scenario file and parse it into config.
  - Otherwise, build from CLI args directly.
  - Parse the `--adversarial` flag string into a list of `AdversarialStrategy` variants.

- **Tests**:
  - `generate_command_build_config_from_args`: Building config from explicit args produces expected `RandomConfig`.
  - `generate_command_adversarial_flag_parses_all`: `--adversarial all` maps to all strategy variants.
  - `generate_command_adversarial_flag_parses_list`: `--adversarial disconnected,one-parent` parses correctly.
  - `generate_command_adversarial_flag_unknown_rejected`: Unknown strategy name returns an error.
  - `generate_command_invalid_seed_handled`: Invalid seed value produces an error.
  - `generate_command_empty_output_path`: Empty output path returns error. (Requires non-empty paths at minimum.)
  - `generate_command_zero_count_rejected`: `--count 0` returns a config error.
  - Integration: Run `gramps-gen generate --count 5 --output /tmp/test.gramps`, verify file exists and is non-empty XML.

#### Step 7 — YAML scenario file parsing

- Create `crates/cli/src/scenario.rs` with the YAML scenario types:

  ```rust
  use serde::Deserialize;

  /// A scenario configuration loaded from a YAML file.
  /// Maps to the schema in design §7.5.
  #[derive(Debug, Deserialize)]
  pub struct Scenario {
      pub name: Option<String>,
      pub person_count: Option<usize>,
      pub family_count: Option<usize>,
      pub generations: Option<GenerationsConfig>,
      pub date_range: Option<DateRangeConfig>,
      pub with_citations: Option<bool>,
      pub with_places: Option<bool>,
      pub with_media: Option<bool>,
      pub with_notes: Option<bool>,
      pub with_tags: Option<bool>,
      pub seed: Option<u64>,
      pub adversarial: Option<AdversarialScenarioConfig>,
  }

  #[derive(Debug, Deserialize)]
  pub struct GenerationsConfig {
      pub depth: Option<usize>,
      pub children_per_family: Option<ChildrenRange>,
  }

  #[derive(Debug, Deserialize)]
  pub struct ChildrenRange {
      pub min: usize,
      pub max: usize,
  }

  #[derive(Debug, Deserialize)]
  pub struct DateRangeConfig {
      pub start: Option<i32>,
      pub end: Option<i32>,
      pub era: Option<String>,
  }

  #[derive(Debug, Deserialize)]
  pub struct AdversarialScenarioConfig {
      pub enabled: Option<bool>,
      pub strategies: Option<Vec<String>>,
  }
  ```

- Implement a function to load a scenario from a file path:

  ```rust
  /// Load a scenario from a YAML file.
  pub fn load_scenario(path: &str) -> Result<Scenario, ScenarioError> {
      let file = std::fs::File::open(path)
          .map_err(|e| ScenarioError::Io { path: path.to_string(), source: e })?;
      let reader = std::io::BufReader::new(file);
      serde_yaml::from_reader(reader)
          .map_err(|e| ScenarioError::ParseError { path: path.to_string(), source: e })
  }
  ```

- Define `ScenarioError`:

  ```rust
  #[derive(Debug)]
  pub enum ScenarioError {
      Io { path: String, source: std::io::Error },
      ParseError { path: String, source: serde_yaml::Error },
  }

  impl std::fmt::Display for ScenarioError { ... }
  impl std::error::Error for ScenarioError { ... }
  ```

- Implement `Scenario::to_random_config()` and `Scenario::to_adversarial_config()` that produce `RandomConfig` and `AdversarialConfig` respectively, using defaults for any unset fields:

  ```rust
  impl Scenario {
      pub fn to_random_config(&self) -> RandomConfig {
          let base = RandomConfig::default();
          RandomConfig {
              person_count: self.person_count.unwrap_or(base.person_count),
              family_count: self.family_count.unwrap_or(base.family_count),
              generations: self.generations.as_ref().map(|g| g.depth.unwrap_or(base.generations)).unwrap_or(base.generations),
              children_per_family: self.generations.as_ref()
                  .and_then(|g| g.children_per_family.as_ref())
                  .map(|r| r.min..r.max)
                  .unwrap_or(base.children_per_family),
              start_year: self.date_range.as_ref().map(|d| d.start.unwrap_or(base.start_year)).unwrap_or(base.start_year),
              end_year: self.date_range.as_ref().map(|d| d.end.unwrap_or(base.end_year)).unwrap_or(base.end_year),
              name_style: self.date_range.as_ref()
                  .and_then(|d| d.era.clone())
                  .unwrap_or(base.name_style),
              with_places: self.with_places.unwrap_or(base.with_places),
              with_citations: self.with_citations.unwrap_or(base.with_citations),
              with_notes: self.with_notes.unwrap_or(base.with_notes),
              with_media: self.with_media.unwrap_or(base.with_media),
              with_tags: self.with_tags.unwrap_or(base.with_tags),
              seed: self.seed,
              place_depth: base.place_depth,
          }
      }

      pub fn to_adversarial_config(&self) -> AdversarialConfig { ... }
  }
  ```

- **Tests**:
  - `scenario_parse_full_yaml`: Parse a complete YAML string with all fields.
  - `scenario_parse_minimal_yaml`: Parse a minimal YAML with just `person_count`.
  - `scenario_parse_invalid_yaml`: Invalid YAML returns `ScenarioError::ParseError`.
  - `scenario_file_not_found`: Non-existent file returns `ScenarioError::Io`.
  - `scenario_to_random_config_full`: Full scenario produces expected `RandomConfig` with all overrides.
  - `scenario_to_random_config_defaults`: Minimal scenario uses defaults for unset fields.
  - `scenario_to_adversarial_config_enabled`: Scenario with adversarial enabled returns enabled config with strategies.
  - `scenario_to_adversarial_config_disabled`: Scenario without adversarial returns disabled config.
  - `scenario_example_three_generation_tree`: Parse the example YAML from design §7.5.
  - Wire: `gramps-gen generate -c scenario.yaml` loads and applies the scenario.

#### Step 8 — `--strict` flag for plausibility warnings

- In `crates/cli/src/commands/generate.rs`, implement strict-mode logic:

  ```rust
  pub fn run(args: GenerateArgs) -> Result<(), CliError> {
      // ... generation ...

      // Validate
      let errors = result.graph.validate(&schema);

      if args.strict {
          // In strict mode, ALL errors (including PlausibilityWarning) are blocking
          if !errors.is_empty() {
              for error in &errors {
                  eprintln!("{}", serde_json::to_string(error).unwrap());
              }
              return Err(CliError::ValidationFailed(errors));
          }
      } else {
          // In non-strict mode, only structural/referential errors are blocking
          let blocking_errors: Vec<_> = errors.iter()
              .filter(|e| !matches!(e, typed_graph::ValidationError::PlausibilityWarning { .. }))
              .collect();
          if !blocking_errors.is_empty() {
              for error in &blocking_errors {
                  eprintln!("{}", serde_json::to_string(error).unwrap());
              }
              return Err(CliError::ValidationFailed(blocking_errors.into_iter().cloned().collect()));
          }
          // Plausibility warnings are reported but non-blocking
          for error in &errors {
              if matches!(error, typed_graph::ValidationError::PlausibilityWarning { .. }) {
                  eprintln!("Warning: {}", error);
              }
          }
      }

      // ... serialization ...
  }
  ```

- **Tests**:
  - `strict_mode_promotes_warnings_to_errors`: With `--strict` and a graph that has plausibility warnings, the command exits with an error.
  - `non_strict_mode_warnings_non_blocking`: Without `--strict`, plausibility warnings are printed but do not block serialization.
  - `strict_mode_structural_errors_still_blocking`: Structural errors (dangling refs) block regardless of `--strict`.
  - `strict_mode_no_warnings_passes`: A graph with no warnings passes strict validation.

#### Step 9 — Progress reporting

- Create `crates/cli/src/progress.rs` with a progress reporter:

  ```rust
  /// Reports generation progress to stderr.
  ///
  /// The reporter prints a progress line every `interval` persons
  /// during generation, showing the current count and total.
  pub struct ProgressReporter {
      /// How often to report (every N persons).
      interval: usize,
      /// Total number of persons to generate.
      total: usize,
      /// Current count of generated persons.
      current: usize,
  }

  impl ProgressReporter {
      pub fn new(interval: usize, total: usize) -> Self { ... }

      /// Advance the counter by one and print progress if at interval.
      pub fn tick(&mut self) { ... }

      /// Print the final progress line.
      pub fn finish(&self) { ... }
  }
  ```

- Integrate progress reporting into the generation loop. Since `generate_random()` already handles generation internally, the progress reporter is used in two ways:

  1. **Pre-generation**: Print a starting message: `"Generating {} persons...".format(config.person_count)`.
  2. **Post-generation**: Report completion with stats.

  For very large generations (>10k persons), the reporter can be wired into the generation loop. Since `generate_random()` is a single function call, there are two approaches:
  - **Option A (recommended)**: Add a progress callback to `generate_random()` that the CLI can provide. This keeps progress reporting in the CLI.
  - **Option B**: Report progress before and after the call (simpler, adequate for <10k persons).

  For the initial implementation, use **Option B** (pre/post reporting) plus the existing statistics output. Option A can be deferred as an enhancement.

  ```rust
  fn run(args: GenerateArgs) -> Result<(), CliError> {
      // ...
      eprintln!("Generating {} persons across {} generations...", config.person_count, config.generations);
      let result = generate_random(&config, &adversarial_config, &schema)?;
      // ...
  }
  ```

  For large generations (configurable via `--progress-interval`), wrap the generation in a thread that periodically checks progress. Since the generation internals aren't yet instrumented with callbacks, use the simple pre/post approach for now and note the enhancement opportunity.

- **Tests**:
  - `progress_reporter_new`: Reporter initializes with correct interval and total.
  - `progress_reporter_tick_silent_below_interval`: Tick does not print until interval is reached.
  - `progress_reporter_tick_output_at_interval`: Tick prints at the interval boundary.
  - `progress_reporter_finish_output`: Finish prints the final message.
  - `progress_reporter_zero_interval`: interval=0 disables progress output (prints nothing).
  - `progress_reporter_tick_exact_boundary`: Reporting at exact `interval` triggers output.
  - `progress_reporter_tick_above_interval`: Reporting at `interval + 3` triggers output at the right points.

#### Step 10 — Validate command

- In `crates/cli/src/commands/validate.rs`, implement validation of `.gramps` files:

  ```rust
  use typed_graph::validate::validate;
  use typed_graph::Schema;

  pub fn run(args: ValidateArgs) -> Result<(), CliError> {
      let file_path = &args.file;

      // Read the .gramps file
      let content = std::fs::read_to_string(file_path)
          .map_err(|e| CliError::Io { path: file_path.clone(), source: e })?;

      // Parse the XML and reconstruct a Graph
      let graph = parse_gramps_xml(&content)
          .map_err(|e| CliError::ValidationFailed(vec![
              typed_graph::ValidationError::PlausibilityWarning {
                  node: "root".to_string(),
                  message: format!("Failed to parse .gramps file: {}", e),
              }
          ]))?;

      // Validate
      let schema = Schema::new();
      let errors = graph.validate(&schema);

      if errors.is_empty() {
          eprintln!("{}: valid", file_path);
          Ok(())
      } else if args.strict {
          // Report all errors (including plausibility) as blocking
          for error in &errors {
              eprintln!("{}", serde_json::to_string(error).unwrap());
          }
          Err(CliError::ValidationFailed(errors))
      } else {
          // Separate blocking errors from warnings
          let blocking: Vec<_> = errors.iter()
              .filter(|e| !matches!(e, typed_graph::ValidationError::PlausibilityWarning { .. }))
              .collect();
          if blocking.is_empty() {
              eprintln!("{}: valid with {} warnings", file_path, errors.len());
              for error in &errors {
                  eprintln!("Warning: {}", error);
              }
              Ok(())
          } else {
              for error in &blocking {
                  eprintln!("{}", serde_json::to_string(error).unwrap());
              }
              Err(CliError::ValidationFailed(blocking.into_iter().cloned().collect()))
          }
      }
  }
  ```

  **Note**: Full `.gramps` file parsing (reconstructing a `Graph` from Gramps XML) is a significant effort that goes beyond Phase 7's scope. For the initial `validate` command, implement a **minimal XML structure check**:
  - Verify the file is well-formed XML.
  - Verify it has a `<database>` root element with the expected namespace.
  - Verify it has a `<header>` section.
  - Report structural issues (missing required sections, malformed XML).
  - Full graph reconstruction from XML is deferred as a future enhancement.

  ```rust
  /// Minimal XML structure validation for .gramps files.
  /// Checks well-formedness and expected elements without full graph reconstruction.
  fn validate_gramps_xml_structure(content: &str) -> Result<(), String> {
      use quick_xml::Reader;
      use quick_xml::events::Event;

      let mut reader = Reader::from_str(content);
      reader.config_mut().trim_text(true);

      let mut has_database = false;
      let mut has_header = false;
      let mut depth = 0u32;

      loop {
          match reader.read_event() {
              Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                  let name = String::from_utf8_lossy(e.name().as_ref());
                  match name.as_ref() {
                      "database" => has_database = true,
                      "header" => has_header = true,
                      _ => {}
                  }
                  depth += 1;
              }
              Ok(Event::End(_)) => depth -= 1,
              Ok(Event::Eof) => break,
              Err(e) => return Err(format!("XML parse error: {}", e)),
              _ => {}
          }
      }

      if !has_database {
          return Err("Missing <database> root element".to_string());
      }
      Ok(())
  }
  ```

- **Tests**:
  - `validate_command_valid_file`: A valid `.gramps` file structure returns success.
  - `validate_command_missing_database`: A file without `<database>` reports an error.
  - `validate_command_not_xml`: A non-XML file reports an error.
  - `validate_command_file_not_found`: A non-existent file returns `CliError::Io`.
  - `validate_command_strict_mode`: `--strict` works with validate command.
  - `validate_command_with_warnings`: A file with plausibility warnings prints them.
  - Integration: `gramps-gen generate --count 5 --output /tmp/test.gramps && gramps-gen validate /tmp/test.gramps` succeeds.

#### Step 11 — Error handling

- Create `crates/cli/src/error.rs` with the unified error type:

  ```rust
  use std::path::PathBuf;

  /// Unified CLI error type covering all failure modes.
  #[derive(Debug)]
  pub enum CliError {
      /// I/O error with context.
      Io { path: String, source: std::io::Error },
      /// Configuration parsing error (CLI args or YAML scenario).
      ConfigError(String),
      /// Generation failure (exhausted constraints, invalid config).
      GenerationFailed(typed_graph::generate::GenerationError),
      /// Validation found errors (structural, referential, or plausibility
      /// in strict mode).
      ValidationFailed(Vec<typed_graph::ValidationError>),
      /// Serialization failure (I/O during XML writing, unsupported types).
      SerializationFailed(output::SerializationError),
      /// Scenario file parse error.
      ScenarioError(crate::scenario::ScenarioError),
  }

  impl std::fmt::Display for CliError {
      fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
          match self {
              CliError::Io { path, source } => {
                  write!(f, "I/O error for '{}': {}", path, source)
              }
              CliError::ConfigError(msg) => {
                  write!(f, "configuration error: {}", msg)
              }
              CliError::GenerationFailed(e) => {
                  write!(f, "generation failed: {}", e)
              }
              CliError::ValidationFailed(errors) => {
                  write!(f, "validation failed with {} error(s)", errors.len())
              }
              CliError::SerializationFailed(e) => {
                  write!(f, "serialization failed: {}", e)
              }
              CliError::ScenarioError(e) => {
                  write!(f, "scenario error: {}", e)
              }
          }
      }
  }

  impl std::error::Error for CliError {
      fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
          match self {
              CliError::Io { source, .. } => Some(source),
              CliError::GenerationFailed(e) => Some(e),
              CliError::SerializationFailed(e) => Some(e),
              CliError::ScenarioError(e) => Some(e),
              _ => None,
          }
      }
  }

  // From implementations for easy error conversion
  impl From<std::io::Error> for CliError { ... }
  impl From<typed_graph::generate::GenerationError> for CliError { ... }
  impl From<output::SerializationError> for CliError { ... }
  impl From<crate::scenario::ScenarioError> for CliError { ... }
  ```

- In `main.rs`, use the error type in the dispatch:

  ```rust
  fn main() -> Result<(), CliError> {
      env_logger::init();
      let cli = Cli::parse();
      match cli.command {
          Command::Generate(args) => commands::generate::run(args)?,
          Command::Validate(args) => commands::validate::run(args)?,
          Command::ExtractSchema(args) => commands::extract_schema::run(args)?,
      }
      Ok(())
  }
  ```

- Ensure all error output uses the format from design §12:
  - **Validation errors**: Written to stderr as JSON Lines (one error per line).
  - **I/O errors**: Include file path and operation (read/write).
  - **Config parsing errors**: Include line/column information (from serde_yaml).
  - **Generation errors**: Include the seed for reproducibility.

- Add `--seed` recording: print the seed to stderr at generation start and embed it in the output XML metadata.

- **Tests**:
  - `cli_error_io_display`: I/O error with path and source produces an actionable message.
  - `cli_error_config_display`: Config error produces a clear message.
  - `cli_error_validation_display`: Validation error shows count.
  - `cli_error_generation_display`: Generation error shows message and seed.
  - `cli_error_serialization_display`: Serialization error shows message.
  - `cli_error_scenario_display`: Scenario error shows path and source.
  - `cli_error_io_from_std`: `std::io::Error` converts to `CliError::Io`.
  - `cli_error_from_generation`: `GenerationError` converts to `CliError::GenerationFailed`.
  - `cli_error_from_serialization`: `SerializationError` converts to `CliError::SerializationFailed`.
  - `cli_error_from_scenario`: `ScenarioError` converts to `CliError::ScenarioError`.
  - `cli_error_source_chain`: `.source()` returns the inner error for wrapped types.

#### Step 12 — README and documentation

- Write `README.md` in the project root with:

  ```
  # Gramps Data Generator

  A tool that generates valid, plausible Gramps family tree datasets
  for testing and development.

  ## Installation

  ```bash
  cargo install --path .
  ```

  ## Usage

  ### Basic generation

  ```bash
  gramps-gen generate --count 200 --depth 3 --output family.gramps
  ```

  ### Reproducible generation with seed

  ```bash
  gramps-gen generate --count 1000 --seed 42 --output reproducible.gramps
  ```

  ### With all optional features

  ```bash
  gramps-gen generate --count 500 --with-places --with-citations --with-notes
  ```

  ### Adversarial datasets

  ```bash
  # All strategies
  gramps-gen generate --count 100 --adversarial all

  # Specific strategies
  gramps-gen generate --count 100 --adversarial disconnected,one-parent

  # Strict mode (warnings as errors)
  gramps-gen generate --count 100 --adversarial all --strict
  ```

  ### Using a YAML scenario file

  ```bash
  gramps-gen generate --config scenario.yaml
  ```

  ### Validate a .gramps file

  ```bash
  gramps-gen validate family.gramps
  ```

  ## Commands

  | Command | Description |
  |---|---|
  | `generate` | Generate a random family tree dataset |
  | `validate` | Validate a .gramps file structure |
  | `extract-schema` | Extract schema from a Gramps installation |

  ## Pipeline

  Generate → Validate → [Adversarial Transform] → Validate → Serialize

  See [docs/research/design.md](docs/research/design.md) for the full architecture.

  ## Security

  The `extract-schema` command imports and executes Python code from the provided
  path. Only point it at a trusted Gramps source checkout.

  ```

- Add crate-level documentation in each crate's `lib.rs`:

  - `typed-graph`: Document the graph model, schema types, validation, and generation.
  - `output`: Document XML serialization and the `SerializationMap`.
  - `cli`: Document the CLI interface and pipeline wiring.

- **Tests**: `—` (no tests for documentation).

#### Step 13 — End-to-end integration tests

- Create `tests/cli/e2e.rs`:

  ```rust
  /// End-to-end tests for the gramps-gen CLI.
  ///
  /// These tests run the CLI binary as a subprocess and verify its output.
  /// They test the full pipeline: generate → validate → serialize.

  use std::process::Command;

  /// Helper to run gramps-gen with arguments.
  fn gramps_gen(args: &[&str]) -> (String, String, Option<i32>) {
      let output = Command::new(env!("CARGO_BIN_EXE_gramps-gen"))
          .args(args)
          .output()
          .expect("Failed to run gramps-gen");
      let stdout = String::from_utf8_lossy(&output.stdout).to_string();
      let stderr = String::from_utf8_lossy(&output.stderr).to_string();
      let code = output.status.code();
      (stdout, stderr, code)
  }
  ```

  **Tests**:
  - `e2e_generate_basic`: `gramps-gen generate --count 10 --output /tmp/e2e_test.gramps` exits 0, produces a non-empty `.gramps` file.
  - `e2e_generate_with_seed`: `--seed 42` produces reproducible output (same seed → same file).
  - `e2e_generate_with_adversarial`: `--adversarial disconnected` runs without error.
  - `e2e_generate_with_all_features`: `--with-places --with-citations --with-notes --with-tags` runs without error.
  - `e2e_validate_generated_file`: `gramps-gen validate` on a generated file exits 0.
  - `e2e_generate_invalid_args_zero_count`: `--count 0` exits with error.
  - `e2e_generate_gen_roundtrip`: Generate → validate produces consistent stats.
  - `e2e_scenario_file`: Generate with `-c` YAML scenario file produces expected output.
  - `e2e_generate_large`: `--count 1000 --depth 10` runs within reasonable time (< 30s).
  - `e2e_generate_all_adversarial`: `--adversarial all` on a moderate-sized graph produces output with no hard crashes.

### Key design references

- **Configuration schema**: design §7.5 — YAML scenario format with `person_count`, `generations`, `date_range`, `adversarial`, etc.
- **XML Serialization**: design §8 — Gramps XML structure, section ordering, serialization map, RelaxNG mapping (hand-coded initial version per Decision 5 fallback).
- **CLI Interface**: design §9 — Three commands (generate, validate, extract-schema), all flags and options.
- **Pipeline model**: design §11 — Five-stage pipeline: Generate → Validate → Adversarial Transform → Validate → Serialize, with validation gates after every data-altering stage.
- **Error handling**: design §12 — Error message format (handle, constraint, fix), JSON Lines validation errors on stderr, seed recording.
- **Dependencies**: design §13 — `clap` (4.x) for CLI, `serde_yaml` for scenario parsing, `quick-xml` for XML, `log` + `env_logger` for logging.
- **Testing strategy**: design §15 — Unit tests per component, integration tests (CLI end-to-end), performance benchmarks at scale.
- **Phase dependencies**: design §10 Phase 7 notes — Depends on Phase 3 (GraphBuilder API), Phase 4 (XML serializer), Phase 5 (Random generation), Phase 6 (Adversarial generation). The output crate (`crates/output/`) is created in this phase if the XML serializer from Phase 4 is not yet present as a standalone crate.
