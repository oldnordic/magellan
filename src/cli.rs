//! CLI argument parsing for Magellan
//!
//! Defines the `Command` enum and related types. See `src/cli/parsers.rs`
//! for the actual `parse_args()` implementation.

use magellan::capabilities::BackendCapabilities;
use magellan::graph::export::ExportFilters;
use magellan::graph::query::CollisionField;
use magellan::{ExportFormat, OutputFormat, WatcherConfig};
use std::path::PathBuf;

/// Print short usage (≤25 lines) for quick reference
pub fn print_short_usage() {
    eprint!("{}", include_str!("cli/help_short.txt"));
}

/// Print backend information (available backends, capabilities, features)
pub fn print_backend_info() {
    eprintln!("Magellan Backends");
    eprintln!();

    let all_caps: Vec<_> = BackendCapabilities::enabled_backends()
        .into_iter()
        .map(BackendCapabilities::for_backend)
        .collect();

    // Print table header
    eprintln!(
        "{:<15} {:<10} {:<10} {:<20} Capabilities",
        "Backend", "Enabled", "Extension", "Feature"
    );
    eprintln!("{}", "-".repeat(120));

    for caps in &all_caps {
        let enabled = if caps.build_enabled { "Yes" } else { "No" };
        let feature = caps.required_feature.as_deref().unwrap_or("default");
        let summary = caps.capability_summary();
        // Truncate summary if too long
        let summary_truncated = if summary.len() > 50 {
            format!("{}...", &summary[..47])
        } else {
            summary
        };

        eprintln!(
            "{:<15} {:<10} {:<10} {:<20} {}",
            caps.backend_type.display_name(),
            enabled,
            caps.database_extension_hint,
            feature,
            summary_truncated
        );
    }

    eprintln!();
    eprintln!("Database file extension for the supported public workflow:");
    eprintln!("  .db   - SQLite backend (default, single source of truth)");
    eprintln!();
    eprintln!("Use the SQLite backend (.db) for all commands.");
}

/// Print full usage with all commands and arguments
pub fn print_full_usage() {
    eprint!("{}", include_str!("cli/help_full.txt"));
}

/// Context subcommands
#[derive(Debug)]
pub enum ContextSubcommand {
    /// Build context index
    Build,
    /// Show project summary
    Summary,
    /// List symbols (paginated)
    List {
        kind: Option<String>,
        page: Option<usize>,
        page_size: Option<usize>,
        cursor: Option<String>,
        project: Option<String>,
        output_format: OutputFormat,
    },
    /// Show symbol detail
    Symbol {
        name: String,
        file: Option<String>,
        callers: bool,
        callees: bool,
        output_format: OutputFormat,
        with_source: bool,
        depth: Option<usize>,
        project: Option<String>,
    },
    /// Show file context
    File { path: String },
    /// Impact analysis — blast radius of changing a symbol
    Impact {
        symbol: String,
        file: Option<String>,
        depth: usize,
        project: Option<String>,
        output_format: OutputFormat,
    },
    /// Affected analysis — what the symbol transitively calls
    Affected {
        symbol: String,
        file: Option<String>,
        depth: usize,
        project: Option<String>,
        output_format: OutputFormat,
    },
}

#[derive(Debug)]
#[allow(
    clippy::large_enum_variant,
    reason = "CLI command enum: size differences expected"
)]
pub enum Command {
    Watch {
        root_path: PathBuf,
        db_path: PathBuf,
        config: WatcherConfig,
        scan_initial: bool,
        validate: bool,
        validate_only: bool,
        output_format: OutputFormat,
    },
    Export {
        db_path: PathBuf,
        format: ExportFormat,
        output: Option<PathBuf>,
        include_symbols: bool,
        include_references: bool,
        include_calls: bool,
        minify: bool,
        include_collisions: bool,
        collisions_field: CollisionField,
        filters: ExportFilters,
    },
    ImportLsif {
        db_path: PathBuf,
        lsif_paths: Vec<PathBuf>,
    },
    Backfill {
        db_path: PathBuf,
    },
    CrossFileRefs {
        db_path: PathBuf,
        fqn: String,
        output_format: OutputFormat,
    },
    RegistryScan {
        root: PathBuf,
        output_format: OutputFormat,
    },
    RegistryList {
        root: PathBuf,
        output_format: OutputFormat,
    },
    ConfigShow {
        output_format: OutputFormat,
    },
    ConfigInit {
        force: bool,
    },
    ProjectInit {
        path: Option<PathBuf>,
    },
    Delete {
        db_path: PathBuf,
        file_path: PathBuf,
        root: Option<PathBuf>,
    },
    Index {
        db_path: PathBuf,
        file_path: PathBuf,
        root: Option<PathBuf>,
    },
    IngestCoverage {
        db_path: PathBuf,
        lcov_path: PathBuf,
    },
    Enrich {
        db_path: PathBuf,
        files: Option<Vec<PathBuf>>,
        timeout_secs: u64,
    },
    Context {
        subcommand: ContextSubcommand,
        db_paths: Vec<PathBuf>,
    },
    Doctor {
        db_path: PathBuf,
        fix: bool,
        output_format: OutputFormat,
    },
    #[cfg(feature = "web-ui")]
    WebUi {
        db_path: PathBuf,
        host: String,
        port: u16,
    },
    Status {
        output_format: OutputFormat,
        db_path: PathBuf,
        all: bool,
    },
    Features {
        db_path: PathBuf,
        output_format: OutputFormat,
    },
    ProjectMetadata {
        db_path: PathBuf,
        query: Option<String>,
        output_format: OutputFormat,
    },
    Query {
        db_path: PathBuf,
        file_path: Option<PathBuf>,
        root: Option<PathBuf>,
        kind: Option<String>,
        explain: bool,
        symbol: Option<String>,
        show_extent: bool,
        output_format: OutputFormat,
        with_context: bool,
        with_callers: bool,
        with_callees: bool,
        with_semantics: bool,
        with_checksums: bool,
        context_lines: usize,
    },
    Find {
        db_path: PathBuf,
        name: Option<String>,
        root: Option<PathBuf>,
        path: Option<PathBuf>,
        glob_pattern: Option<String>,
        symbol_id: Option<String>,
        ambiguous_name: Option<String>,
        first: bool,
        output_format: OutputFormat,
        with_context: bool,
        with_callers: bool,
        with_callees: bool,
        with_semantics: bool,
        with_checksums: bool,
        context_lines: usize,
        all: bool,
    },
    Refs {
        db_path: PathBuf,
        name: String,
        root: Option<PathBuf>,
        path: Option<PathBuf>,
        symbol_id: Option<String>,
        direction: String,
        output_format: OutputFormat,
        with_context: bool,
        with_semantics: bool,
        with_checksums: bool,
        context_lines: usize,
        all: bool,
    },
    Get {
        db_path: PathBuf,
        file_path: String,
        symbol_name: String,
        output_format: OutputFormat,
        with_context: bool,
        with_semantics: bool,
        with_checksums: bool,
        context_lines: usize,
    },
    GetFile {
        db_path: PathBuf,
        file_path: String,
        output_format: OutputFormat,
    },
    Files {
        db_path: PathBuf,
        output_format: OutputFormat,
        with_symbols: bool,
    },
    Verify {
        root_path: PathBuf,
        db_path: PathBuf,
        output_format: OutputFormat,
    },
    /// Refresh index based on git changes
    Refresh {
        db_path: PathBuf,
        dry_run: bool,
        include_untracked: bool,
        staged: bool,
        unstaged: bool,
        force: bool,
        output_format: OutputFormat,
    },
    /// Query symbols by label (Phase 2: Label integration)
    Label {
        db_path: PathBuf,
        label: Vec<String>,
        list: bool,
        count: bool,
        show_code: bool,
        output_format: OutputFormat,
    },
    Collisions {
        db_path: PathBuf,
        field: CollisionField,
        limit: usize,
        output_format: OutputFormat,
    },
    Migrate {
        db_path: PathBuf,
        dry_run: bool,
        no_backup: bool,
        output_format: OutputFormat,
    },
    /// Backend migration (Phase 47)
    MigrateBackend {
        input_db: PathBuf,
        output_db: PathBuf,
        export_dir: Option<PathBuf>,
        dry_run: bool,
        output_format: OutputFormat,
    },
    Chunks {
        db_path: PathBuf,
        output_format: OutputFormat,
        limit: Option<usize>,
        file_filter: Option<String>,
        kind_filter: Option<String>,
    },
    ChunkBySpan {
        db_path: PathBuf,
        file_path: String,
        byte_start: usize,
        byte_end: usize,
        output_format: OutputFormat,
    },
    ChunkBySymbol {
        db_path: PathBuf,
        symbol_name: String,
        file_filter: Option<String>,
        output_format: OutputFormat,
    },
    Ast {
        db_path: PathBuf,
        file_path: String,
        position: Option<usize>,
        output_format: OutputFormat,
    },
    FindAst {
        db_path: PathBuf,
        kind: String,
        output_format: OutputFormat,
    },
    /// Reachability analysis (Phase 40)
    Reachable {
        db_path: PathBuf,
        symbol_id: String,
        reverse: bool,
        output_format: OutputFormat,
    },
    /// Dead code detection (Phase 40)
    /// Cycles detection (Phase 40)
    Cycles {
        db_path: PathBuf,
        symbol_id: Option<String>,
        output_format: OutputFormat,
    },
    /// Condensation graph (Phase 40)
    Condense {
        db_path: PathBuf,
        show_members: bool,
        output_format: OutputFormat,
    },
    DeadCode {
        db_path: PathBuf,
        entry_symbol_id: String,
        output_format: OutputFormat,
    },
    /// Path enumeration (Phase 40)
    Paths {
        db_path: PathBuf,
        start_symbol_id: String,
        end_symbol_id: Option<String>,
        max_depth: usize,
        max_paths: usize,
        output_format: OutputFormat,
    },
    /// Program slicing (Phase 40)
    Slice {
        db_path: PathBuf,
        target: String,
        direction: String,
        verbose: bool,
        output_format: OutputFormat,
    },
    /// Source inventory for graph memory (Phase 1)
    SourceInventory {
        db_path: PathBuf,
        scan_dirs: Vec<(PathBuf, String)>,
        list_kind: Option<String>,
        show_stale: bool,
        output_format: OutputFormat,
    },
    /// Candidate fact staging for graph memory (Phase 2)
    CandidateFact {
        db_path: PathBuf,
        action: crate::candidate_fact_cmd::CandidateFactAction,
        output_format: OutputFormat,
    },
    /// Service daemon control (Phase 0)
    Service {
        action: crate::service_cmd::ServiceAction,
        output_format: OutputFormat,
    },
    /// Run the service daemon directly (used by `service start`)
    ServiceDaemon,
    /// Cypher graph query (sqlitegraph 3.0)
    Cypher {
        db_path: PathBuf,
        query: String,
        output_format: OutputFormat,
    },
    /// HNSW vector index creation (sqlitegraph 3.0)
    HnswCreate {
        db_path: PathBuf,
        name: String,
        dim: usize,
        m: usize,
        ef_construction: usize,
        ef_search: usize,
        output_format: OutputFormat,
    },
    /// HNSW vector index query (sqlitegraph 3.0)
    HnswQuery {
        db_path: PathBuf,
        name: String,
        vector: String,
        k: usize,
        output_format: OutputFormat,
    },
    /// Ask — natural-language intent router (Phase 2 UX)
    Ask {
        question: String,
        db_path: PathBuf,
        output_format: OutputFormat,
        all: bool,
    },
    /// Navigate — grounded investigation packet (magellan + llmgrep + mirage)
    Navigate {
        task: String,
        db_path: PathBuf,
        depth: usize,
        budget: usize,
        limit: usize,
        concise: bool,
        with_llmgrep: bool,
        with_mirage: bool,
    },
    /// Telemetry — query performance telemetry events
    Telemetry {
        db_path: PathBuf,
        /// Show recent events
        recent: bool,
        /// Show phase durations for an execution
        phases: Option<String>,
        /// Limit number of results
        limit: usize,
        output_format: OutputFormat,
    },
}

// ============================================================================
// Command Parsers — See src/cli/parsers.rs
// ============================================================================

pub mod parsers;
pub use parsers::*;

#[cfg(test)]
mod tests;
