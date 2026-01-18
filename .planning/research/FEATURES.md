# Feature Research

**Domain:** Deterministic codebase mapping / code graph indexing tools (local developer CLI)
**Researched:** 2026-01-18
**Confidence:** MEDIUM

## Feature Landscape

### Table Stakes (Users Expect These)

Features users assume exist. Missing these = product feels incomplete.

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| Deterministic indexing results | Users need repeatable automation + diff-friendly outputs | MEDIUM | Same inputs → same outputs (ordering, IDs, counts). Deterministic path canonicalization + stable sort order are mandatory.
| “Scan initial” full baseline before incremental | Users expect a complete map, not partial/lagging state | MEDIUM | Watchers that only react to changes feel unreliable; baseline must be produced first, then incremental updates.
| Incremental watch mode (create/modify/delete) | Core dev workflow is “edit → re-run tooling quickly” | HIGH | Requires robust filesystem events + debouncing + idempotent DB updates.
| Per-file error tolerance | Real repos have broken/generated/unparseable files | MEDIUM | Must keep running; record errors as structured diagnostics instead of crashing.
| Ignore rules / file discovery filters | Users expect parity with common CLI tooling (gitignore, etc.) | MEDIUM | At minimum: respect `.gitignore`-style ignore and explicit `--include/--exclude` globs. (Ripgrep and Semgrep model this heavily.)
| Clear language detection + per-language configuration | Multi-language repos are common | MEDIUM | Detect by extension + allow explicit override; report language per document.
| Span-aware locations in outputs | Downstream tools need precise mapping back to source | MEDIUM | Include byte offsets and line/col; explicitly state encoding and whether ranges are half-open. SCIP/LSIF emphasize this.
| Core graph facts: definitions + references | This is the minimum useful “code navigation” dataset | HIGH | Definitions alone are not sufficient; need reference occurrences for “find refs” and impact.
| Core relationships: caller → callee edges | Call graph is a baseline expectation for “mapping” tools | HIGH | Even if heuristic, must be deterministic; label edge type and confidence.
| Simple query surface (CLI) | Local developers script tools via CLI | MEDIUM | Table stakes commands: find definition, find references, callers/callees, list symbols in file/module, export.
| Structured output format (JSON) | Tooling integrations expect machine-readable output | MEDIUM | Provide stable schema versioning, consistent error object shape, and deterministic ordering.
| Stable exit codes | Scripts/CI need predictable behavior | LOW | Common pattern: `0` success, non-zero for fatal errors; optionally separate codes for partial failures (Semgrep documents explicit exit codes).
| DB portability and inspectability | Local dev tools are often debugged via direct DB inspection | LOW | SQLite DB + documented schema; avoid hidden binary formats.
| Reproducible run metadata | Users need to know “what produced this graph?” | MEDIUM | Record tool version + args + project root; SCIP includes tool info and project root metadata.

### Differentiators (Competitive Advantage)

Features that set the product apart. Not required, but valuable.

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| **Stable IDs** for symbols, matches, spans, and executions | Enables safe downstream automation (patching, refactors, caching, audit trails) | HIGH | Distinguish: `symbol_id` (semantic identity), `span_id` (location identity), `match_id` (query result identity), `execution_id` (run identity). LSIF/SCIP provide precedent for stable symbol strings and range encodings.
| Deterministic structured outputs with explicit schemas | Makes the CLI feel like an API, not a human tool | MEDIUM | JSON Schema / OpenAPI-like docs; enforce “no implicit defaults” and “always include schema_version”.
| Span fidelity options (byte + line/col + enclosing range) | Reduces ambiguity in tooling and improves UX for “jump to code” | MEDIUM | SCIP includes both a range and an `enclosing_range` concept (useful for call hierarchies and outline).
| Validation hooks (pre/post) | Trust: detect stale DB, partial indexing, or drift early | HIGH | Examples: input file hash manifest, DB invariants, foreign key consistency, optional “no missing files” checks, post-run sanity stats.
| Execution logging + replay | Debuggability: “why did this result change?” | MEDIUM | Store: invocation args, environment, tool version, project root, file list/hash summary, timing, error list.
| Deterministic incremental updates (event-sourced semantics) | Prevents watcher flakiness and “ghost edges” | HIGH | Treat updates as idempotent transactions; handle rename/move as delete+create with stable path normalization.
| Export to interoperable index formats (SCIP / LSIF) | Integrates with broader ecosystem and downstream tooling | HIGH | SCIP and LSIF are common interchange formats for code-intel; exporting makes Magellan a “local indexer” feeding other tools.
| Query-time filters and guarantees | Makes results safer for automation | MEDIUM | Examples: `--max-results`, `--order-by`, `--deterministic`, `--require-complete` (fail if partial), `--include-errors`.
| Multi-workspace / multi-db workflows | Enables monorepos and comparisons | HIGH | Even if out-of-scope for v1, offering “merge two DBs”, “compare snapshots”, or “multi-root read-only query” is a strong differentiator.
| Rich diagnostics model | Faster iteration on parser issues and repo-specific failures | MEDIUM | Structured errors: file, language, stage (parse/index/write), severity, retryability.
| Performance without losing determinism | “Fast enough” while still reproducible | HIGH | Parallel parsing is fine if commit ordering is deterministic; provide per-stage timing output.

### Anti-Features (Commonly Requested, Often Problematic)

Features that seem good but create problems.

| Feature | Why Requested | Why Problematic | Alternative |
|---------|---------------|-----------------|-------------|
| “Best effort” renaming/refactoring built into the tool | Users want one tool to do everything | Blurs scope; can silently break code without semantic/type info | Keep Magellan “facts only”; export spans/IDs so other refactor tools can act safely.
| Heuristic text-based symbol search (grep-as-symbol) | Quick to implement; seems to work on small repos | Produces false positives/negatives; not span-safe; undermines trust | Use AST-derived spans + explicit symbol tables (tree-sitter driven).
| Non-deterministic parallel output (race-based ordering) | Speed | Breaks diffing, caching, and stable IDs | Always sort outputs; decouple parallel work from deterministic commit.
| Hidden global state/config (implicit DB paths, auto-magic) | Convenience | Makes runs non-reproducible and hard to automate | Explicit CLI flags (`--db`, `--root`, `--include/--exclude`) and printed “run context”.
| Always-on network / telemetry | Product analytics | Violates “local developer tool” trust model; breaks airgapped usage | Offline by default; if ever added, opt-in only.
| “Fix my code” autofix mode | Users want cleanup | Risk of data loss; surprises in local tools (Semgrep warns about autofix risk) | Offer preview-only suggestions and export patches, but don’t apply by default.
| Automatic DB cleanup / compaction without user control | Users want low disk use | Can destroy historical auditability and makes debugging harder | Provide explicit maintenance commands with dry-run and logs.

## Feature Dependencies

```
[Deterministic file discovery + canonical paths]
    └──requires──> [Stable span model (byte + line/col + encoding + half-open rules)]
                       └──requires──> [Stable IDs for spans/symbols]

[Initial scan baseline]
    └──requires──> [Incremental watcher updates]

[Structured JSON schema versioning]
    └──enables──> [Execution logging + replay]
                       └──enables──> [Validation hooks + audit trail]

[AST extraction per language]
    └──requires──> [Language detection + per-language parser configuration]

[DB persistence]
    └──enables──> [Query CLI + export]

[Export to SCIP/LSIF]
    └──requires──> [Span model + stable IDs + symbol naming rules]
```

### Dependency Notes

- **Deterministic file discovery requires canonical paths + ignore rules:** stable IDs and deterministic outputs are impossible if the same file can be addressed in multiple ways (symlinks, `./` vs absolute, case differences).
- **Stable IDs require a stable span model:** you need unambiguous range semantics (byte vs UTF-16 vs UTF-32, 0-based vs 1-based, half-open vs closed intervals). SCIP and LSIF both explicitly define position encodings and 0-based ranges.
- **Incremental watch requires an initial baseline:** otherwise the DB state depends on “what changed while I was watching,” which is not reproducible.
- **Validation hooks require execution logging:** you need to record what was validated (inputs, hashes, tool version) to make validation meaningful and debuggable.
- **SCIP/LSIF export requires adopting their range/encoding conventions:** both formats are explicit about position encoding and range constraints; adopting them early prevents later rewrites.

## MVP Definition

### Launch With (v1)

Minimum viable product — what's needed to validate the concept.

- [ ] Deterministic scan (single-shot) that produces DB + JSON outputs — proves “facts only, correct and repeatable”
- [ ] Span-aware outputs everywhere (byte offsets + line/col + explicit encoding + deterministic order) — enables downstream tools
- [ ] Stable IDs (`execution_id`, `symbol_id`, `span_id`, `match_id`) — makes outputs automation-safe
- [ ] Core queries: definition, references, callers/callees, list symbols in file — table-stakes CLI surface
- [ ] Watch mode: initial scan + deterministic incremental updates + per-file error tolerance — main dev workflow
- [ ] Execution logging (structured, queryable) + minimal validation hooks (input manifest/hash summary) — makes the system trustworthy

### Add After Validation (v1.x)

Features to add once core is working.

- [ ] Export to SCIP and/or LSIF — unlocks ecosystem interoperability
- [ ] Richer validation modes (DB invariants, orphan edge detection, “completeness gates”) — reduce silent corruption
- [ ] Performance work that preserves determinism (parallel parse with ordered commits, caching) — scaling to monorepos
- [ ] Better ignore/include UX (profiles, per-language defaults, explain-why-file-skipped) — reduces user confusion

### Future Consideration (v2+)

Features to defer until product-market fit is established.

- [ ] Multi-root workspaces / DB merging — complex correctness surface area
- [ ] Pluggable language adapters (external plugin API) — hard to stabilize early
- [ ] Optional semantic augmentation (type info) — explicitly out-of-scope for Magellan v1; requires new trust model

## Feature Prioritization Matrix

| Feature | User Value | Implementation Cost | Priority |
|---------|------------|---------------------|----------|
| Deterministic structured JSON outputs (schema-versioned) | HIGH | MEDIUM | P1 |
| Span-aware outputs with explicit encoding + half-open rules | HIGH | MEDIUM | P1 |
| Stable IDs (symbol/span/match/execution) | HIGH | HIGH | P1 |
| Initial scan baseline + deterministic ordering | HIGH | MEDIUM | P1 |
| Watch mode (incremental updates + idempotent DB writes) | HIGH | HIGH | P1 |
| Per-file error tolerance + diagnostics | HIGH | MEDIUM | P1 |
| Validation hooks + execution log | MEDIUM | MEDIUM | P2 |
| Export to SCIP/LSIF | MEDIUM | HIGH | P2 |
| Performance scaling without nondeterminism | MEDIUM | HIGH | P2 |
| Multi-root / DB merge workflows | LOW | HIGH | P3 |

**Priority key:**
- P1: Must have for launch
- P2: Should have, add when possible
- P3: Nice to have, future consideration

## Competitor Feature Analysis

| Feature | Competitor A | Competitor B | Our Approach |
|---------|--------------|--------------|--------------|
| Tagging / symbol extraction | Universal Ctags: broad language support, editor integration (tags) | SCIP indexers: compiler-backed for some langs, standardized protocol | Magellan: tree-sitter AST across 7 languages, local-first DB + CLI queries |
| Structured output + schemas | Ripgrep/others show strong CLI discipline but not a code graph | SCIP/LSIF define strongly structured interchange formats | Magellan: JSON output with explicit schemas + deterministic ordering; optionally export to SCIP/LSIF |
| Stable symbol identity | Ctags: names + locations, but stable identity varies and collisions happen | SCIP: explicit symbol string grammar + per-document position encoding | Magellan: explicit stable ID strategy (symbol_id + span_id) + execution_id auditing |
| Ignore/filter behaviors | Ripgrep: extensive ignore and filtering capabilities | Semgrep: `.semgrepignore` + include/exclude and clear exit codes | Magellan: `.gitignore`-style ignore + `--include/--exclude` + “explain skipped” mode |

## Sources

- Sourcegraph SCIP protocol (position encoding, symbol identity grammar): https://github.com/sourcegraph/scip and https://raw.githubusercontent.com/sourcegraph/scip/main/scip.proto
- Microsoft LSIF specification (unique IDs, ranges, streaming events, stable monikers): https://raw.githubusercontent.com/microsoft/language-server-protocol/main/indexFormat/specification.md
- Ripgrep guide (ignore behavior, deterministic sort option, config discipline): https://raw.githubusercontent.com/BurntSushi/ripgrep/master/GUIDE.md
- Semgrep CLI reference (ignore file concept + exit code conventions; warnings about autofix): https://semgrep.dev/docs/cli-reference
- Universal Ctags overview (tags as baseline code object indexing): https://github.com/universal-ctags/ctags

---
*Feature research for: deterministic codebase mapping / code graph indexing tooling*
*Researched: 2026-01-18*
