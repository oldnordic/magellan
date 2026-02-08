---
phase: 60-import-infrastructure
plan: 01
type: execute
wave: 1
depends_on: []
files_modified:
  - src/ingest/mod.rs
  - src/graph/schema.rs
  - src/ingest/imports.rs
  - src/graph/imports.rs
  - src/graph/mod.rs
  - src/graph/ops.rs
  - src/lib.rs
autonomous: true

must_haves:
  truths:
    - "ImportExtractor extracts use, import, from statements during indexing"
    - "Import nodes stored in database with IMPORTS edges to defining symbols"
    - "Import indexing integrated into CodeGraph::index_file() pipeline"
    - "ImportOps follows same Ops pattern as ReferenceOps and CallOps"
  artifacts:
    - path: "src/ingest/imports.rs"
      provides: "Import statement extraction from source code using tree-sitter"
      exports:
        - "ImportFact"
        - "ImportKind"
        - "ImportExtractor"
        - "extract_imports_rust"
        - "extract_imports_python"
      covered_by: "Task 1"
    - path: "src/graph/imports.rs"
      provides: "Import node CRUD operations and IMPORTS edge management"
      exports:
        - "ImportOps"
        - "ImportOps::delete_imports_in_file"
        - "ImportOps::index_imports"
        - "ImportOps::get_imports_for_file"
      covered_by: "Task 2"
    - path: "src/graph/schema.rs"
      provides: "ImportNode schema for persistence"
      exports:
        - "ImportNode"
      covered_by: "Task 1"
  key_links:
    - from: "src/graph/ops.rs::CodeGraph::index_file"
      to: "src/ingest/imports.rs::ImportExtractor"
      via: "Language-specific import extraction during indexing"
      pattern: "extract_imports.*Language::"
    - from: "src/graph/ops.rs::CodeGraph::index_file"
      to: "src/graph/imports.rs::ImportOps::index_imports"
      via: "Store extracted imports as graph nodes with IMPORTS edges"
      pattern: "imports.index_imports"
    - from: "src/graph/mod.rs::CodeGraph"
      to: "src/graph/imports.rs::ImportOps"
      via: "CodeGraph includes imports: ImportOps field"
      pattern: "pub imports: imports::ImportOps"
---

<objective>
Create the import infrastructure foundation for cross-file symbol resolution. This includes extracting import/use/from statements during indexing and storing them as graph nodes with IMPORTS edges.

Purpose: Cross-file symbol resolution requires knowing which symbols each file imports. Without this, reference resolution must guess across all symbols in the database, leading to false positives and missed matches.

Output: ImportExtractor for parsing imports, ImportOps for graph storage, ImportNode schema, and integration into the indexing pipeline.
</objective>

<execution_context>
@/home/feanor/.claude/get-shit-done/workflows/execute-plan.md
@/home/feanor/.claude/get-shit-done/templates/summary.md
</execution_context>

<context>
@.planning/PROJECT.md
@.planning/ROADMAP.md
@.planning/research/ARCHITECTURE.md
@/home/feanor/Projects/magellan/src/ingest/mod.rs
@/home/feanor/Projects/magellan/src/graph/schema.rs
@/home/feanor/Projects/magellan/src/graph/references.rs
@/home/feanor/Projects/magellan/src/graph/call_ops.rs
@/home/feanor/Projects/magellan/src/graph/mod.rs
@/home/feanor/Projects/magellan/src/graph/ops.rs
</context>

<tasks>

<task type="auto">
  <name>Task 1: Create ImportFact and ImportNode schema with ImportExtractor</name>
  <files>src/ingest/mod.rs, src/graph/schema.rs, src/ingest/imports.rs</files>
  <action>
    Create import extraction infrastructure:

    1. In src/ingest/mod.rs, add ImportFact and ImportKind definitions:
       - ImportKind enum: UseCrate, UseSuper, UseSelf, ExternCrate, PlainUse, FromImport, ImportStatement
       - ImportFact struct with: file_path, import_kind, import_path (Vec<String>), imported_names (Vec<String>), is_glob, byte_start, byte_end, start_line, start_col, end_line, end_col
       - Re-export ImportFact and ImportKind from ingest module

    2. In src/graph/schema.rs, add ImportNode struct:
       - Follow same pattern as ReferenceNode and CallNode
       - Fields: file, import_kind, import_path, imported_names, is_glob, byte_start, byte_end, start_line, start_col, end_line, end_col
       - Add serde derives for JSON serialization

    3. Create src/ingest/imports.rs with:
       - pub struct ImportExtractor with tree_sitter::Parser field
       - impl ImportExtractor with: new(), extract_imports() for Rust
       - Walk tree-sitter AST for use_statement, use_declaration, mod_item
       - Parse import_path components (crate::foo::bar -> ["crate", "foo", "bar"])
       - Handle glob imports (use foo::*)
       - Handle renamed imports (use foo as bar)
       - Handle self imports (use self::foo)
       - Return Vec<ImportFact> with proper spans

    4. Add mod imports to src/ingest/mod.rs
    5. Add pub use imports::* to src/ingest/mod.rs

    Follow existing patterns from src/ingest/mod.rs (Parser trait) and src/graph/references.rs (language detection, parser pooling).

    Do NOT add visibility tracking yet (deferred to Phase 60-02 or 61).
  </action>
  <verify>cargo check --all-targets passes with new imports module</verify>
  <done>ImportFact defined in ingest/mod.rs, ImportNode in schema.rs, imports.rs module compiles with extract_imports_rust function</done>
</task>

<task type="auto">
  <name>Task 2: Create ImportOps module for graph storage</name>
  <files>src/graph/imports.rs, src/lib.rs</files>
  <action>
    Create ImportOps following the exact pattern of ReferenceOps and CallOps:

    1. Create src/graph/imports.rs with:
       - pub struct ImportOps { pub backend: Rc<dyn GraphBackend> }
       - impl ImportOps with methods:
         * delete_imports_in_file(&self, path: &str) -> Result<usize>
           - Query all Import nodes by file path
           - Sort entity_ids deterministically
           - Delete in sorted order
           - Return count deleted
         - index_imports(&self, path: &str, imports: Vec<ImportFact>) -> Result<usize>
           - For each ImportFact, insert Import node
           - Create IMPORTS edge from file (or placeholder) to import
           - Return count indexed
         - get_imports_for_file(&self, file_id: i64) -> Result<Vec<ImportFact>>
           - Query imports by file_id
           - Convert nodes back to ImportFact

    2. Follow ReferenceOps pattern exactly:
       - Use NodeSpec for insert (kind: "Import", file_path from ImportFact)
       - Use EdgeSpec for IMPORTS edges
       - Use serde_json for node data serialization
       - Proper error handling with anyhow::Result

    3. Add mod imports to src/lib.rs if needed

    Do NOT integrate with CodeGraph yet (next task).
  </action>
  <verify>cargo check --all-targets passes, ImportOps compiles with delete_imports_in_file and index_imports methods</verify>
  <done>ImportOps module created with CRUD operations following ReferenceOps pattern</done>
</task>

<task type="auto">
  <name>Task 3: Wire ImportOps into CodeGraph and integrate with indexing pipeline</name>
  <files>src/graph/mod.rs, src/graph/ops.rs</files>
  <action>
    Integrate import extraction into the indexing pipeline:

    1. In src/graph/mod.rs CodeGraph struct:
       - Add pub imports: imports::ImportOps field
       - Initialize in open_sqlite() and open_native_v2()
       - Follow same pattern as files, symbols, references, calls fields

    2. In src/graph/ops.rs index_file():
       - After symbol indexing, before reference indexing
       - Extract imports using ImportExtractor based on language:
         * Rust: ImportExtractor::extract_imports_rust
         * Python: ImportExtractor::extract_imports_python (stub returning empty for now)
         * Other languages: empty vec
       - Call imports.delete_imports_in_file(path) first
       - Call imports.index_imports(path, extracted_imports)
       - Handle errors gracefully with .context()

    3. Add imports module to src/graph/mod.rs:
       - mod imports;
       - Keep private (not pub use)

    Follow existing integration pattern from reference/call indexing in ops.rs.

    Do NOT create IMPORTS edges to symbols yet (deferred to Phase 61 - need module resolution first).
  </action>
  <verify>cargo test passes, indexing a Rust file creates Import nodes in database</verify>
  <done>ImportOps integrated into CodeGraph, imports extracted during index_file, Import nodes persisted</done>
</task>

</tasks>

<verification>
After completing all tasks:

1. Create a test Rust file with various import patterns:
   ```rust
   use std::collections::HashMap;
   use crate::my_module::foo;
   use super::parent::bar;
   use self::local::baz;
   ```
2. Run: magellan index --db test.db --root . test_file.rs
3. Query: sqlite3 test.db "SELECT kind, name FROM graph_nodes WHERE kind = 'Import'"
4. Verify: Import nodes exist with correct import_path and imported_names

cargo check --all-targets must pass with no warnings.
</verification>

<success_criteria>
1. ImportFact and ImportKind defined in src/ingest/mod.rs
2. ImportNode schema defined in src/graph/schema.rs
3. src/ingest/imports.rs created with ImportExtractor
4. src/graph/imports.rs created with ImportOps
5. CodeGraph has imports: ImportOps field
6. index_file() extracts and stores imports during indexing
7. cargo check passes
8. Unit tests for ImportExtractor (verify use/import parsing)
</success_criteria>

<output>
After completion, create `.planning/phases/60-import-infrastructure/60-import-infrastructure-01-SUMMARY.md` with:
- Implementation details (ImportFact structure, ImportOps API)
- Test results (import extraction accuracy)
- Files created/modified
- Next steps (Phase 60-02: Module Resolution or 61: Cross-File Resolution)
</output>
