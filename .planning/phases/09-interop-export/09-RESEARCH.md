# Phase 9: Interop Export (SCIP / LSIF) - Research

**Researched:** 2026-01-19
**Domain:** SCIP protocol, protobuf serialization, symbol identity mapping
**Confidence:** HIGH

## Summary

Phase 9 implements export to SCIP (Source Code Intelligence Protocol), enabling interoperability with standard code intelligence tools. The research reveals:

1. **SCIP is the industry standard for code indexing**: Developed by Sourcegraph as a successor to LSIF, SCIP provides 8x smaller file sizes and 3x faster processing compared to LSIF. LSIF is now deprecated.

2. **Official `scip` crate provides Rust bindings**: The crate at version 0.6.1 includes pre-generated protobuf types using `prost` (protobuf 3.7.2). No need to generate types from `.proto` files manually.

3. **SCIP uses UTF-8 byte offsets for Rust**: The `PositionEncoding.UTF8CodeUnitOffsetFromLineStart` enum value matches Magellan's existing span model (half-open byte offsets). Line/col must be 0-indexed for SCIP.

4. **Symbol format is structured**: SCIP symbols use the format `scheme package manager/name version descriptors...` with descriptors using suffixes like `/` (namespace), `#` (type), `.` (term), `()` (method).

5. **Existing export pattern from Phase 7**: The codebase has a well-established export pattern in `src/graph/export.rs` with `ExportFormat` enum and `export_graph` function that can be extended for SCIP.

**Primary recommendation:** Add `scip` crate as dependency, extend `ExportFormat` enum with `Scip` variant, create `scip` submodule following the existing export pattern, and implement mapping from Magellan's SymbolNode/ReferenceNode to SCIP Document/Occurrence messages.

## Standard Stack

### Core (New Dependencies Required)
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `scip` | 0.6.1 | SCIP protobuf types (prost-generated) | Official Sourcegraph crate, provides all SCIP message types |
| `protobuf` | 3.7.2 | Runtime dependency via scip crate | Required for serialization |

### Already in Use (No Changes)
| Library | Current | Purpose | Reuse |
|---------|---------|---------|-------|
| `anyhow` | 1.0 | Error handling | Already used throughout codebase |
| `serde` | 1.0 | JSON serialization (not used for SCIP binary) | Existing patterns |

**Installation:**
```bash
# Add to Cargo.toml [dependencies]
scip = "0.6.1"
```

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| scip crate | Generate types from scip.proto directly | scip crate is pre-built, actively maintained by Sourcegraph |

### LSIF Deferred
Per CONTEXT.md decision, LSIF export is deferred to v2. Reasons:
- LSIF is deprecated (Sourcegraph officially recommends SCIP)
- SCIP provides better performance (8x smaller, 3x faster)
- SCIP is actively maintained, LSIF is legacy

## Architecture Patterns

### Recommended Project Structure
```
src/graph/
├── export.rs           # Extend with SCIP support
│   ├── mod exports::json (existing)
│   ├── mod exports::jsonl (existing)
│   ├── mod exports::dot (existing)
│   ├── mod exports::csv (existing)
│   └── mod exports::scip (NEW)
└── ...

src/
├── export_cmd.rs       # Extend with SCIP format handling
└── main.rs             # Extend CLI with --format scip
```

### Pattern 1: Extend ExportFormat Enum
**What:** Add `Scip` variant to existing `ExportFormat` enum.

**When to use:** Core export entrypoint for SCIP format.

**Example:**
```rust
// Source: src/graph/export.rs (extended)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Json,
    JsonL,
    Dot,
    Csv,
    Scip,  // NEW
}

impl ExportFormat {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "json" => Some(ExportFormat::Json),
            "jsonl" => Some(ExportFormat::JsonL),
            "dot" => Some(ExportFormat::Dot),
            "csv" => Some(ExportFormat::Csv),
            "scip" => Some(ExportFormat::Scip),  // NEW
            _ => None,
        }
    }
}
```

### Pattern 2: SCIP Index Structure
**What:** Create SCIP Index message with Metadata and Documents.

**When to use:** Main SCIP export function.

**Example:**
```rust
// Source: src/graph/export/scip.rs (NEW)
use anyhow::Result;
use scip::types as scip;
use crate::graph::{CodeGraph, SymbolNode, ReferenceNode};

pub fn export_scip(graph: &mut CodeGraph, project_root: &str) -> Result<Vec<u8>> {
    let mut index = scip::Index {
        metadata: Some(scip::Metadata {
            version: scip::ProtocolVersion::UnspecifiedProtocolVersion.into(),
            tool_info: Some(scip::ToolInfo {
                name: "magellan".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
                arguments: vec![],
            }),
            project_root: project_root.to_string(),
            text_document_encoding: scip::TextEncoding::Utf8.into(),
        }),
        documents: vec![],
        external_symbols: vec![],
    };

    // Group symbols by file
    let mut documents_by_file: std::collections::HashMap<String, Vec<SymbolNode>> = ...;

    // Convert each file's symbols to a SCIP Document
    for (file_path, symbols) in documents_by_file {
        let document = to_scip_document(graph, &file_path, symbols)?;
        index.documents.push(document);
    }

    // Serialize to protobuf binary
    let mut buffer = Vec::new();
    index.encode(&mut buffer)?;
    Ok(buffer)
}
```

### Pattern 3: Symbol to SCIP Symbol Mapping
**What:** Map Magellan's symbol_id + name to SCIP Symbol format.

**When to use:** Converting SymbolNode to SCIP Occurrence.

**Example:**
```rust
// Source: src/graph/export/scip.rs
/// Convert Magellan symbol to SCIP symbol string
///
/// SCIP symbol format: "scheme manager/name version descriptors..."
/// For Magellan v1, use simplified format: "magellan . . . symbol_name"
///
/// Format breakdown:
/// - scheme: "magellan"
/// - package manager: "." (placeholder for no package manager)
/// - package name: "." (placeholder for no package name)
/// - version: "." (placeholder for no version)
/// - descriptors: "." (term suffix) + symbol_name
fn to_scip_symbol(symbol_name: &str) -> String {
    // Escape spaces in symbol name with double space
    let escaped = symbol_name.replace(' ', "  ");
    format!("magellan . . . {}", escaped)
}

/// For fully-qualified symbols with module path
fn to_scip_symbol_qualified(symbol_name: &str, module_path: &str) -> String {
    let escaped_name = symbol_name.replace(' ', "  ");
    let escaped_module = module_path.replace(' ', "  ");
    // Use namespace (/) for module, term (.) for symbol
    format!("magellan . . . {}/{}", escaped_module, escaped_name)
}
```

### Pattern 4: Span to SCIP Range Conversion
**What:** Convert Magellan's half-open byte offsets + 1-indexed line/col to SCIP's 0-indexed range.

**When to use:** Converting SymbolNode spans to SCIP Occurrence ranges.

**Example:**
```rust
// Source: src/graph/export/scip.rs
/// Convert Magellan span to SCIP range
///
/// SCIP range format: [startLine, startCharacter, endLine, endCharacter]
/// All values are 0-based.
///
/// Magellan uses:
/// - 1-indexed line numbers
/// - Byte-based columns (matching SCIP UTF8CodeUnitOffsetFromLineStart)
/// - Half-open ranges [start, end)
fn to_scip_range(
    start_line: usize,  // Magellan: 1-indexed
    start_col: usize,   // Byte offset within line
    end_line: usize,    // Magellan: 1-indexed
    end_col: usize,     // Byte offset within line
) -> Vec<i32> {
    vec![
        (start_line - 1) as i32,   // Convert to 0-indexed
        start_col as i32,
        (end_line - 1) as i32,     // Convert to 0-indexed
        end_col as i32,
    ]
}

/// Same-line span optimization (3-element range)
fn to_scip_range_same_line(
    start_line: usize,
    start_col: usize,
    end_col: usize,
) -> Vec<i32> {
    vec![
        (start_line - 1) as i32,
        start_col as i32,
        end_col as i32,
    ]
}
```

### Pattern 5: Document with Occurrences
**What:** Create SCIP Document with Occurrences for symbols and references.

**When to use:** Converting file contents to SCIP.

**Example:**
```rust
// Source: src/graph/export/scip.rs
use scip::types::{Document, Occurrence, SymbolInformation, SymbolRole, PositionEncoding};

fn to_scip_document(
    graph: &mut CodeGraph,
    file_path: &str,
    symbols: Vec<SymbolNode>,
) -> Result<Document> {
    let mut occurrences = vec![];
    let symbol_infos = vec![];

    // Detect language from file extension
    let language = detect_language_from_path(file_path);

    for symbol in symbols {
        let scip_symbol = to_scip_symbol_qualified(
            symbol.name.as_deref().unwrap_or(""),
            &extract_module_path(&symbol.kind),
        );

        // Definition occurrence
        let range = to_scip_range(
            symbol.start_line,
            symbol.start_col,
            symbol.end_line,
            symbol.end_col,
        );

        occurrences.push(Occurrence {
            range,
            symbol: scip_symbol.clone(),
            symbol_roles: (SymbolRole::Definition as i32),
            ..Default::default()
        });

        // SymbolInformation (optional, for metadata)
        symbol_infos.push(SymbolInformation {
            symbol: scip_symbol,
            kind: to_scip_kind(&symbol.kind),
            display_name: symbol.name,
            ..Default::default()
        });
    }

    // Add references from ReferenceNode
    let refs = get_references_for_file(graph, file_path)?;
    for reference in refs {
        let range = to_scip_range(
            reference.start_line as usize,
            reference.start_col as usize,
            reference.end_line as usize,
            reference.end_col as usize,
        );

        occurrences.push(Occurrence {
            range,
            symbol: to_scip_symbol(&reference.referenced_symbol),
            symbol_roles: 0,  // Not a definition
            ..Default::default()
        });
    }

    Ok(Document {
        relative_path: to_relative_path(file_path)?,
        language,
        occurrences,
        symbols: symbol_infos,
        position_encoding: PositionEncoding::Utf8CodeUnitOffsetFromLineStart as i32,
        ..Default::default()
    })
}
```

### Anti-Patterns to Avoid
- **Mixing position encodings:** Always use `UTF8CodeUnitOffsetFromLineStart` for Rust. SCIP supports UTF-16 and UTF-32, but Magellan uses byte offsets.
- **1-indexed line numbers in SCIP:** SCIP requires 0-indexed lines. Magellan uses 1-indexed, must convert.
- **Forgetting to escape spaces:** SCIP symbol format uses double space to escape literal spaces.
- **Omitting required fields:** `Document.relative_path` and `Document.language` are required. Missing them causes parse errors.
- **Using deprecated LSIF:** Sourcegraph has deprecated LSIF in favor of SCIP. Don't implement LSIF for v1.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| SCIP protobuf types | Manual Message definitions | `scip` crate v0.6.1 | Pre-generated from official scip.proto, maintained by Sourcegraph |
| Protobuf serialization | Manual binary encoding | `prost::Message::encode()` | scip crate includes prost, handles wire format correctly |
| Symbol format parsing | Custom string parsers | SCIP format is documented but v1 can use simple format | Full descriptor parsing is complex; for v1, simplified format sufficient |

**Key insight:** The `scip` crate provides all necessary types. SCIP's symbol format is powerful but complex for v1. Use a simplified format `magellan . . . symbol_name` that can be extended later.

## Common Pitfalls

### Pitfall 1: Line Number Index Mismatch
**What goes wrong:** SCIP consumers report incorrect positions or reject the index entirely.

**Why it happens:** Magellan uses 1-indexed line numbers (user-friendly, matches editors). SCIP requires 0-indexed lines.

**How to avoid:** Always subtract 1 from line numbers when creating SCIP ranges:
```rust
// WRONG - using Magellan lines directly
let range = vec![start_line, start_col, end_line, end_col];

// RIGHT - convert to 0-indexed
let range = vec![start_line - 1, start_col, end_line - 1, end_col];
```

**Warning signs:** "Position out of bounds" errors in SCIP consumers, positions appearing one line off.

### Pitfall 2: Position Encoding Mismatch
**What goes wrong:** Column positions are incorrect for multi-byte UTF-8 characters.

**Why it happens:** SCIP has three position encodings (UTF-8, UTF-16, UTF-32). Must match what's declared in `Document.position_encoding`.

**How to avoid:** Always use `PositionEncoding::Utf8CodeUnitOffsetFromLineStart` and set it in Document:
```rust
Document {
    position_encoding: PositionEncoding::Utf8CodeUnitOffsetFromLineStart as i32,
    // ...
}
```

**Warning signs:** Non-ASCII characters (emoji, CJK) have incorrect column positions.

### Pitfall 3: Invalid Relative Path Format
**What goes wrong:** SCIP consumer rejects documents with "invalid path" errors.

**Why it happens:** SCIP has strict requirements for `Document.relative_path`:
- Must not start with `/`
- Must use `/` separator (even on Windows)
- Must not contain `..` or empty components `//`
- Must be relative to `Metadata.project_root`

**How to avoid:**
```rust
fn to_relative_path(full_path: &str, project_root: &str) -> Result<String> {
    // Strip project_root prefix
    let relative = full_path.strip_prefix(project_root)
        .ok_or_else(|| anyhow!("Path not under project_root"))?;

    // Remove leading separator
    let relative = relative.strip_prefix('/').unwrap_or(relative);

    // Normalize: replace \ with / (Windows), remove . and ..
    let normalized = relative.replace('\\', "/");

    // Validate no empty components
    if normalized.contains("//") || normalized.contains("..") {
        return Err(anyhow!("Invalid relative path: {}", normalized));
    }

    Ok(normalized)
}
```

**Warning signs:** Path validation errors from SCIP consumers.

### Pitfall 4: Symbol Format Errors
**What goes wrong:** Symbol strings are rejected or don't resolve correctly.

**Why it happens:** SCIP symbol format has specific syntax with special characters (`/`, `#`, `.`, `(`, `)`, `[`, `]`, `:`, `!`). Spaces must be escaped with double space.

**How to avoid:** Use helper functions for symbol creation:
```rust
fn escape_symbol_name(name: &str) -> String {
    name.replace(' ', "  ")  // Double space for literal space
}

fn validate_symbol(symbol: &str) -> Result<()> {
    // Basic validation: must not be empty, must not start with "local "
    if symbol.is_empty() || symbol.starts_with("local ") {
        return Err(anyhow!("Invalid SCIP symbol: {}", symbol));
    }
    Ok(())
}
```

**Warning signs:** "Invalid symbol format" errors, symbols not resolving in consumers.

### Pitfall 5: Wrong Protobuf Serialization
**What goes wrong:** Output file cannot be parsed by SCIP consumers.

**Why it happens:** SCIP binary format is protobuf. Must use `prost::Message::encode()` correctly with proper buffer handling.

**How to avoid:**
```rust
// WRONG - using JSON
let json = serde_json::to_string(&index)?;

// RIGHT - use protobuf binary
let mut buffer = Vec::new();
index.encode(&mut buffer)?;
std::fs::write(output_path, buffer)?;
```

**Warning signs:** File size much larger than expected, consumers can't parse.

## Code Examples

### SCIP Export Main Function
```rust
// Source: src/graph/export/scip.rs
use anyhow::Result;
use scip::types as scip;
use prost::Message;

pub fn export_scip(graph: &mut CodeGraph, config: &ScipExportConfig) -> Result<Vec<u8>> {
    let mut index = scip::Index {
        metadata: Some(scip::Metadata {
            version: scip::ProtocolVersion::UnspecifiedProtocolVersion.into(),
            tool_info: Some(scip::ToolInfo {
                name: "magellan".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
                arguments: vec![],
            }),
            project_root: config.project_root.clone(),
            text_document_encoding: scip::TextEncoding::Utf8.into(),
        }),
        documents: vec![],
        external_symbols: vec![],
    };

    // Collect and group symbols by file
    let mut symbols_by_file: std::collections::HashMap<String, Vec<SymbolNode>> =
        std::collections::HashMap::new();

    let entity_ids = graph.files.backend.entity_ids()?;
    for entity_id in entity_ids {
        let entity = graph.files.backend.get_node(entity_id)?;
        if entity.kind == "Symbol" {
            if let Ok(symbol) = serde_json::from_value::<SymbolNode>(entity.data) {
                let file_path = get_file_path_from_symbol(graph, entity_id)?;
                symbols_by_file.entry(file_path).or_default().push(symbol);
            }
        }
    }

    // Convert each file to a Document
    let mut documents: Vec<scip::Document> = vec![];
    for (file_path, symbols) in symbols_by_file {
        let document = to_scip_document(graph, &file_path, symbols)?;
        documents.push(document);
    }

    // Sort deterministically
    documents.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
    index.documents = documents;

    // Serialize to protobuf binary
    let mut buffer = Vec::new();
    index.encode(&mut buffer)?;
    Ok(buffer)
}
```

### Symbol Kind Mapping
```rust
// Source: src/graph/export/scip.rs
use scip::types::SymbolInformation;

/// Map Magellan symbol kind to SCIP Kind
fn to_scip_kind(magellan_kind: &str) -> i32 {
    match magellan_kind {
        "Function" | "function" => SymbolInformation::Kind::Function as i32,
        "Method" | "method" => SymbolInformation::Kind::Method as i32,
        "Struct" | "struct" => SymbolInformation::Kind::Struct as i32,
        "Enum" | "enum" => SymbolInformation::Kind::Enum as i32,
        "Class" | "class" => SymbolInformation::Kind::Class as i32,
        "Trait" | "trait" => SymbolInformation::Kind::Trait as i32,
        "Module" | "mod" => SymbolInformation::Kind::Module as i32,
        "Variable" | "variable" => SymbolInformation::Kind::Variable as i32,
        "Constant" | "constant" => SymbolInformation::Kind::Constant as i32,
        "Field" | "field" => SymbolInformation::Kind::Field as i32,
        "Interface" | "interface" => SymbolInformation::Kind::Interface as i32,
        "Type" | "type" => SymbolInformation::Kind::Type as i32,
        _ => SymbolInformation::Kind::UnspecifiedKind as i32,
    }
}
```

### Language Detection
```rust
// Source: src/graph/export/scip.rs
use scip::types::Language;

fn detect_language_from_path(path: &str) -> String {
    let ext = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");

    match ext {
        "rs" => Language::Rust.as_str_name(),
        "py" => Language::Python.as_str_name(),
        "js" => Language::JavaScript.as_str_name(),
        "ts" => Language::TypeScript.as_str_name(),
        "tsx" => Language::TypeScriptReact.as_str_name(),
        "jsx" => Language::JavaScriptReact.as_str_name(),
        "go" => Language::Go.as_str_name(),
        "java" => Language::Java.as_str_name(),
        "kt" => Language::Kotlin.as_str_name(),
        "cpp" | "cc" | "cxx" | "hpp" | "hxx" => Language::Cpp.as_str_name(),
        "c" | "h" => Language::C.as_str_name(),
        "rb" => Language::Ruby.as_str_name(),
        "sh" | "bash" => Language::ShellScript.as_str_name(),
        _ => "UnspecifiedLanguage",
    }.to_string()
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| LSIF for code indexing | SCIP (Source Code Intelligence Protocol) | 2022 (Sourcegraph announcement) | SCIP is 8x smaller, 3x faster; LSIF deprecated |
| Custom symbol formats | Standardized SCIP symbol format | Ongoing | Interoperability between tools |
| JSON-only index formats | Binary protobuf for SCIP | Ongoing | More efficient encoding and parsing |

**SCIP vs LSIF (decided in CONTEXT.md):**
- SCIP is the recommended format
- LSIF is deprecated by Sourcegraph
- SCIP provides better performance and smaller file sizes

**For Magellan:**
- v1: SCIP export only
- v2: Consider LSIF if demand exists (currently deferred per CONTEXT.md)

## Open Questions

1. **SCIP symbol format complexity:** The full SCIP symbol format with descriptors is complex. For v1, should we use a simplified format?
   - **What we know:** SCIP symbol format supports packages, versions, multiple descriptor types.
   - **What's unclear:** How much of this complexity is needed for basic interoperability.
   - **Recommendation:** Use simplified format `magellan . . . symbol_name` for v1. Can extend to full format with module paths in v2.

2. **External symbols handling:** Should we include `external_symbols` in the Index for referenced but not defined symbols?
   - **What we know:** SCIP Index has optional `external_symbols` field for symbols defined in other packages.
   - **What's unclear:** Whether this is needed for single-repo indexing.
   - **Recommendation:** Leave `external_symbols` empty for v1. Only populate if multi-repo/cross-package references are needed.

3. **Document.text inclusion:** Should we include source text in Document.text field?
   - **What we know:** Document.text is optional but recommended for hover documentation.
   - **What's unclear:** Performance impact and memory usage for large files.
   - **Recommendation:** Leave Document.text empty for v1. Consumers can read from filesystem using `project_root` + `relative_path`.

## Sources

### Primary (HIGH confidence)
- [SCIP Protocol Buffer Definition](https://raw.githubusercontent.com/sourcegraph/scip/main/scip.proto) - Full SCIP specification, verified 2026-01-19
- [scip crate v0.6.1](https://crates.io/crates/scip) - Official Rust SCIP bindings, prost-generated types
- [SCIP announcement blog](https://sourcegraph.com/blog/announcing-scip) - Motivation and design philosophy
- [tokio-rs/prost](https://github.com/tokio-rs/prost) - Protobuf implementation used by scip crate
- [Existing codebase: src/graph/export.rs](https://github.com/feanor/magellan) - Verified export pattern (lines 1-1057)
- [Existing codebase: src/graph/schema.rs](https://github.com/feanor/magellan) - SymbolNode, ReferenceNode definitions

### Secondary (MEDIUM confidence)
- [SCIP TypeScript vs LSIF comparison](https://sourcegraph.com/blog/announcing-scip-typescript) - Performance benchmarks (8x smaller, 3x faster)
- [sourcegraph/scip-rust](https://github.com/sourcegraph/scip-rust) - Reference SCIP implementation for Rust

### Tertiary (LOW confidence)
- WebSearch results for SCIP rust crate usage patterns - Verified against official crate docs

## Metadata

**Confidence breakdown:**
- SCIP specification: HIGH - Directly reviewed scip.proto from official source
- scip crate usage: HIGH - Official crate with prost types verified
- Position encoding mapping: HIGH - Magellan uses byte offsets, matches SCIP UTF8 encoding
- Symbol format: MEDIUM - Format is documented but complex; simplified format proposed
- LSIF deferral: HIGH - Sourcegraph officially deprecated LSIF in favor of SCIP

**Research date:** 2026-01-19
**Valid until:** 2026-02-18 (SCIP protocol is stable; scip crate version 0.6.1 current as of Jan 2025)
