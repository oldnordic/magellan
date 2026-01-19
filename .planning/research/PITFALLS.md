# Pitfalls Research

**Domain:** Deterministic codebase mapping / code graph CLI tools (tree-sitter-based)
**Researched:** 2026-01-18 (Updated 2026-01-19 for v1.1 FQN + Path Validation + Transaction Safety)
**Confidence:** MEDIUM-HIGH

This document focuses on *domain-specific* failure modes when retrofitting an existing codebase-mapping tool with:
- structured JSON outputs and explicit schemas
- stable identifiers (execution_id, match_id, span_id)
- span-aware reporting (byte offsets + line/col)
- validation hooks (pre/post verification, checksums)
- execution logging / audit trails
- **v1.1 focus:** fully-qualified names (FQN), path validation security, SQLite transaction safety

> Constraints assumed: Magellan is deterministic, synchronous, CLI-first, local-only, no LSP/network/config files.

---

## Critical Pitfalls

### Pitfall 1: "Stable IDs" that aren't actually stable

**What goes wrong:**
Downstream systems (tests, caches, refactor tools) rely on IDs that silently change across runs:
- DB internal entity IDs are reused/compacted
- per-run incrementing counters shift when ordering changes
- parser-provided IDs are only unique within one parse tree

This breaks baselining, "same finding across runs", and makes "execution logging" useless for correlating outcomes.

**Why it happens:**
It's tempting to reuse what's already there (sqlite rowid / graph entity id / tree-sitter Node::id) instead of designing a first-class identity scheme.

Tree-sitter's `Node::id()` is explicitly *unique within a tree*, but not a durable identifier across arbitrary runs; reuse behavior depends on incremental parsing and does not guarantee stability. (See tree-sitter Rust bindings docs.)

**How to avoid:**
Define IDs by *content-addressing + deterministic context*, not by runtime allocation.

Recommended approach:
- **execution_id:** deterministic hash of (tool version + command + args + normalized root + normalized db path + input file set manifest hashes). Do **not** include wall-clock time.
- **span_id:** hash of (normalized file identity + byte_start + byte_end + kind + "span role" (definition/reference/callsite) + optional symbol key).
- **symbol_id (public):** separate from DB id. Use a stable "symbol key" derived from (language + kind + fully_qualified_name + defining file + defining span). If full qualification isn't available, be explicit that collisions are possible.
- Record the *derivation recipe* in schema docs (so future versions can keep compatibility).

**Warning signs:**
- IDs differ when rerunning the same command twice with no file changes.
- IDs differ depending on filesystem traversal order.
- IDs are integers that monotonically increase per run.
- IDs are taken from tree-sitter Node::id or sqlitegraph entity IDs.

**Phase to address:**
Phase 1–2 (Output schema + deterministic ordering + ID scheme). Must happen before "execution logging", because logs need stable correlation.

**Test cases to prevent regression:**
```rust
#[test]
fn test_symbol_id_stable_across_runs() {
    // Index same file twice → symbol_id must be identical
    let id1 = index_and_get_symbol_id("test.rs", SOURCE);
    let id2 = index_and_get_symbol_id("test.rs", SOURCE);
    assert_eq!(id1, id2);
}

#[test]
fn test_symbol_id_changes_on_signature_change() {
    // Symbol with different signature/span → different ID
    let id1 = index_and_get_symbol_id("test.rs", SOURCE_A);
    let id2 = index_and_get_symbol_id("test.rs", SOURCE_B); // different signature
    assert_ne!(id1, id2);
}
```

---

### Pitfall 2: Span reporting that mixes incompatible coordinate systems

**What goes wrong:**
Consumers can't locate spans reliably because the tool mixes:
- byte offsets vs character offsets
- UTF-8 bytes vs UTF-16 code units
- 0-based vs 1-based line/column
- inclusive vs exclusive end offsets

This leads to "off by one" highlights, broken patch application, and incorrect "impact analysis".

**Why it happens:**
Span math is deceptively tricky in multi-language, multi-encoding codebases. Tool authors often store byte offsets (from parser) but present line/col as human-friendly without specifying the base.

Tree-sitter positions (`Point`) are **0-based** (row/column), while many editor conventions are 1-based. (See `tree_sitter::Point` docs.)

**How to avoid:**
Adopt a single canonical span model and translate *explicitly* at API boundaries:

- Canonical internal span: `{ byte_start, byte_end }` where `byte_end` is **exclusive**.
- For UI: include both:
  - `point_start: { line_1based, column_1based }`
  - `point_end: { line_1based, column_1based }`
- Specify encoding assumptions in schema:
  - byte offsets are UTF-8 byte offsets into the original file bytes
  - line/col derived from the same bytes

Add invariants:
- `0 <= byte_start <= byte_end <= file_byte_len`
- `point_start <= point_end`
- converting (byte->point->byte) is consistent for ASCII-only test fixtures

**Warning signs:**
- Different commands report the same symbol with different line/col.
- Reported columns drift on files containing non-ASCII characters.
- End positions sometimes point to the next token/line inconsistently.

**Phase to address:**
Phase 2 (Span model + correctness tests, including non-ASCII fixtures).

**Test cases to prevent regression:**
```rust
#[test]
fn test_span_roundtrip_ascii() {
    let source = b"fn test() {}";
    let span = Span { start: 0, end: 11 };
    let point = span_to_point(source, &span);
    let recovered = point_to_span(source, &point);
    assert_eq!(span, recovered);
}

#[test]
fn test_span_roundtrip_non_ascii() {
    let source = "fn ???() {}".as_bytes(); // emoji in function name
    let span = Span { start: 3, end: 6 }; // span of emoji
    let point = span_to_point(source, &span);
    let recovered = point_to_span(source, &point);
    assert_eq!(span, recovered);
}
```

---

### Pitfall 3: Crashing or corrupting output on non-ASCII source

**What goes wrong:**
Tools panic or emit invalid spans because they slice a UTF-8 `String` with byte offsets (not guaranteed to land on char boundaries). This is a known sharp edge in Rust: `str` slicing requires character boundaries.

Magellan already has a documented risk: "Potential panic when slicing Rust String with byte offsets" in `src/graph/ops.rs` (see `.planning/codebase/CONCERNS.md`).

**Why it happens:**
Tree-sitter yields byte offsets. It's easy to convert file bytes to `&str` for convenience, then accidentally use byte indices on it.

**How to avoid:**
- Treat source as `&[u8]` for all span slicing and hashing.
- Only convert to `&str` when needed, and then validate boundaries:
  - `source_str.is_char_boundary(byte_start)` and `.is_char_boundary(byte_end)`
- Prefer storing snippets as bytes or validated UTF-8; include `encoding` metadata if storing raw bytes.

**Warning signs:**
- Panics on repos with emoji, CJK identifiers, or string literals in other languages.
- "Snippet" fields in JSON sometimes contain replacement characters (U+FFFD).

**Phase to address:**
Phase 2 (Span correctness + chunk/snippet storage hardening).

**Test cases to prevent regression:**
```rust
#[test]
fn test_non_ascii_symbol_extraction() {
    let source = "fn ????????() {}".as_bytes(); // Arabic function name
    let symbols = parse_symbols("test.rs", source);
    assert!(!symbols.is_empty());
    // Should not panic when extracting snippet
}

#[test]
fn test_char_boundary_validation() {
    let source = "fn ???() {}".as_bytes();
    let byte_start = 3; // middle of emoji sequence (may not be char boundary)
    assert!(!source.is_char_boundary(byte_start));
    // Extraction should handle this gracefully
}
```

---

### Pitfall 4: "Deterministic output" that still isn't reproducible

**What goes wrong:**
Even with sorted arrays, outputs still change between runs due to hidden nondeterminism:
- hash maps serialized with unstable key iteration
- OS filesystem order leaking into results
- inclusion of timestamps, durations, PIDs in primary output
- absolute paths that differ per machine

SARIF explicitly calls out nondeterministic elements and provides guidance for producing deterministic logs (Appendix F in SARIF v2.1.0). Even if Magellan doesn't emit SARIF, the *failure modes are the same.*

**Why it happens:**
Teams focus on "sort the main list" but forget nested lists/maps and metadata fields.

**How to avoid:**
Make determinism a first-class acceptance criterion:

- Always sort collections (files, symbols, refs, edges) by a stable composite key:
  `normalized_path, span.byte_start, span.byte_end, kind, name`.
- Ban wall-clock in JSON outputs unless behind `--include-timing`.
- Define path normalization rules:
  - prefer workspace-relative paths in output
  - if absolute is necessary, include both and document it
- Use canonical JSON serialization for tests (e.g., stable key ordering during serialization).

**Warning signs:**
- JSON diffs show reordering only.
- Reruns differ only in "metadata" fields.
- CI fails on Windows/macOS but passes on Linux due to path/line-ending differences.

**Phase to address:**
Phase 1 (Deterministic ordering + schema rules + test harness).

**Test cases to prevent regression:**
```rust
#[test]
fn test_json_output_deterministic() {
    let output1 = run_index_and_capture_json("test_repo");
    let output2 = run_index_and_capture_json("test_repo");
    assert_eq!(output1, output2, "Outputs must be byte-identical");
}

#[test]
fn test_json_output_independent_of_traversal_order() {
    let repo = create_test_repo();
    let output1 = index_repo(&repo);
    shuffle_repo_files(&repo); // Change filesystem order
    let output2 = index_repo(&repo);
    assert_eq!(output1, output2);
}
```

---

### Pitfall 5: Output schema that is "JSON-shaped" but not a contract

**What goes wrong:**
Tools ship JSON output without:
- versioning
- explicit field semantics
- backward compatibility strategy
- machine-checked schema

Downstream integrations become fragile; every CLI change is a breaking change.

**Why it happens:**
"Just emit JSON" feels sufficient for scripts, until multiple consumers exist (CI gates, dashboards, refactor automation).

**How to avoid:**
- Define **schema version** and include it in every JSON document (top-level `schema_version`).
- Publish JSON Schema (or equivalent) and validate outputs in tests.
- Establish compatibility rules:
  - additive fields allowed
  - removing/renaming fields requires major version bump
  - enums must be forward-compatible (unknown values tolerated)

**Warning signs:**
- Consumers use `jq '.foo.bar[0].baz'` with no fallback.
- Different commands return "similar but different" shapes.
- Errors are printed as plain text mixed into stdout.

**Phase to address:**
Phase 1 (Schema-first JSON output across commands).

**Test cases to prevent regression:**
```rust
#[test]
fn test_json_conforms_to_schema() {
    let output = run_index_and_capture_json("test_repo");
    let schema = load_json_schema("schema/index_output_v1.json");
    assert!(schema.validate(&output).is_ok());
}

#[test]
fn test_schema_version_present() {
    let output: Value = serde_json::from_str(&run_index("test_repo"));
    assert!(output.get("schema_version").is_some());
}
```

---

### Pitfall 6: Logging that breaks machine output (stdout contamination)

**What goes wrong:**
The tool prints "INFO: ..." or progress output to stdout, interleaving with JSON. This makes the tool unusable in pipelines.

**Why it happens:**
CLI tools start as human-facing; adding JSON output later often forgets the "stdout is data" rule.

**How to avoid:**
- **stdout**: machine output only (JSON / NDJSON). No banners.
- **stderr**: human logs, warnings, progress.
- Provide `--quiet`, `--verbose`, and `--log-format` (e.g., text vs JSON logs) but keep stdout semantics strict.

**Warning signs:**
- `magellan ... | jq` fails intermittently.
- Users report "invalid JSON" errors when `--progress` is enabled.

**Phase to address:**
Phase 1 (Output discipline) + Phase 4 (execution logging format).

**Test cases to prevent regression:**
```bash
#!/bin/bash
# Integration test: stdout must be valid JSON
output=$(magellan index /tmp/test_repo 2>/dev/null)
echo "$output" | jq . >/dev/null
exit_code=$?
if [ $exit_code -ne 0 ]; then
    echo "FAIL: stdout is not valid JSON"
    exit 1
fi
```

---

### Pitfall 7: "Validation hooks" that don't validate what matters

**What goes wrong:**
Validation exists, but it checks the wrong thing:
- only checks that DB file exists
- only checks schema migration
- does not check that indexed facts correspond to current file contents
- does not detect partial writes / interrupted runs

**Why it happens:**
Validation is bolted on as a separate command, not integrated into the lifecycle of commands that mutate state.

**How to avoid:**
Implement validation as **pre/post invariants** around every operation that mutates the graph:

- Pre:
  - DB is writable
  - workspace root exists
  - target file set is stable and deterministic (sorted)
- Post:
  - DB invariants hold (no dangling edges; counts match expected)
  - per-file content hash stored in DB matches filesystem if "fresh"
  - graph export re-loadable (round-trip minimal check)

Also adopt a consistent failure contract:
- exit codes for validation failures
- machine-readable validation errors in JSON

**Warning signs:**
- `verify` says OK but downstream queries return missing symbols.
- interrupted scan leaves DB in inconsistent state.

**Phase to address:**
Phase 3 (Validation hooks + invariants + exit codes).

**Test cases to prevent regression:**
```rust
#[test]
fn test_detects_dangling_references() {
    let mut graph = create_test_graph();
    // Create a reference pointing to non-existent symbol
    insert_orphan_reference(&mut graph);
    let result = validate_invariants(&graph);
    assert!(result.is_err());
    assert!(matches!(result, Err(InvariantError::DanglingEdge(_))));
}

#[test]
fn test_detects_hash_mismatch() {
    let mut graph = create_test_graph();
    // Modify file without updating hash
    tamper_file(&graph);
    let result = verify_freshness(&graph);
    assert!(result.is_err());
}
```

---

### Pitfall 8: Event storms and redundant indexing (watcher determinism collapse)

**What goes wrong:**
File watchers generate multiple events per save (temp file write, rename, chmod, etc.). If you process them naively, you:
- re-index the same file N times
- fall behind (unbounded queue)
- produce nondeterministic intermediate states

Magellan already notes that `WatcherConfig.debounce_ms` exists but is effectively unused, and that the watcher uses an unbounded channel (see `.planning/codebase/CONCERNS.md`).

**Why it happens:**
Filesystem notification semantics vary across OSes/editors, and are rarely "one event per meaningful change."

**How to avoid:**
- Implement explicit debouncing/coalescing per path within `debounce_ms`.
- Collapse sequences into a single canonical action per path: Delete dominates Modify, etc.
- Bound the queue or coalesce in-memory map keyed by path.
- Ensure the *ordering rule* is explicit (e.g., lexicographic path order per batch).

**Warning signs:**
- CPU spikes while typing/saving.
- Database writes far exceed actual changes.
- Watch mode produces different results depending on edit cadence.

**Phase to address:**
Phase 0–1 for watcher correctness (before expanding logging + stable IDs).

**Test cases to prevent regression:**
```rust
#[test]
fn test_debouncing_single_save() {
    let (tx, rx) = create_watcher();
    // Simulate editor save: create temp, write, rename, chmod
    send_events(&tx, &[
        FileEvent::Create("main.rs~"),
        FileEvent::Modify("main.rs~"),
        FileEvent::Rename("main.rs~", "main.rs"),
        FileEvent::Chmod("main.rs"),
    ]);
    let batch = wait_for_batch(rx);
    assert_eq!(batch.paths.len(), 1, "Should coalesce to single event");
    assert_eq!(batch.paths[0], PathBuf::from("main.rs"));
}
```

---

### Pitfall 9: Path identity bugs (relative vs absolute, lossy conversion)

**What goes wrong:**
The same file appears as different identities across commands/runs because:
- some commands store absolute paths, others store relative
- paths are converted with `to_string_lossy()` (non-UTF8 becomes "U+FFFD")
- symlinks/case-insensitive filesystems cause aliasing

Magellan currently uses `to_string_lossy()` widely (documented in `.planning/codebase/CONCERNS.md`).

**Why it happens:**
Rust's `PathBuf` isn't directly JSON-serializable; converting to string seems easy.

**How to avoid:**
- Define a *canonical path normalization contract*:
  - store workspace-relative, normalized separators in output
  - store absolute path optionally for debugging
- Decide policy for non-UTF8 paths:
  - either reject early with a clear error
  - or store raw bytes (base64) alongside a lossy display string
- Include `workspace_root` in outputs so relative paths are resolvable.

**Warning signs:**
- Queries return "file not found" for files that exist.
- `verify` reports a file as missing, but it's present under a different path form.

**Phase to address:**
Phase 1 (Output schema + normalization). This is foundational for stable IDs.

**Test cases to prevent regression:**
```rust
#[test]
fn test_path_normalization_relative_absolute() {
    let root = PathBuf::from("/project");
    let path1 = root.join("src/main.rs");
    let path2 = PathBuf::from("/project/src/main.rs");
    assert_eq!(normalize_path(&root, &path1), normalize_path(&root, &path2));
}

#[test]
fn test_symlink_canonicalization() {
    let root = PathBuf::from("/project");
    let symlink = root.join("link.rs");
    let target = root.join("target.rs");
    create_symlink(&symlink, &target).unwrap();
    let normalized = normalize_path(&root, &symlink);
    // Should resolve to target, not symlink path
    assert_eq!(normalized, "target.rs");
}
```

---

### Pitfall 10: Name-based cross-file resolution produces "confidently wrong" graphs

**What goes wrong:**
The tool links references/calls to the wrong symbol when names collide across files/modules. The output looks plausible but is semantically wrong.

Magellan already documents collision behavior: references keep the "first" symbol for a name; calls use a different policy (prefers current file) (see `.planning/codebase/CONCERNS.md`).

**Why it happens:**
Without semantic resolution (types/imports), teams fall back to matching by name.

**How to avoid (within Magellan's constraints):**
- Be explicit that cross-file resolution is heuristic unless you have a stronger key.
- Improve determinism and reduce "wrong edges" by:
  - including container/module path in symbol key where possible
  - preferring same-file definitions (consistent across references and calls)
  - emitting "unresolved" edges separately rather than forcing a match
  - recording *candidates* when ambiguous (list of possible symbol_ids)

**Warning signs:**
- Impact analysis shows surprising callers for a function name common in repo.
- Refactor tooling renames the wrong target.

**Phase to address:**
Phase 2–3 (stable identity + resolution policy + validation warnings for ambiguity).

**Test cases to prevent regression:**
```rust
#[test]
fn test_name_collision_disambiguation() {
    // Create two files with same-named function
    let source_a = "fn foo() {}";
    let source_b = "fn foo() {}";
    index_file("a.rs", source_a);
    index_file("b.rs", source_b);

    // Reference from a third file should be marked ambiguous
    let refs = resolve_references("c.rs", "fn main() { foo(); }");
    assert!(refs.iter().any(|r| r.is_ambiguous));
}

#[test]
fn test_same_file_reference_priority() {
    index_file("a.rs", "fn foo() {}");
    index_file("b.rs", "fn foo() {} fn bar() { foo(); }");

    // Reference in b.rs should prefer local foo
    let callers = get_callers_of("b.rs", "foo");
    assert_eq!(callers.len(), 1);
    assert_eq!(callers[0].file, "b.rs");
}
```

---

## v1.1 Critical Pitfalls (FQN + Path Validation + Transaction Safety)

### Pitfall 11: Incomplete or inconsistent FQN construction

**What goes wrong:**
When implementing fully-qualified names (FQN), common mistakes include:
- Using simple names instead of hierarchical names (current Magellan state)
- Inconsistent separator usage (`::` vs `.` vs `/`)
- Missing container context (module/class/namespace)
- Language-specific edge cases not handled (anonymous namespaces, closures, traits)
- Encoding issues in FQN strings (non-ASCII identifiers)

These issues cause:
- **symbol_id collisions**: Different symbols get same hash
- **Cross-file mislinks**: References resolve to wrong definition
- **Instability**: Refactoring code changes FQN unexpectedly

**Why it happens:**
FQN construction requires AST traversal and language-specific knowledge. The naive approach uses only the symbol's immediate name, ignoring its container hierarchy.

Current Magellan code (`src/ingest/mod.rs:188`):
```rust
let fqn = name.clone(); // For v1, FQN is just the symbol name
```

This is documented as a limitation in `.planning/codebase/CONCERNS.md`.

**How to avoid:**

1. **Define FQN format specification per language:**
   ```rust
   // Rust: crate::module::struct::method
   // Python: package.module.Class.method
   // JavaScript: module.Class#method (instance) or module.Class.method (static)
   // Java: org.package.Class.method
   // C++: namespace::Class::method
   ```

2. **Implement hierarchical AST traversal:**
   - Walk up the tree from symbol node to root
   - Collect container names (mod/impl/trait/class/namespace)
   - Use language-specific rules for separator selection

3. **Handle special cases explicitly:**
   - Anonymous closures: use parent name + unique suffix (line/col)
   - Trait impls: include trait name in FQN
   - Re-exports: preserve original FQN
   - Generic parameters: include in FQN or handle separately

4. **Validate FQN uniqueness:**
   - Assert no two symbols with different spans have same FQN
   - If collision detected, use more specific context or encode span

**Warning signs:**
- symbol_id hash collisions occur in test repos
- Reference resolution picks wrong definition when multiple exist
- FQN format differs between language parsers
- Non-ASCII identifiers cause FQN encoding issues

**Phase to address:**
Phase v1.1 (Correctness milestone) - FQN is foundational for symbol_id stability.

**Test cases to prevent regression:**
```rust
#[test]
fn test_fqn_uniqueness_nested_symbols() {
    let source = r#"
        mod outer {
            mod inner {
                fn foo() {}
            }
            fn foo() {}
        }
    "#;
    let symbols = parse_symbols("test.rs", source.as_bytes());
    let fqns: Vec<_> = symbols.iter().filter_map(|s| s.fqn.as_ref()).collect();
    assert_eq!(fqns.len(), 2, "Should have 2 'foo' functions");
    assert_ne!(fqns[0], fqns[1], "FQNs must differ");
    assert!(fqns[0].contains("outer"), "FQN should include module");
}

#[test]
fn test_fqn_trait_impl() {
    let source = r#"
        trait Trait {
            fn method(&self);
        }
        struct Struct;
        impl Trait for Struct {
            fn method(&self) {}
        }
    "#;
    let symbols = parse_symbols("test.rs", source.as_bytes());
    let method_fqn = symbols.iter()
        .find(|s| s.name.as_deref() == Some("method"))
        .and_then(|s| s.fqn.as_ref());
    assert!(method_fqn.is_some());
    // FQN should indicate it's Trait's method implemented for Struct
    assert!(method_fqn.unwrap().contains("Trait"), "FQN should mention trait");
}

#[test]
fn test_fqn_non_ascii_identifier() {
    let source = "fn ????????() {}";
    let symbols = parse_symbols("test.rs", source.as_bytes());
    let fqn = symbols[0].fqn.as_ref().unwrap();
    assert!(fqn.is_ascii() || fqn.contains("?????"));
    // FQN should handle non-ASCII without corruption
}

#[test]
fn test_symbol_id_stability_with_fqn() {
    let source1 = "mod a { mod b { fn foo() {} } }";
    let source2 = "mod a { mod b { fn foo() {} } }";
    let id1 = compute_symbol_id("test.rs", source1.as_bytes());
    let id2 = compute_symbol_id("test.rs", source2.as_bytes());
    assert_eq!(id1, id2, "Same code → same symbol_id");
}
```

---

### Pitfall 12: Path traversal vulnerabilities

**What goes wrong:**
Without proper path validation, malicious input can access files outside the intended workspace:
- `../../etc/passwd` in file paths
- Symlinks pointing outside workspace
- UNC paths on Windows (`\\server\share`)
- Device files on Unix (`/dev/urandom`)

Recent CVEs demonstrate this is an active threat:
- **CVE-2025-68705**: RustFS path traversal vulnerability (2025)
- **CVE-2025-11233**: Cygwin path validation bypass (2025)

**Why it happens:**
Path handling libraries (Rust's `std::path`) don't validate security boundaries. `strip_prefix()` and string operations can be bypassed.

Current Magellan code (`src/graph/filter.rs:239-243`) uses `strip_prefix()` but may not fully validate escape attempts.

**How to avoid:**

1. **Always validate paths are within workspace root:**
   ```rust
   pub fn validate_path_within_root(path: &Path, root: &Path) -> Result<Path> {
       let canonical = path.canonicalize()?;
       let canonical_root = root.canonicalize()?;
       if !canonical.starts_with(&canonical_root) {
           return Err(anyhow!("Path {} escapes workspace root", path.display()));
       }
       Ok(canonical)
   }
   ```

2. **Use dedicated validation crates:**
   - Consider `strict-path` crate (2025) for hardened path handling
   - Or implement similar validation: reject `..` components, validate absolute paths

3. **Handle symlinks explicitly:**
   ```rust
   // Option A: Reject symlinks entirely
   if path.is_symlink() {
       return Err(anyhow!("Symlinks not allowed"));
   }
   // Option B: Resolve and validate target
   let target = path.read_link()?;
   let resolved = path.parent().unwrap().join(target);
   validate_path_within_root(&resolved, root)?;
   ```

4. **Cross-platform considerations:**
   - Windows: Reject UNC paths, handle drive letters
   - Unix: Reject device file access, handle `~` expansion safely
   - Test on both platforms

**Warning signs:**
- Tool accepts paths with `..` components
- Can read files outside declared workspace root
- Symlinks allow escaping root boundary
- No explicit path validation before file operations

**Phase to address:**
Phase v1.1 (Safety milestone) - path validation is security-critical.

**Test cases to prevent regression:**
```rust
#[test]
fn test_reject_path_traversal() {
    let root = PathBuf::from("/workspace");
    let malicious = PathBuf::from("/workspace/../../etc/passwd");
    assert!(validate_path_within_root(&malicious, &root).is_err());
}

#[test]
fn test_reject_symlink_escape() {
    let root = tempfile::tempdir().unwrap();
    let link = root.path().join("escape");
    let target = PathBuf::from("/etc/passwd");
    symlink(&target, &link).unwrap();
    assert!(validate_path_within_root(&link, root.path()).is_err());
}

#[test]
fn test_reject_unc_path_windows() {
    #[cfg(windows)]
    {
        let root = PathBuf::from("C:\\workspace");
        let unc = PathBuf::from("\\\\server\\share");
        assert!(validate_path_within_root(&unc, &root).is_err());
    }
}

#[test]
fn test_accept_valid_path() {
    let root = PathBuf::from("/workspace");
    let valid = PathBuf::from("/workspace/src/main.rs");
    assert!(validate_path_within_root(&valid, &root).is_ok());
}
```

---

### Pitfall 13: Cross-platform path normalization bugs

**What goes wrong:**
Path handling differs between platforms, causing:
- **Windows**: Backslash vs forward slash confusion
- **Case sensitivity**: macOS case-insensitive but case-preserving, Linux case-sensitive
- **Drive letters**: Windows paths include `C:\`, others don't
- **Unicode normalization**: Different byte representations for same visual path

These issues break determinism across platforms and cause "file not found" errors.

**Why it happens:**
Developing on one platform and not testing on others. Rust's `PathBuf` abstracts some differences but not all.

**How to avoid:**

1. **Use camino for UTF-8 path normalization:**
   ```rust
   use camino::Utf8PathBuf;
   // Always use UTF-8 paths internally
   let normalized = Utf8PathBuf::from(path)
       .canonicalize_utf8()
       .map_err(|e| anyhow!("Path not UTF-8: {}", e))?;
   ```

2. **Define canonical representation:**
   - Always store paths with forward slashes
   - Always relative to workspace root
   - Document case sensitivity policy (case-sensitive for determinism)

3. **Platform-specific handling:**
   ```rust
   pub fn normalize_path_separators(path: &str) -> String {
       // Always use forward slashes (cross-platform compatible)
       path.replace('\\', "/")
   }
   ```

4. **Case sensitivity for determinism:**
   - On case-insensitive filesystems, warn about case collisions
   - Use exact case for determinism (don't lowercase)

**Warning signs:**
- Tests pass on Linux but fail on Windows/macOS
- Same file shows up with different path representations
- Path comparisons fail unexpectedly

**Phase to address:**
Phase v1.1 (Correctness) - cross-platform determinism is a core promise.

**Test cases to prevent regression:**
```rust
#[test]
fn test_path_separator_normalization() {
    let windows_path = "src\\main.rs";
    let normalized = normalize_path(windows_path);
    assert_eq!(normalized, "src/main.rs");
}

#[test]
fn test_case_sensitivity_collision_detection() {
    #[cfg(target_os = "macos")]
    {
        // On macOS, create files that differ only in case
        let dir = tempfile::tempdir().unwrap();
        let path1 = dir.path().join("file.rs");
        let path2 = dir.path().join("FILE.rs");
        write(&path1, "content1").unwrap();
        // This should warn or error (depends on policy)
        let result = detect_case_collision(&dir);
        assert!(result.has_collision());
    }
}
```

---

### Pitfall 14: SQLite transaction misuse - partial updates on error

**What goes wrong:**
When database operations fail partway through, the system leaves:
- Orphaned records (symbols without edges)
- Half-completed file deletions
- Inconsistent state between `graph_nodes` and `graph_data` tables

SQLite documentation explicitly states: transactions created with BEGIN don't nest, and errors may or may not cause automatic rollback depending on the error type.

**Why it happens:**
Current code in `src/graph/ops.rs` performs multiple operations without explicit transaction wrapping:
1. `delete_file_symbols` - separate operation
2. `insert_symbol_node` - no transaction
3. `insert_defines_edge` - no transaction
4. `index_references` - separate operation

If step 3 fails, step 2's data remains but step 4 never runs.

**How to avoid:**

1. **Wrap multi-step operations in explicit transactions:**
   ```rust
   pub fn index_file(graph: &mut CodeGraph, path: &str, source: &[u8]) -> Result<usize> {
       let conn = graph.files.backend.graph().db_connection()?;
       let tx = conn.unchecked_transaction()?;

       // All operations here...
       let result = (|| {
           // Step 1: Delete existing
           graph.symbols.delete_file_symbols(file_id)?;
           // Step 2: Insert symbols
           for fact in &symbol_facts {
               let symbol_id = graph.symbols.insert_symbol_node(fact)?;
               graph.symbols.insert_defines_edge(file_id, symbol_id)?;
           }
           // Step 3: Index references
           query::index_references(graph, path, source)?;
           Ok(symbol_facts.len())
       })();

       match result {
           Ok(count) => {
               tx.commit()?;
               Ok(count)
           }
           Err(e) => {
               tx.rollback()?;
               Err(e)
           }
       }
   }
   ```

2. **Handle SQLite-specific error behaviors:**
   - SQLITE_FULL, SQLITE_IOERR, SQLITE_BUSY, SQLITE_NOMEM may cause partial rollback
   - Explicitly ROLLBACK on error (SQLite docs recommend this)
   - Check autocommit status after errors

3. **Use appropriate transaction type:**
   - DEFERRED (default): start transaction on first read/write
   - IMMEDIATE: acquire write lock immediately (use for writes)
   - EXCLUSIVE: prevent other readers (use for schema changes)

4. **WAL mode considerations:**
   - WAL allows concurrent readers
   - Long-running reads block checkpointing
   - Explicit checkpoints may be needed for large transactions

**Warning signs:**
- Orphan detection tests fail after error injection
- DB contains symbols without file edges
- `sqlite3 magellan.db "SELECT COUNT(*) FROM graph_nodes"` doesn't match expected counts

**Phase to address:**
Phase v1.1 (Safety) - transactional integrity is critical for correctness.

**Test cases to prevent regression:**
```rust
#[test]
fn test_transaction_rollback_on_error() {
    let mut graph = create_test_graph();
    let initial_symbols = count_symbols(&graph);

    // Inject failure during indexing
    let result = index_file_with_injected_failure(&mut graph, "test.rs", SOURCE, Step::InsertEdge);

    assert!(result.is_err());
    // All symbols should be rolled back
    let after_symbols = count_symbols(&graph);
    assert_eq!(initial_symbols, after_symbols, "No new symbols on failure");
}

#[test]
fn test_no_orphaned_edges_after_error() {
    let mut graph = create_test_graph();
    index_file(&mut graph, "a.rs", SOURCE_A).unwrap();

    // Fail while indexing second file
    let _ = index_file_with_injected_failure(&mut graph, "b.rs", SOURCE_B, Step::Halfway);

    // Should have no edges pointing to non-existent symbols
    let orphans = detect_orphan_edges(&graph);
    assert_eq!(orphans, 0, "No orphaned edges after rollback");
}

#[test]
fn test_delete_file_facts_is_atomic() {
    let mut graph = create_test_graph();
    index_file(&mut graph, "test.rs", SOURCE).unwrap();
    let file_id = graph.files.find_file_node("test.rs").unwrap().unwrap();

    // Inject failure mid-deletion
    let _ = delete_file_with_failure(&mut graph, "test.rs", FailurePoint::MidDelete);

    // Either fully deleted or fully present (not half-state)
    let still_exists = graph.files.find_file_node("test.rs").unwrap().is_some();
    let has_symbols = graph.symbols.count_symbols_in_file("test.rs") > 0;
    assert_eq!(still_exists, has_symbols, "File and symbols must be consistent");
}
```

---

### Pitfall 15: SQLite connection and locking issues

**What goes wrong:**
Multiple connections to the same database can cause:
- `SQLITE_BUSY` errors when one connection holds a write lock
- Deadlocks in WAL mode with long-running reads
- "database is locked" errors in concurrent scenarios

SQLite WAL mode documentation notes that while WAL improves concurrency, certain scenarios still cause SQLITE_BUSY.

**Why it happens:**
Current code opens separate connections:
- `sqlitegraph` has its own connection
- `ChunkStore` opens its own connection (`src/generation/mod.rs:34`)
- These connections may conflict

**How to avoid:**

1. **Use a single shared connection where possible:**
   ```rust
   pub struct CodeGraph {
       // Single connection for all operations
       conn: Arc<Mutex<rusqlite::Connection>>,
       // ...
   }
   ```

2. **Configure busy timeout:**
   ```rust
   let conn = Connection::open(&db_path)?;
   conn.busy_timeout(Duration::from_secs(30))?;
   ```

3. **Use appropriate locking mode:**
   - NORMAL mode for WAL (default, allows concurrent readers)
   - IMMEDIATE transactions for writes
   - Avoid EXCLUSIVE unless necessary

4. **Handle SQLITE_BUSY gracefully:**
   ```rust
   fn with_retry<T, F>(mut f: F) -> Result<T>
   where
       F: FnMut() -> Result<T>,
   {
       for attempt in 0..3 {
           match f() {
               Ok(v) => return Ok(v),
               Err(rusqlite::Error::SqliteFailure(err, _))
                   if err.code == ErrorCode::DatabaseBusy =>
               {
                   if attempt < 2 {
                       std::thread::sleep(Duration::from_millis(100));
                       continue;
                   }
               }
               Err(e) => return Err(e.into()),
           }
       }
       unreachable!()
   }
   ```

**Warning signs:**
- Intermittent "database is locked" errors
- Tests fail when run in parallel
- Watch mode crashes under high load

**Phase to address:**
Phase v1.1 (Safety) - connection management affects reliability.

**Test cases to prevent regression:**
```rust
#[test]
fn test_concurrent_reads() {
    let graph = Arc::new(create_test_graph());
    let handles: Vec<_> = (0..10).map(|_| {
        let graph = graph.clone();
        thread::spawn(move || {
            // All threads should be able to read simultaneously
            query_symbols(&graph, "*")
        })
    }).collect();
    for handle in handles {
        handle.join().unwrap();
    }
}

#[test]
fn test_write_during_read() {
    let graph = Arc::new(Mutex::new(create_test_graph()));
    let reader = thread::spawn({
        let graph = graph.clone();
        move || {
            let g = graph.lock().unwrap();
            // Start a long read transaction
            let conn = g.files.backend.graph().db_connection().unwrap();
            let tx = conn.unchecked_transaction().unwrap();
            thread::sleep(Duration::from_millis(100));
            // Query should still work
            let count: i64 = tx.query_row("SELECT COUNT(*) FROM graph_nodes", [], |r| r.get(0)).unwrap();
            count
        }
    });

    thread::sleep(Duration::from_millis(50));
    let writer = thread::spawn({
        let graph = graph.clone();
        move || {
            let mut g = graph.lock().unwrap();
            // This should wait for read to finish (or busy-wait with timeout)
            index_file(&mut g, "test.rs", b"fn test() {}")
        }
    });

    let read_count = reader.join().unwrap();
    let write_result = writer.join().unwrap();
    assert!(write_result.is_ok(), "Write should succeed after read");
}
```

---

### Pitfall 16: DDL statements in transactions (schema migration pitfalls)

**What goes wrong:**
Performing schema changes (DDL) within transactions can cause:
- Implicit commits in some SQLite configurations
- Database locks preventing migration
- Partial migrations on error

SQLite has specific behaviors around DDL in transactions that vary by version and configuration.

**Why it happens:**
Schema changes are often added without proper migration planning. Each schema change should be versioned and applied atomically.

**How to avoid:**

1. **Version your schema explicitly:**
   ```sql
   CREATE TABLE IF NOT EXISTS schema_version (
       version INTEGER PRIMARY KEY,
       applied_at INTEGER NOT NULL
   );
   INSERT OR IGNORE INTO schema_version VALUES (1, strftime('%s', 'now'));
   ```

2. **Use separate transactions for DDL:**
   ```rust
   pub fn migrate(conn: &Connection) -> Result<()> {
       // DDL in its own transaction
       conn.execute_batch("
           BEGIN IMMEDIATE;
           CREATE TABLE IF NOT EXISTS ...
           CREATE INDEX IF NOT EXISTS ...
           COMMIT;
       ")?;
       Ok(())
   }
   ```

3. **Test migration rollback:**
   - Migrate up, verify, migrate down, verify
   - Test on empty and populated databases

**Warning signs:**
- Schema changes require manual DB deletion
- Tests fail after code update due to schema mismatch
- Migration scripts fail partway through

**Phase to address:**
Phase v1.1 (Safety) - proper schema migration is essential.

---

## Technical Debt Patterns

Shortcuts that seem reasonable but create long-term problems.

| Shortcut | Immediate Benefit | Long-term Cost | When Acceptable |
|----------|-------------------|----------------|-----------------|
| Use DB entity IDs as public IDs | Zero new design work | IDs churn; consumers break; can't baseline | Never for public JSON; OK for internal-only debugging |
| Emit "best effort" JSON with ad-hoc fields per command | Fast iteration | Fragmented API surface; hard to maintain | MVP only if schema_v0 is explicitly "unstable" |
| Include timestamps in every output | Easy debugging | Non-deterministic diffs; breaks caching | Only behind `--include-timing` |
| Force-match refs/calls by name | "Complete" graphs | Wrong edges become trusted | Only if also emitting `ambiguity: true` + candidates |
| Log to stdout | Simple | Breaks pipelines; breaks NDJSON | Never (stdout must be data) |
| Simple FQN = name only | Easy to implement | Symbol ID collisions; wrong cross-file links | Short-term prototyping only |
| Partial transactions | Simpler error handling | Orphaned data; inconsistent state | Never |
| Multiple DB connections | Avoids refactoring | Locking issues; SQLITE_BUSY | Single shared connection preferred |

---

## Integration Gotchas

Common mistakes when connecting *developer tooling* to other tools (CI, scripts, editors) even without network services.

| Integration | Common Mistake | Correct Approach |
|-------------|----------------|------------------|
| `jq` / shell pipelines | Mixing progress logs into stdout | stdout = JSON/NDJSON only; logs to stderr |
| CI baselining | Non-deterministic ordering breaks diffs | enforce stable ordering; provide `--stable` default |
| SARIF/other converters (optional) | Missing stable fingerprints | Provide stable IDs + span keys; fingerprints can be derived (see SARIF "fingerprint" concept) |
| "verify" in CI | Verifies wrong root/path form | Always emit workspace_root + normalized file paths |
| Cross-platform CI | Platform-specific path breaks | Use camino; normalize separators; test on Linux/Windows/macOS |

---

## Performance Traps

Patterns that work at small scale but fail as usage grows.

| Trap | Symptoms | Prevention | When It Breaks |
|------|----------|------------|----------------|
| O(N) DB scans per file event (for refs/calls) | Watch mode lags; high CPU | maintain symbol index table/cache; incremental updates | Medium repos (10k+ symbols) |
| Unbounded watcher queue | Memory growth; late processing | debounce/coalesce; bounded queue | High-churn repos or rapid-save editors |
| Emitting huge monolithic JSON | Memory spikes; slow parsing | support streaming NDJSON; paginate | Very large graphs/exports |
| Long-running read transactions | WAL grows unbounded | Keep reads short; explicit checkpoints | Write-heavy workloads |
| Multiple DB connections | SQLITE_BUSY spikes | Single connection with mutex | Concurrent access patterns |

---

## Security Mistakes

Domain-specific security issues for local code graph tools.

| Mistake | Risk | Prevention |
|---------|------|------------|
| Path traversal (escaped workspace root) | Arbitrary file read/write | Validate all paths; resolve symlinks; use canonical paths |
| Persisting large code snippets by default in shared environments | Sensitive data stored in DB artifacts | Provide flags to disable snippet/chunk storage; document DB as sensitive |
| Logging full file contents/snippets in execution logs | Data leakage via CI logs | Redact by default; log only spans/IDs unless explicitly requested |
| Indexing unbounded file sizes | DoS / disk bloat | enforce max file size; cap snippet lengths |
| No validation of input file paths | Malicious repository attacks | Reject paths with `..`; validate within root; check symlink targets |

**Recent CVEs (2025) demonstrating active threat:**
- [CVE-2025-68705: RustFS Path Traversal](https://www.sentinelone.com/vulnerability-database/cve-2025-68705)
- [CVE-2025-11233: Cygwin Path Validation Bypass](https://www.wiz.io/vulnerability-database/cve/cve-2025-11233)

---

## UX Pitfalls

Common user experience mistakes in deterministic mapping CLIs.

| Pitfall | User Impact | Better Approach |
|---------|-------------|-----------------|
| "JSON mode" still requires reading text docs to interpret | Hard to integrate | Embed schema_version + command metadata in every output |
| Ambiguous results presented as definitive | Users make wrong refactors | Mark ambiguity; provide candidates; expose resolution policy |
| Errors reported only as a single string | Hard to triage programmatically | Structured error objects with codes + locations |
| Cross-platform path differences | Confusion, failed automation | Document path format; use normalized separators; test on all platforms |
| Non-obvious FQN format | Misunderstanding symbol identity | Document FQN per language; include in reference docs |

---

## "Looks Done But Isn't" Checklist

- [ ] **Structured JSON output:** stdout contains *only* JSON/NDJSON (no progress/log lines) — verify by piping to `jq`.
- [ ] **Deterministic ordering:** re-run same command twice; output identical byte-for-byte — verify with `diff`.
- [ ] **Stable IDs:** IDs unchanged across reruns on unchanged repo — verify with a golden test.
- [ ] **Span fidelity:** byte offsets match extracted snippet boundaries; non-ASCII files don't panic — verify with fixtures.
- [ ] **Validation hooks:** a deliberately corrupted DB/file mismatch is detected and returns non-zero exit — verify with tests.
- [ ] **FQN correctness:** Fully-qualified names include container context; no collisions in test repos with nested symbols.
- [ ] **Path validation:** Paths with `..`, symlinks outside root, and UNC paths are rejected.
- [ ] **Transaction safety:** Error injection during multi-step DB operations leaves no partial state.
- [ ] **Cross-platform paths:** Same input produces same path string on Linux, macOS, and Windows.

---

## Recovery Strategies

| Pitfall | Recovery Cost | Recovery Steps |
|---------|---------------|----------------|
| Unstable IDs shipped | HIGH | version-bump schema; provide migration/compat layer; add deprecation period |
| Off-by-one spans shipped | HIGH | fix span model; add "span_version"; regenerate snapshots; add compatibility translation |
| stdout contamination shipped | MEDIUM | change logging targets; add `--quiet`; document strict stdout contract |
| Name collision mis-links | MEDIUM | add ambiguity reporting; tighten heuristics; add regression fixtures |
| Watcher event storms | MEDIUM | implement debouncing/coalescing; cap backlog; add metrics counters |
| Incomplete FQN shipped | HIGH | version-bump symbol_id algorithm; reindex all data; provide migration tool |
| Path traversal vulnerabilities | CRITICAL | immediate patch; add comprehensive path validation; security advisory |
| Transaction issues | HIGH | audit all multi-step operations; wrap in transactions; add rollback tests |

---

## Pitfall-to-Phase Mapping

Suggested roadmap phases referenced below are conceptual; rename to match your actual milestone plan.

| Pitfall | Prevention Phase | Verification |
|---------|------------------|--------------|
| Stable IDs aren't stable | Phase 1–2 | Golden tests: rerun → identical IDs |
| Mixed coordinate systems | Phase 2 | Span round-trip tests; 0/1-based documented |
| Non-ASCII panics | Phase 2 | Fixtures with multi-byte chars; fuzz-ish tests |
| Output not reproducible | Phase 1 | `diff` identical outputs; deterministic serializer |
| JSON without contract | Phase 1 | JSON Schema validation in CI |
| stdout contamination | Phase 1 | `magellan ... | jq` always succeeds |
| Validation checks wrong things | Phase 3 | Inject mismatch; expect structured error + non-zero |
| Watch event storms | Phase 0–1 | Stress test rapid saves; bounded memory |
| Path identity bugs | Phase 1 | Tests with relative/absolute and symlink scenarios |
| Name collision mis-links | Phase 2–3 | Fixtures with duplicate names; ambiguity flagged |
| **Incomplete FQN construction** | **Phase v1.1** | FQN uniqueness tests; nested symbol fixtures |
| **Path traversal vulnerabilities** | **Phase v1.1** | Malicious path rejection tests; CVE regression tests |
| **Cross-platform path bugs** | **Phase v1.1** | Multi-platform CI; separator normalization tests |
| **Transaction misuse** | **Phase v1.1** | Error injection rollback tests; orphan detection |
| **SQLite locking issues** | **Phase v1.1** | Concurrent access tests; busy timeout tests |

---

## Sources

### Primary (HIGH confidence)
- SQLite official documentation — Transaction behavior, WAL mode, locking:
  - https://www.sqlite.org/lang_transaction.html (transactions, DEFERRED/IMMEDIATE/EXCLUSIVE)
  - https://www.sqlite.org/wal.html (WAL mode, concurrency, checkpoints)
- Tree-sitter Rust bindings documentation:
  - `Node::id` uniqueness constraints and reuse notes. https://docs.rs/tree-sitter/latest/tree_sitter/struct.Node.html
  - `Point` row/column are zero-based. https://docs.rs/tree-sitter/latest/tree_sitter/struct.Point.html
- Magellan internal codebase audit:
  - `.planning/codebase/CONCERNS.md` (FQN issues, path traversal risk, transaction gaps)

### Secondary (MEDIUM confidence)
- Rust Security Best Practices 2025:
  - https://corgea.com/Learn/rust-security-best-practices-2025
- strict-path crate for hardened path validation:
  - https://crates.io/crates/strict-path
- Recent CVEs demonstrating path traversal threats:
  - [CVE-2025-68705: RustFS Path Traversal](https://www.sentinelone.com/vulnerability-database/cve-2025-68705)
  - [CVE-2025-11233: Cygwin Path Validation](https://www.wiz.io/vulnerability-database/cve/cve-2025-11233)
- camino crate for UTF-8 path normalization:
  - https://docs.rs/camino/latest/camino/

### Tertiary (LOW confidence)
- Academic research on FQN resolution challenges:
  - "A Chain of AI-based Solutions for Resolving FQNs" (arXiv 2023). https://arxiv.org/pdf/2306.11981
  - "FQN Inference in Partial Code" (ACM 2023). https://dl.acm.org/doi/full/10.1145/3617174
- Rust Path Traversal Guide:
  - https://www.stackhawk.com/blog/rust-path-traversal-guide-example-and-prevention/

---
*Pitfalls research for: deterministic codebase mapping tools (Magellan)*
*Researched: 2026-01-18, Updated: 2026-01-19 for v1.1 FQN + Path Validation + Transaction Safety*
