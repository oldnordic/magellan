# Schema Reference

This document describes the data schema used by Magellan, intended to be followed by other tools in the integrated toolset for consistent data representation.

## Table of Contents

1. [ID Formats](#id-formats)
2. [Symbol Types](#symbol-types)
3. [Symbol Properties](#symbol-properties)
4. [Reference Types](#reference-types)
5. [Call Graph Structure](#call-graph-structure)
6. [AST Node Schema](#ast-node-schema-v190)
7. [Canonical FQN vs Display FQN](#canonical-fqn-vs-display-fqn)
8. [Node Types](#node-types)
9. [Edge Types](#edge-types)
10. [Language Support](#language-support)

---

## ID Formats

### Symbol ID

Stable identifier for symbols across indexing runs.

**Format**: 32 hex characters (128 bits)

**Generation**:
```
symbol_id = BLAKE3(language + ":" + fqn + ":" + byte_start + ":" + byte_end)
```

**Components**:
- `language`: Programming language (e.g., "rust", "python")
- `fqn`: Fully-qualified name
- `byte_start`: Start byte offset
- `byte_end`: End byte offset

**Stability**:
- Stable across re-indexing when position unchanged
- Changes on: rename, move, file rename, signature change

**Example**: `28e17e99cb937643a1b2c3d4e5f67890`

### Span ID

Stable identifier for a location in source code.

**Format**: 16 hex characters (64 bits)

**Generation**:
```
span_id = SHA256(file_path + ":" + byte_start + ":" + byte_end)[0:16]
```

**Components**:
- `file_path`: Path to source file
- `byte_start`: Start byte offset
- `byte_end`: End byte offset

**Example**: `f8e9d0c1b2a3f4e5`

### Match ID

Identifier for query results (ephemeral, not persisted).

**Format**: Hexadecimal hash string

**Generation**:
```
match_id = hash(symbol_name + file_path + byte_start)
```

**Example**: `abc123def456`

### Reference Match ID

Identifier for reference query results (ephemeral).

**Format**: Hexadecimal string with "ref_" prefix

**Example**: `ref_abc123def456`

### Execution ID

Unique identifier for a tool execution.

**Format**: `{timestamp_hex}-{pid_hex}`

**Generation**:
```
execution_id = format("{:x}-{:x}", unix_timestamp, process_id)
```

**Example**: `67abc123d4567-123a`

---

## Symbol Types

### Symbol Kinds (Normalized)

Internal normalized kinds used for querying:

| Normalized | Display Names | Description |
|------------|---------------|-------------|
| `fn` | Function, function | Standalone function |
| `method` | Method | Function within a type/impl |
| `struct` | Struct, Class | Composite data type |
| `enum` | Enum | Enumeration type |
| `trait` | Trait, Interface | Type interface/contract |
| `module` | Module, namespace | Namespace/scope |
| `variable` | Variable, let | Local variable |
| `const` | Const, static | Compile-time constant |
| `type` | Type, TypeAlias | Type alias |
| `union` | Union | Union type |
| `impl` | Impl block | Implementation block |
| `unknown` | Unknown | Unrecognized kind |

### Language-Specific Mappings

#### Rust

| Tree-sitter Kind | Normalized |
|------------------|------------|
| `function_item` | fn |
| `method_definition` | method |
| `struct_item` | struct |
| `enum_item` | enum |
| `trait_item` | trait |
| `mod_item` | module |
| `const_item` | const |
| `static_item` | const |
| `type_item` | type |
| `union_item` | union |
| `impl_item` | impl |

#### Python

| Tree-sitter Kind | Normalized |
|------------------|------------|
| `function_definition` | fn |
| `class_definition` | struct |
| `decorated_definition` | (depends on inner) |

#### JavaScript/TypeScript

| Tree-sitter Kind | Normalized |
|------------------|------------|
| `function_declaration` | fn |
| `method_definition` | method |
| `class_declaration` | struct |
| `interface_declaration` | trait |
| `type_alias_declaration` | type |

#### Java

| Tree-sitter Kind | Normalized |
|------------------|------------|
| `method_declaration` | method |
| `class_declaration` | struct |
| `interface_declaration` | trait |
| `enum_declaration` | enum |

#### C/C++

| Tree-sitter Kind | Normalized |
|------------------|------------|
| `function_definition` | fn |
| `class_specifier` | struct |
| `struct_specifier` | struct |
| `enum_specifier` | enum |

---

## Symbol Properties

### SymbolNode Schema

Stored in graph database for each symbol:

```json
{
  "symbol_id": "a1b2c3d4e5f6g7h8",
  "fqn": "my_crate::my_module::my_function",
  "canonical_fqn": "my_crate::src/lib.rs::Function my_function",
  "display_fqn": "my_crate::my_module::my_function",
  "name": "my_function",
  "kind": "Function",
  "kind_normalized": "fn",
  "byte_start": 42,
  "byte_end": 100,
  "start_line": 3,
  "start_col": 4,
  "end_line": 7,
  "end_col": 8
}
```

### Field Descriptions

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `symbol_id` | string | No | Stable symbol ID (16 hex chars) |
| `fqn` | string | No | Fully-qualified name |
| `canonical_fqn` | string | No | Unambiguous identity with file path |
| `display_fqn` | string | No | Human-readable FQN without file path |
| `name` | string | No | Simple symbol name |
| `kind` | string | Yes | Symbol kind (display form) |
| `kind_normalized` | string | No | Normalized kind for querying |
| `byte_start` | number | Yes | Start byte offset |
| `byte_end` | number | Yes | End byte offset |
| `start_line` | number | Yes | Start line (1-indexed) |
| `start_col` | number | Yes | Start column (0-indexed) |
| `end_line` | number | Yes | End line (1-indexed) |
| `end_col` | number | Yes | End column (0-indexed) |

### Span Position Fields

All spans use consistent position representation:

- **byte_start**: UTF-8 byte offset from file start (inclusive)
- **byte_end**: UTF-8 byte offset from file start (exclusive)
- **start_line**: Line number where span starts (1-indexed)
- **start_col**: Byte offset within start_line (0-indexed)
- **end_line**: Line number where span ends (1-indexed)
- **end_col**: Byte offset within end_line (0-indexed, exclusive)

---

## Reference Types

### Reference Kinds

Categorization of references:

| Kind | Description |
|------|-------------|
| `call` | Function/method call |
| `read` | Variable read access |
| `write` | Variable write access |
| `type_ref` | Type reference in annotation |
| `import` | Module/import reference |
| `inheritance` | Class/interface inheritance |

### ReferenceNode Schema

Stored in graph database for each reference:

```json
{
  "file": "src/main.rs",
  "byte_start": 150,
  "byte_end": 160,
  "start_line": 10,
  "start_col": 8,
  "end_line": 10,
  "end_col": 18
}
```

### ReferenceFact Schema

Transient fact during indexing:

```json
{
  "file_path": "src/main.rs",
  "referenced_symbol": "println",
  "byte_start": 150,
  "byte_end": 160,
  "start_line": 10,
  "start_col": 8,
  "end_line": 10,
  "end_col": 18
}
```

---

## Call Graph Structure

### Call Fact

Represents a caller → callee relationship:

```json
{
  "file_path": "src/main.rs",
  "caller": "main",
  "callee": "println",
  "caller_symbol_id": "a1b2c3d4",
  "callee_symbol_id": "e5f6g7h8",
  "byte_start": 150,
  "byte_end": 160,
  "start_line": 10,
  "start_col": 8,
  "end_line": 10,
  "end_col": 18
}
```

### CallNode Schema

Stored in graph database:

```json
{
  "file": "src/main.rs",
  "caller": "main",
  "callee": "println",
  "caller_symbol_id": "a1b2c3d4",
  "callee_symbol_id": "e5f6g7h8",
  "byte_start": 150,
  "byte_end": 160,
  "start_line": 10,
  "start_col": 8,
  "end_line": 10,
  "end_col": 18
}
```

### Call Direction

| Direction | Description |
|-----------|-------------|
| `in` | Incoming calls (callers of the symbol) |
| `out` | Outgoing calls (callees from the symbol) |

---

## AST Node Schema (v1.9.0)

Abstract Syntax Tree node representation for hierarchical code structure analysis.

### AstNode Schema

Stored in database for each AST node:

```json
{
  "id": 123,
  "parent_id": 120,
  "kind": "if_expression",
  "byte_start": 1500,
  "byte_end": 1650
}
```

### Field Descriptions

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `id` | number | Yes | Internal node ID (auto-increment) |
| `parent_id` | number | No | Parent node ID (null for root nodes) |
| `kind` | string | Yes | Tree-sitter node kind |
| `byte_start` | number | Yes | Start byte offset |
| `byte_end` | number | Yes | End byte offset |

### AST Node Kinds

Common structural node kinds extracted:

| Kind | Description | Example |
|------|-------------|---------|
| `function_item` | Function definition | `fn foo() {}` |
| `struct_item` | Struct definition | `struct Foo {}` |
| `enum_item` | Enum definition | `enum Bar {}` |
| `impl_item` | Implementation block | `impl Foo {}` |
| `if_expression` | If statement | `if x { }` |
| `while_expression` | While loop | `while x { }` |
| `for_expression` | For loop | `for x in y { }` |
| `loop_expression` | Loop block | `loop { }` |
| `match_expression` | Match expression | `match x { }` |
| `block` | Code block | `{ statements }` |
| `call_expression` | Function call | `foo()` |
| `return_expression` | Return statement | `return x` |
| `let_declaration` | Variable declaration | `let x = 1` |
| `unsafe_block` | Unsafe block | `unsafe { }` |

### AST Query API

```bash
# Show AST tree for a file
magellan ast --db code.db --file src/main.rs

# Find AST nodes by kind
magellan find-ast --db code.db --kind if_expression

# Get node at specific position
magellan ast --db code.db --file src/main.rs --position 1234
```

---

## Canonical FQN vs Display FQN

### Canonical FQN (Unambiguous Identity)

Format: `{crate}::{file_path}::{kind} {symbol_name}`

Purpose: Provides unambiguous symbol identity including file path.

Examples:
- `my_crate::src/lib.rs::Function my_function`
- `my_crate::src/main.rs::Function main`
- `my_crate::src/types.rs::Struct MyStruct`

Used for:
- Symbol identity across files
- Collision detection
- Stable references

### Display FQN (Human-Readable)

Format: `{crate}::{module_chain}::{type}::{symbol_name}`

Purpose: Shortened form for user-facing output.

Examples:
- `my_crate::my_function`
- `my_crate::my_module::my_helper`
- `my_crate::MyStruct::my_method`

Used for:
- CLI output
- User interaction
- Logging

### FQN Builder API

```rust
FqnBuilder::new(crate_name, file_path, scope_separator)
    .canonical(&scope_stack, symbol_kind, symbol_name)
    .display(&scope_stack, symbol_kind, symbol_name)
```

### Scope Separator by Language

| Language | Separator |
|----------|-----------|
| Rust | `::` |
| Python | `.` |
| Java | `.` |
| JavaScript/TypeScript | `.` |
| C/C++ | `::` |

---

## Node Types

### Graph Node Kinds

Stored in sqlitegraph:

| Kind | Description | Properties |
|------|-------------|------------|
| `File` | Source file | path, hash, last_indexed_at |
| `Symbol` | Code symbol | SymbolNode data |
| `Reference` | Symbol reference | ReferenceNode data |
| `Call` | Function call | CallNode data |
| `CodeChunk` | Source code chunk | content, checksum |
| `AstNode` | AST node (v1.9.0) | kind, parent_id, byte_start, byte_end |

### FileNode Schema

```json
{
  "path": "src/main.rs",
  "hash": "sha256:abc123...",
  "last_indexed_at": 1706102400,
  "last_modified": 1706102350
}
```

Fields:
- `path`: File path (absolute or root-relative)
- `hash`: SHA-256 content hash
- `last_indexed_at`: Unix timestamp (seconds) of last indexing
- `last_modified`: Unix timestamp (seconds) of file mtime when indexed

---

## Edge Types

### Graph Edge Kinds

| Kind | From | To | Description |
|------|------|-------|-------------|
| `DEFINES` | File | Symbol | File defines a symbol |
| `REFERS` | Symbol | Reference | Symbol references another symbol |
| `CALLS` | Symbol | Call | Symbol makes a call |

### Edge Schema

```json
{
  "from": 123,
  "to": 456,
  "edge_type": "DEFINES",
  "data": {}
}
```

---

## Language Support

### Supported Languages

| Language | Extension | Status |
|----------|-----------|--------|
| Rust | `.rs` | Full support |
| Python | `.py` | Full support |
| JavaScript | `.js` | Full support |
| TypeScript | `.ts` | Full support |
| Java | `.java` | Full support |
| C | `.c`, `.h` | Full support |
| C++ | `.cpp`, `.hpp`, `.cc` | Full support |

### Language Detection

Based on file extension:
- `.rs` → Rust
- `.py` → Python
- `.js`, `.mjs`, `.cjs` → JavaScript
- `.ts` → TypeScript
- `.java` → Java
- `.c`, `.h` → C
- `.cpp`, `.hpp`, `.cc`, `.cxx` → C++

### Language Labels

Symbols are labeled with their language for efficient querying:

```bash
# Query by language
label --db db.db --label rust
```

---

## Conventions for Other Tools

When implementing data schemas for other tools:

1. **Use consistent ID formats**: 16 hex chars for stable IDs
2. **Follow span conventions**: Half-open ranges, 1-indexed lines, 0-indexed columns
3. **Include FQN variants**: Both canonical (with path) and display (readable)
4. **Normalize symbol kinds**: Use consistent kind_normalized values
5. **Label by language**: Enable language-based filtering
6. **Document schema version**: Track breaking changes
7. **Use SHA-256 for hashes**: Content checksums, ID generation

### Example: Symbol Schema for New Tool

```json
{
  "entity_id": "a1b2c3d4e5f6g7h8",
  "entity_type": "symbol",
  "language": "rust",
  "name": "my_function",
  "kind": "Function",
  "kind_normalized": "fn",
  "fqn": "my_crate::my_module::my_function",
  "canonical_fqn": "my_crate::src/lib.rs::Function my_function",
  "display_fqn": "my_crate::my_module::my_function",
  "location": {
    "file_path": "src/lib.rs",
    "byte_start": 42,
    "byte_end": 100,
    "start_line": 3,
    "start_col": 4,
    "end_line": 7,
    "end_col": 8
  }
}
```
