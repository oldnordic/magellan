//! LLM Context Query Interface
//!
//! Provides summarized, paginated context for LLMs instead of full exports.
//!
//! # Design Principles
//!
//! 1. **Summarized**: High-level overviews, not raw data dumps
//! 2. **Paginated**: Chunked into context-window-sized pieces
//! 3. **Queryable**: LLM requests what it needs, not everything
//!
//! # Usage
//!
//! ```bash
//! # Build context index (once per project)
//! magellan context build --db code.db
//!
//! # Query for context
//! magellan context summary --db code.db           # Project overview (~50 tokens)
//! magellan context list --db code.db --kind fn    # List functions, paginated
//! magellan context symbol --db code.db --name main  # Details on main()
//! magellan context callers --db code.db --name main # What calls main()
//! ```
//!
//! # Multi-Level Summaries
//!
//! | Level | Content | Token Budget |
//! |-------|---------|--------------|
//! | Project | "Rust CLI tool, 143 files, 2271 symbols" | ~50 |
//! | File | "src/main.rs: 26 symbols, 3 pub fn" | ~100 |
//! | Symbol | "main(): fn() -> i32, calls: [x, y]" | ~150 |
//! | Full | Complete signature + docs + call graph | ~500 |

pub mod build;
pub mod query;
pub mod server;

pub use build::{build_context_index, get_or_build_context_index, ContextIndex};
pub use query::{
    ListQuery, PaginatedResult, SymbolDetail, FileContext, ProjectSummary, SymbolCounts,
    get_project_summary, get_file_context, get_symbol_detail, list_symbols,
};
pub use server::{run_context_server, ServerConfig};
