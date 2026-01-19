# Phase 13: SCIP Tests + Documentation - Research

**Researched:** 2026-01-20
**Domain:** SCIP (Source Code Intelligence Protocol) format validation, Rust protobuf testing, CLI security documentation
**Confidence:** HIGH

## Summary

Phase 13 requires two deliverables: (1) SCIP format validation via round-trip tests, and (2) user-facing security documentation. The SCIP export is currently a stub in `src/graph/export/scip.rs` that returns empty bytes. The scip crate v0.6.1 provides protobuf-based types and parsing utilities. SCIP format is a binary protobuf protocol defined by Sourcegraph for language-agnostic code indexing. Round-trip testing (export -> parse -> verify) is the standard pattern for validating protobuf outputs. For documentation, Magellan already has comprehensive path security code in `src/validation.rs` but lacks user-facing guidance on `.db` file placement.

**Primary recommendations:**
1. Implement SCIP export using `scip::types` module, round-trip test with `protobuf::Message::parse_from_bytes`
2. Document `.db` placement in README.md and MANUAL.md with clear security rationale

## Standard Stack

### Core

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `scip` | 0.6.1 (already in Cargo.toml) | SCIP protobuf types and utilities | Official Sourcegraph bindings for SCIP format |
| `protobuf` | 3.7 (already in Cargo.toml) | Protobuf serialization/parsing | Required by scip crate for message I/O |
| `tempfile` | 3.10 (dev dependency) | Test file/directory creation | Standard Rust testing pattern for file I/O |

### Supporting (for testing only)

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `scip::types` | 0.6.1 | Access to `Index`, `Document`, `Occurrence` message types | Building SCIP protobuf structures |
| `scip::symbol` | 0.6.1 | Symbol formatting/parsing utilities | Creating SCIP symbol strings from Magellan's FQN |
| `protobuf::Message` | 3.7 | `write_to_writer()`, `parse_from_bytes()` | Round-trip test pattern |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| Round-trip testing | Manual SCIP inspection with external tools | Round-trip is automated and catches format errors early; external tools require manual steps and may miss subtle encoding issues |

**Installation:**
```bash
# Dependencies already present in Cargo.toml
scip = "0.6.1"
protobuf = "3.7"
tempfile = "3.10"  # dev-dependencies
```

## Architecture Patterns

### SCIP Export Module Structure

The `src/graph/export/scip.rs` module should follow this pattern:

```rust
//! SCIP export functionality
//!
//! Implements SCIP (Source Code Intelligence Protocol) export for Magellan.
//! SCIP is a language-agnostic protocol for code indexing defined by Sourcegraph.

use anyhow::Result;
use scip::types::{Index, Metadata, Document, Occurrence, SymbolInformation, ToolInfo};
use scip::symbol;
use protobuf::Message;

use super::CodeGraph;

/// SCIP export configuration
#[derive(Debug, Clone)]
pub struct ScipExportConfig {
    pub project_root: String,
    pub project_name: Option<String>,
    pub version: Option<String>,
}

/// Export graph to SCIP format
pub fn export_scip(graph: &CodeGraph, config: &ScipExportConfig) -> Result<Vec<u8>> {
    // 1. Build Index metadata
    // 2. Iterate over graph documents -> SCIP Documents
    // 3. Serialize to protobuf bytes
    let mut index = Index::new();
    // ... populate index ...
    Ok(index.write_to_bytes()?)
}
```

### Round-Trip Test Pattern (Standard for Protobuf Validation)

```rust
#[test]
fn test_scip_roundtrip() {
    // Arrange: Create a graph with known symbols
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    {
        let mut graph = CodeGraph::open(&db_path).unwrap();
        graph.index_file("/test/main.rs", b"fn main() {}").unwrap();
    }

    // Act: Export to SCIP
    let scip_bytes = {
        let mut graph = CodeGraph::open(&db_path).unwrap();
        export_scip(&graph, &ScipExportConfig::default()).unwrap()
    };

    // Assert: Parse SCIP bytes and verify structure
    let parsed_index = Index::parse_from_bytes(&scip_bytes)
        .expect("SCIP export should be parseable");

    // Verify metadata
    assert!(!parsed_index.metadata.is_unknown());
    assert!(!parsed_index.documents.is_empty());

    // Verify document count matches expectation
    // Verify symbol counts
}
```

### Test File Organization

Create `tests/scip_export_tests.rs` following existing pattern from `tests/cli_export_tests.rs`:

```rust
//! SCIP export round-trip tests
//!
//! Verifies SCIP export format correctness by exporting then parsing.

use std::fs;
use tempfile::TempDir;

#[test]
fn test_scip_roundtrip_basic() { /* ... */ }

#[test]
fn test_scip_parseable_by_scip_crate() { /* ... */ }

#[test]
fn test_scip_metadata_correct() { /* ... */ }

#[test]
fn test_scip_symbol_encoding() { /* ... */ }
```

### Documentation Update Pattern

Add new sections to existing documentation files:

**README.md** - Add "Security" section after "Commands":

```markdown
## Security

### Database File Placement

Magellan's database (`--db <FILE>`) stores all indexed code information. For security:

- **Place `.db` outside watched directories** to prevent it from being included in export operations
- The watcher follows symlinks and resolves paths; ensure `.db` is not symlinked into a watched directory
- Recommended: Use a dedicated cache directory like `~/.cache/magellan/` or `/var/cache/magellan/`

Example:
```bash
# Recommended: database outside project
magellan watch --root /path/to/project --db ~/.cache/magellan/project.db --scan-initial

# Avoid: database inside watched directory (may be processed as part of index)
magellan watch --root . --db ./magellan.db  # Not recommended
```
```

**MANUAL.md** - Add section 8 "Security Best Practices":

```markdown
## 8. Security Best Practices

### 8.1 Database Placement

[expanded guidance with examples]

### 8.2 Path Traversal Protection

Magellan validates all paths to prevent directory traversal attacks:
- [Reference existing validation.rs implementation]
```

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| SCIP protobuf types | Custom struct definitions | `scip::types::*` from scip crate | SCIP is a standardized protocol; custom types won't be compatible with SCIP consumers like Sourcegraph |
| Protobuf serialization | Manual byte encoding | `Message::write_to_bytes()` | Protobuf has complex encoding rules; manual implementation will have bugs |
| SCIP symbol formatting | Custom string concatenation | `scip::symbol::format_symbol()` | SCIP symbol syntax has specific escaping rules (spaces, special chars) |
| Test temp directories | Hardcoded `/tmp` paths | `tempfile::TempDir` | Cross-platform, automatic cleanup, isolated test environments |
| Protobuf parsing | Manual byte deserialization | `Message::parse_from_bytes()` | Handles wire format, varint encoding, nested messages |

**Key insight:** SCIP is a standardized protocol with existing tooling. Hand-rolling SCIP export will create incompatible output. Use the official bindings and verify via round-trip testing.

## Common Pitfalls

### Pitfall 1: SCIP Symbol Format Incorrectness

**What goes wrong:** SCIP symbols have specific syntax (`scheme package/descriptor1/descriptor2.`) with escaping rules for spaces and special characters.

**Why it happens:** The symbol syntax appears simple (just strings with slashes), but has edge cases like:
- Spaces must be escaped as double spaces
- Must end with `.` for global symbols
- `local ` prefix for local symbols

**How to avoid:** Use `scip::symbol::format_symbol()` or `scip::symbol::format_symbol_with()` from the scip crate instead of manual string manipulation.

**Warning signs:** SCIP indexes fail to load in Sourcegraph or show "symbol not found" errors for definitions that exist in the index.

### Pitfall 2: Incorrect Position Encoding

**What goes wrong:** Source ranges use wrong character offsets (UTF-16 vs UTF-8 bytes), causing incorrect hover/goto-definition positions.

**Why it happens:** SCIP supports multiple position encodings (UTF8CodeUnitOffset, UTF16CodeUnitOffset, UTF32CodeUnitOffset). Magellan uses UTF-8 byte offsets internally, but SCIP defaults to unspecified.

**How to avoid:** Set `Document.position_encoding = scip::types::PositionEncoding::Utf8CodeUnitOffsetFromLineStart` for each SCIP Document.

**Warning signs:** Sourcegraph shows code hovers at wrong column positions.

### Pitfall 3: Missing SCIP Metadata

**What goes wrong:** SCIP index is parseable but missing required metadata fields, making it unusable by consumers.

**Why it happens:** The Index::new() constructor creates an empty message. Forgetting to populate `metadata` means the index has no project root, tool info, or protocol version.

**How to avoid:** Always populate `Index.metadata` before serialization:
```rust
let mut metadata = Metadata::new();
metadata.set_tool_info(tool_info);
metadata.set_project_root(config.project_root.clone());
index.set_metadata(metadata);
```

**Warning signs:** SCIP CLI rejects the index with "missing metadata" errors.

### Pitfall 4: Database in Watched Directory

**What goes wrong:** Placing `magellan.db` inside the watched directory causes it to be processed as if it's a source file, potentially:
- Causing the watcher to try to parse the database as code
- Including database contents in export output
- Creating circular file system events

**Why it happens:** Users follow convenience and place `.db` next to source code for easy access, not realizing the watcher treats all files in the watched tree equally.

**How to avoid:** Document and recommend external database placement. Could add a warning in `watch_cmd.rs` if `--db` path is within `--root` path.

**Warning signs:** Watcher logs show processing of `.db` file, or export output includes binary database content.

## Code Examples

### SCIP Index Construction

```rust
// Source: https://docs.rs/scip/0.6.1/scip/types/index.html

use scip::types::{Index, Metadata, ToolInfo, Document, Occurrence};
use scip::symbol::{format_symbol, SymbolDescriptor};
use protobuf::Message;

fn build_scip_index() -> Index {
    let mut index = Index::new();

    // Set metadata (required)
    let mut metadata = Metadata::new();
    let mut tool_info = ToolInfo::new();
    tool_info.set_name("magellan".to_string());
    tool_info.set_version(env!("CARGO_PKG_VERSION").to_string());
    metadata.set_tool_info(tool_info);
    metadata.set_project_root("/path/to/project".to_string());
    index.set_metadata(metadata);

    // Add documents
    let mut document = Document::new();
    document.set_relative_path("src/main.rs".to_string());
    document.set_language("rust".to_string());
    document.set_position_encoding(scip::types::PositionEncoding::Utf8CodeUnitOffsetFromLineStart);

    // Add occurrences
    let mut occurrence = Occurrence::new();
    occurrence.set_range(vec![0, 0, 0, 10]);  // [line, col, line, col]
    occurrence.set_symbol("rust lang/magellan/main.".to_string());
    occurrence.set_symbol_roles(scip::types::SymbolRole::Definition as i32);
    document.mut_occurrences().push(occurrence);

    index.mut_documents().push(document);
    index
}
```

### Round-Trip Verification Pattern

```rust
// Source: Protobuf round-trip testing pattern from Apache Arrow DataFusion
// https://github.com/apache/arrow-datafusion/issues/7600

#[test]
fn test_scip_export_roundtrip() {
    use scip::types::Index;
    use protobuf::Message;

    // Create and populate graph
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let file_path = temp_dir.path().join("test.rs");

    fs::write(&file_path, r#"
fn main() {
    println!("Hello, world!");
}

fn helper() -> i32 {
    42
}
"#).unwrap();

    {
        let mut graph = CodeGraph::open(&db_path).unwrap();
        let source = fs::read(&file_path).unwrap();
        let path_str = file_path.to_string_lossy().to_string();
        graph.index_file(&path_str, &source).unwrap();
    }

    // Export to SCIP
    let scip_bytes = {
        let mut graph = CodeGraph::open(&db_path).unwrap();
        export_scip(&graph, &ScipExportConfig::default()).unwrap()
    };

    // Verify: Parse SCIP bytes
    let parsed_index = Index::parse_from_bytes(&scip_bytes)
        .expect("SCIP export should be parseable protobuf");

    // Verify structure
    assert!(parsed_index.has_metadata(), "Index must have metadata");
    assert!(!parsed_index.documents().is_empty(), "Index must have documents");

    // Verify specific expectations
    let main_doc = parsed_index.documents().iter()
        .find(|d| d.relative_path().ends_with("test.rs"))
        .expect("Should find document for test.rs");

    assert_eq!(main_doc.language(), "rust");
    assert!(!main_doc.occurrences().is_empty(), "Should have occurrences");
}
```

### SCIP Symbol Formatting from Magellan FQN

```rust
// Source: https://docs.rs/scip/0.6.1/scip/symbol/index.html

use scip::symbol::{format_symbol, SymbolDescriptor, Descriptor, Suffix};

fn magellan_to_scip_symbol(fqn: &str, kind: &str) -> String {
    // Parse Magellan's FQN format (e.g., "mycrate::module::function")
    let parts: Vec<&str> = fqn.split("::").collect();

    // Build SCIP descriptors
    let mut descriptors = Vec::new();

    // Package/namespace descriptor
    if parts.len() > 1 {
        let mut pkg_desc = Descriptor::new();
        pkg_desc.set_name(parts[0].to_string());
        pkg_desc.set_suffix(Suffix::Namespace);
        descriptors.push(pkg_desc);
    }

    // Type/method descriptor
    if let Some(&name) = parts.last() {
        let mut desc = Descriptor::new();
        desc.set_name(name.to_string());

        // Map Magellan kinds to SCIP suffixes
        desc.set_suffix(match kind {
            "Function" => Suffix::Method,
            "Method" => Suffix::Method,
            "Struct" => Suffix::Type,
            "Enum" => Suffix::Type,
            "Module" => Suffix::Namespace,
            _ => Suffix::Namespace,
        });

        descriptors.push(desc);
    }

    // Format as SCIP symbol
    // Syntax: scheme package/descriptor1/descriptor2.
    format_symbol("rust", "magellan", &descriptors)
        .expect("Symbol formatting should succeed")
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| LSIF (JSON-based indexing) | SCIP (protobuf-based indexing) | 2022 (Sourcegraph announcement) | SCIP is 50% smaller payload, more efficient parsing; LSIF is deprecated |
| Manual SCIP format verification | Round-trip automated testing | 2024+ (Apache Arrow pattern) | Catches format errors early in CI/CD; manual inspection is error-prone |

**Deprecated/outdated:**
- LSIF (Language Server Index Format): Replaced by SCIP; Sourcegraph deprecated LSIF in 2023
- Manual SCIP validation with external CLI: Valid for smoke testing but insufficient for automated regression testing

## Open Questions

1. **SCIP Symbol Scheme Format**
   - What we know: SCIP symbols use `scheme/package/descriptor.` format
   - What's unclear: Exact scheme string for Magellan (possibly `rust magellan/` or generic `magellan/`)
   - Recommendation: Use `magellan` as scheme, project name as package component; verify with Sourcegraph if compatibility needed

2. **Multi-language SCIP Metadata**
   - What we know: SCIP allows mixing languages in a single Index
   - What's unclear: Should Magellan emit one Index per language or one combined Index
   - Recommendation: Start with single combined Index; can optimize later if needed

3. **SCIP Occurrence Range Format**
   - What we know: Ranges are `[startLine, startChar, endLine, endChar]` or `[startLine, startChar, endChar]` (3-element)
   - What's unclear: Whether to use 3 or 4 element ranges
   - Recommendation: Use 3-element for single-line, 4-element for multi-line spans; matches Magellan's existing span model

## Sources

### Primary (HIGH confidence)

- [scip crate docs.rs](https://docs.rs/scip/0.6.1/scip/) - Official Rust bindings API documentation
- [scip::types module](https://docs.rs/scip/0.6.1/scip/types/index.html) - Protobuf message types (Index, Document, Occurrence, etc.)
- [scip::symbol module](https://docs.rs/scip/0.6.1/scip/symbol/index.html) - Symbol formatting utilities
- [SCIP Protocol Definition (scip.proto)](https://raw.githubusercontent.com/sourcegraph/scip/main/scip.proto) - Official protobuf schema
- [sourcegraph/scip GitHub repository](https://github.com/sourcegraph/scip) - Official SCIP specification and tools

### Secondary (MEDIUM confidence)

- [Apache Arrow DataFusion protobuf round-trip tests](https://github.com/apache/arrow-datafusion/issues/7600) - Pattern for organizing protobuf round-trip tests
- [Serialize and Deserialize Protobuf in Rust](https://ssojet.com/serialize-and-deserialize/serialize-and-deserialize-protobuf-in-rust) - Rust protobuf I/O patterns
- [SQLite Security Best Practices](https://sqlite.org/security.html) - Official SQLite security documentation ("Defense Against The Dark Arts")

### Tertiary (LOW confidence)

- [SQLite Tutorial: Best Practices 2025](https://dev.to/chat2db/sqlite-tutorial-installation-usage-and-best-practices-54h1) - General SQLite usage patterns (not specific to CLI tools)

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - All libraries verified via official docs and crates.io
- Architecture: HIGH - SCIP format and scip crate API verified from official Sourcegraph sources
- Pitfalls: HIGH - Based on verified SCIP specification and common protobuf errors
- Documentation recommendations: MEDIUM - Based on general security best practices; no SCIP-specific security guidance exists (SCIP is data format, not a security boundary)

**Research date:** 2026-01-20
**Valid until:** 2026-06-01 (SCIP format is stable, but verify scip crate for updates before implementation)
