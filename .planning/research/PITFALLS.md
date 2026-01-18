# Pitfalls Research

**Domain:** Deterministic codebase mapping / code graph CLI tools (tree-sitter-based)
**Researched:** 2026-01-18
**Confidence:** MEDIUM

This document focuses on *domain-specific* failure modes when retrofitting an existing codebase-mapping tool with:
- structured JSON outputs and explicit schemas
- stable identifiers (execution_id, match_id, span_id)
- span-aware reporting (byte offsets + line/col)
- validation hooks (pre/post verification, checksums)
- execution logging / audit trails

> Constraints assumed: Magellan is deterministic, synchronous, CLI-first, local-only, no LSP/network/config files.

---

## Critical Pitfalls

### Pitfall 1: “Stable IDs” that aren’t actually stable

**What goes wrong:**
Downstream systems (tests, caches, refactor tools) rely on IDs that silently change across runs:
- DB internal entity IDs are reused/compacted
- per-run incrementing counters shift when ordering changes
- parser-provided IDs are only unique within one parse tree

This breaks baselining, “same finding across runs”, and makes “execution logging” useless for correlating outcomes.

**Why it happens:**
It’s tempting to reuse what’s already there (sqlite rowid / graph entity id / tree-sitter Node::id) instead of designing a first-class identity scheme.

Tree-sitter’s `Node::id()` is explicitly *unique within a tree*, but not a durable identifier across arbitrary runs; reuse behavior depends on incremental parsing and does not guarantee stability. (See tree-sitter Rust bindings docs.)

**How to avoid:**
Define IDs by *content-addressing + deterministic context*, not by runtime allocation.

Recommended approach:
- **execution_id:** deterministic hash of (tool version + command + args + normalized root + normalized db path + input file set manifest hashes). Do **not** include wall-clock time.
- **span_id:** hash of (normalized file identity + byte_start + byte_end + kind + “span role” (definition/reference/callsite) + optional symbol key).
- **symbol_id (public):** separate from DB id. Use a stable “symbol key” derived from (language + kind + name + container context + defining file + defining span). If full qualification isn’t available, be explicit that collisions are possible.
- Record the *derivation recipe* in schema docs (so future versions can keep compatibility).

**Warning signs:**
- IDs differ when rerunning the same command twice with no file changes.
- IDs differ depending on filesystem traversal order.
- IDs are integers that monotonically increase per run.
- IDs are taken from tree-sitter Node::id or sqlitegraph entity IDs.

**Phase to address:**
Phase 1–2 (Output schema + deterministic ordering + ID scheme). Must happen before “execution logging”, because logs need stable correlation.

---

### Pitfall 2: Span reporting that mixes incompatible coordinate systems

**What goes wrong:**
Consumers can’t locate spans reliably because the tool mixes:
- byte offsets vs character offsets
- UTF-8 bytes vs UTF-16 code units
- 0-based vs 1-based line/column
- inclusive vs exclusive end offsets

This leads to “off by one” highlights, broken patch application, and incorrect “impact analysis”.

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
- converting (byte→point→byte) is consistent for ASCII-only test fixtures

**Warning signs:**
- Different commands report the same symbol with different line/col.
- Reported columns drift on files containing non-ASCII characters.
- End positions sometimes point to the next token/line inconsistently.

**Phase to address:**
Phase 2 (Span model + correctness tests, including non-ASCII fixtures).

---

### Pitfall 3: Crashing or corrupting output on non-ASCII source

**What goes wrong:**
Tools panic or emit invalid spans because they slice a UTF-8 `String` with byte offsets (not guaranteed to land on char boundaries). This is a known sharp edge in Rust: `str` slicing requires character boundaries.

Magellan already has a documented risk: “Potential panic when slicing Rust String with byte offsets” in `src/graph/ops.rs` (see `.planning/codebase/CONCERNS.md`).

**Why it happens:**
Tree-sitter yields byte offsets. It’s easy to convert file bytes to `&str` for convenience, then accidentally use byte indices on it.

**How to avoid:**
- Treat source as `&[u8]` for all span slicing and hashing.
- Only convert to `&str` when needed, and then validate boundaries:
  - `source_str.is_char_boundary(byte_start)` and `.is_char_boundary(byte_end)`
- Prefer storing snippets as bytes or validated UTF-8; include `encoding` metadata if storing raw bytes.

**Warning signs:**
- Panics on repos with emoji, CJK identifiers, or string literals in other languages.
- “Snippet” fields in JSON sometimes contain replacement characters (�).

**Phase to address:**
Phase 2 (Span correctness + chunk/snippet storage hardening).

---

### Pitfall 4: “Deterministic output” that still isn’t reproducible

**What goes wrong:**
Even with sorted arrays, outputs still change between runs due to hidden nondeterminism:
- hash maps serialized with unstable key iteration
- OS filesystem order leaking into results
- inclusion of timestamps, durations, PIDs in primary output
- absolute paths that differ per machine

SARIF explicitly calls out nondeterministic elements and provides guidance for producing deterministic logs (Appendix F in SARIF v2.1.0). Even if Magellan doesn’t emit SARIF, the *failure modes are the same.*

**Why it happens:**
Teams focus on “sort the main list” but forget nested lists/maps and metadata fields.

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
- Reruns differ only in “metadata” fields.
- CI fails on Windows/macOS but passes on Linux due to path/line-ending differences.

**Phase to address:**
Phase 1 (Deterministic ordering + schema rules + test harness).

---

### Pitfall 5: Output schema that is “JSON-shaped” but not a contract

**What goes wrong:**
Tools ship JSON output without:
- versioning
- explicit field semantics
- backward compatibility strategy
- machine-checked schema

Downstream integrations become fragile; every CLI change is a breaking change.

**Why it happens:**
“Just emit JSON” feels sufficient for scripts, until multiple consumers exist (CI gates, dashboards, refactor automation).

**How to avoid:**
- Define **schema version** and include it in every JSON document (top-level `schema_version`).
- Publish JSON Schema (or equivalent) and validate outputs in tests.
- Establish compatibility rules:
  - additive fields allowed
  - removing/renaming fields requires major version bump
  - enums must be forward-compatible (unknown values tolerated)

**Warning signs:**
- Consumers use `jq '.foo.bar[0].baz'` with no fallback.
- Different commands return “similar but different” shapes.
- Errors are printed as plain text mixed into stdout.

**Phase to address:**
Phase 1 (Schema-first JSON output across commands).

---

### Pitfall 6: Logging that breaks machine output (stdout contamination)

**What goes wrong:**
The tool prints “INFO: …” or progress output to stdout, interleaving with JSON. This makes the tool unusable in pipelines.

**Why it happens:**
CLI tools start as human-facing; adding JSON output later often forgets the “stdout is data” rule.

**How to avoid:**
- **stdout**: machine output only (JSON / NDJSON). No banners.
- **stderr**: human logs, warnings, progress.
- Provide `--quiet`, `--verbose`, and `--log-format` (e.g., text vs JSON logs) but keep stdout semantics strict.

**Warning signs:**
- `magellan … | jq` fails intermittently.
- Users report “invalid JSON” errors when `--progress` is enabled.

**Phase to address:**
Phase 1 (Output discipline) + Phase 4 (execution logging format).

---

### Pitfall 7: “Validation hooks” that don’t validate what matters

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
  - per-file content hash stored in DB matches filesystem if “fresh”
  - graph export re-loadable (round-trip minimal check)

Also adopt a consistent failure contract:
- exit codes for validation failures
- machine-readable validation errors in JSON

**Warning signs:**
- `verify` says OK but downstream queries return missing symbols.
- interrupted scan leaves DB in inconsistent state.

**Phase to address:**
Phase 3 (Validation hooks + invariants + exit codes).

---

### Pitfall 8: Event storms and redundant indexing (watcher determinism collapse)

**What goes wrong:**
File watchers generate multiple events per save (temp file write, rename, chmod, etc.). If you process them naively, you:
- re-index the same file N times
- fall behind (unbounded queue)
- produce nondeterministic intermediate states

Magellan already notes that `WatcherConfig.debounce_ms` exists but is effectively unused, and that the watcher uses an unbounded channel (see `.planning/codebase/CONCERNS.md`).

**Why it happens:**
Filesystem notification semantics vary across OSes/editors, and are rarely “one event per meaningful change.”

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

---

### Pitfall 9: Path identity bugs (relative vs absolute, lossy conversion)

**What goes wrong:**
The same file appears as different identities across commands/runs because:
- some commands store absolute paths, others store relative
- paths are converted with `to_string_lossy()` (non-UTF8 becomes “�”)
- symlinks/case-insensitive filesystems cause aliasing

Magellan currently uses `to_string_lossy()` widely (documented in `.planning/codebase/CONCERNS.md`).

**Why it happens:**
Rust’s `PathBuf` isn’t directly JSON-serializable; converting to string seems easy.

**How to avoid:**
- Define a *canonical path normalization contract*:
  - store workspace-relative, normalized separators in output
  - store absolute path optionally for debugging
- Decide policy for non-UTF8 paths:
  - either reject early with a clear error
  - or store raw bytes (base64) alongside a lossy display string
- Include `workspace_root` in outputs so relative paths are resolvable.

**Warning signs:**
- Queries return “file not found” for files that exist.
- `verify` reports a file as missing, but it’s present under a different path form.

**Phase to address:**
Phase 1 (Output schema + normalization). This is foundational for stable IDs.

---

### Pitfall 10: Name-based cross-file resolution produces “confidently wrong” graphs

**What goes wrong:**
The tool links references/calls to the wrong symbol when names collide across files/modules. The output looks plausible but is semantically wrong.

Magellan already documents collision behavior: references keep the “first” symbol for a name; calls use a different policy (prefers current file) (see `.planning/codebase/CONCERNS.md`).

**Why it happens:**
Without semantic resolution (types/imports), teams fall back to matching by name.

**How to avoid (within Magellan’s constraints):**
- Be explicit that cross-file resolution is heuristic unless you have a stronger key.
- Improve determinism and reduce “wrong edges” by:
  - including container/module path in symbol key where possible
  - preferring same-file definitions (consistent across references and calls)
  - emitting “unresolved” edges separately rather than forcing a match
  - recording *candidates* when ambiguous (list of possible symbol_ids)

**Warning signs:**
- Impact analysis shows surprising callers for a function name common in repo.
- Refactor tooling renames the wrong target.

**Phase to address:**
Phase 2–3 (stable identity + resolution policy + validation warnings for ambiguity).

---

## Technical Debt Patterns

Shortcuts that seem reasonable but create long-term problems.

| Shortcut | Immediate Benefit | Long-term Cost | When Acceptable |
|----------|-------------------|----------------|-----------------|
| Use DB entity IDs as public IDs | Zero new design work | IDs churn; consumers break; can’t baseline | Never for public JSON; OK for internal-only debugging |
| Emit “best effort” JSON with ad-hoc fields per command | Fast iteration | Fragmented API surface; hard to maintain | MVP only if schema_v0 is explicitly “unstable” |
| Include timestamps in every output | Easy debugging | Non-deterministic diffs; breaks caching | Only behind `--include-timing` |
| Force-match refs/calls by name | “Complete” graphs | Wrong edges become trusted | Only if also emitting `ambiguity: true` + candidates |
| Log to stdout | Simple | Breaks pipelines; breaks NDJSON | Never (stdout must be data) |

---

## Integration Gotchas

Common mistakes when connecting *developer tooling* to other tools (CI, scripts, editors) even without network services.

| Integration | Common Mistake | Correct Approach |
|-------------|----------------|------------------|
| `jq` / shell pipelines | Mixing progress logs into stdout | stdout = JSON/NDJSON only; logs to stderr |
| CI baselining | Non-deterministic ordering breaks diffs | enforce stable ordering; provide `--stable` default |
| SARIF/other converters (optional) | Missing stable fingerprints | Provide stable IDs + span keys; fingerprints can be derived (see SARIF “fingerprint” concept) |
| “verify” in CI | Verifies wrong root/path form | Always emit workspace_root + normalized file paths |

---

## Performance Traps

Patterns that work at small scale but fail as usage grows.

| Trap | Symptoms | Prevention | When It Breaks |
|------|----------|------------|----------------|
| O(N) DB scans per file event (for refs/calls) | Watch mode lags; high CPU | maintain symbol index table/cache; incremental updates | Medium repos (10k+ symbols) |
| Unbounded watcher queue | Memory growth; late processing | debounce/coalesce; bounded queue | High-churn repos or rapid-save editors |
| Emitting huge monolithic JSON | Memory spikes; slow parsing | support streaming NDJSON; paginate | Very large graphs/exports |

---

## Security Mistakes

Domain-specific security issues for local code graph tools.

| Mistake | Risk | Prevention |
|---------|------|------------|
| Persisting large code snippets by default in shared environments | Sensitive data stored in DB artifacts | Provide flags to disable snippet/chunk storage; document DB as sensitive |
| Logging full file contents/snippets in execution logs | Data leakage via CI logs | Redact by default; log only spans/IDs unless explicitly requested |
| Indexing unbounded file sizes | DoS / disk bloat | enforce max file size; cap snippet lengths |

---

## UX Pitfalls

Common user experience mistakes in deterministic mapping CLIs.

| Pitfall | User Impact | Better Approach |
|---------|-------------|-----------------|
| “JSON mode” still requires reading text docs to interpret | Hard to integrate | Embed schema_version + command metadata in every output |
| Ambiguous results presented as definitive | Users make wrong refactors | Mark ambiguity; provide candidates; expose resolution policy |
| Errors reported only as a single string | Hard to triage programmatically | Structured error objects with codes + locations |

---

## "Looks Done But Isn't" Checklist

- [ ] **Structured JSON output:** stdout contains *only* JSON/NDJSON (no progress/log lines) — verify by piping to `jq`.
- [ ] **Deterministic ordering:** re-run same command twice; output identical byte-for-byte — verify with `diff`.
- [ ] **Stable IDs:** IDs unchanged across reruns on unchanged repo — verify with a golden test.
- [ ] **Span fidelity:** byte offsets match extracted snippet boundaries; non-ASCII files don’t panic — verify with fixtures.
- [ ] **Validation hooks:** a deliberately corrupted DB/file mismatch is detected and returns non-zero exit — verify with tests.

---

## Recovery Strategies

| Pitfall | Recovery Cost | Recovery Steps |
|---------|---------------|----------------|
| Unstable IDs shipped | HIGH | version-bump schema; provide migration/compat layer; add deprecation period |
| Off-by-one spans shipped | HIGH | fix span model; add “span_version”; regenerate snapshots; add compatibility translation |
| stdout contamination shipped | MEDIUM | change logging targets; add `--quiet`; document strict stdout contract |
| Name collision mis-links | MEDIUM | add ambiguity reporting; tighten heuristics; add regression fixtures |
| Watcher event storms | MEDIUM | implement debouncing/coalescing; cap backlog; add metrics counters |

---

## Pitfall-to-Phase Mapping

Suggested roadmap phases referenced below are conceptual; rename to match your actual milestone plan.

| Pitfall | Prevention Phase | Verification |
|---------|------------------|--------------|
| Stable IDs aren’t stable | Phase 1–2 | Golden tests: rerun → identical IDs |
| Mixed coordinate systems | Phase 2 | Span round-trip tests; 0/1-based documented |
| Non-ASCII panics | Phase 2 | Fixtures with multi-byte chars; fuzz-ish tests |
| Output not reproducible | Phase 1 | `diff` identical outputs; deterministic serializer |
| JSON without contract | Phase 1 | JSON Schema validation in CI |
| stdout contamination | Phase 1 | `magellan … | jq` always succeeds |
| Validation checks wrong things | Phase 3 | Inject mismatch; expect structured error + non-zero |
| Watch event storms | Phase 0–1 | Stress test rapid saves; bounded memory |
| Path identity bugs | Phase 1 | Tests with relative/absolute and symlink scenarios |
| Name collision mis-links | Phase 2–3 | Fixtures with duplicate names; ambiguity flagged |

---

## Sources

- SARIF v2.1.0 (OASIS Standard). See especially concepts of *fingerprints* and guidance on determinism (Appendix F). https://docs.oasis-open.org/sarif/sarif/v2.1.0/sarif-v2.1.0.html
- Tree-sitter Rust bindings (`tree-sitter` crate) documentation:
  - `Node::id` uniqueness constraints and reuse notes. https://docs.rs/tree-sitter/latest/tree_sitter/struct.Node.html
  - `Point` row/column are zero-based. https://docs.rs/tree-sitter/latest/tree_sitter/struct.Point.html
- Semgrep CLI docs (for real-world patterns around structured outputs, exit codes, and multiple formats). https://semgrep.dev/docs/cli-reference/
- Magellan internal codebase audit notes:
  - `.planning/codebase/CONCERNS.md` (watcher debounce unused; unbounded queue; lossy paths; file node delete+insert; name collision behavior; UTF-8 slicing panic risk).

---
*Pitfalls research for: deterministic codebase mapping tools (Magellan)*
*Researched: 2026-01-18*
