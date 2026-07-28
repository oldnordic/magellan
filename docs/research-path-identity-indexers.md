# Research: Path identity & normalization in code indexers — recommendations for magellan's path contract

**Status:** research note, uncommitted. Produced 2026-07-27.
**Context:** commit `ad28c295` ("fix(graph): resolve relative-path lookups against stored index paths, not process cwd") shipped a two-stage contract for `find_file_node` / `symbol_id_by_name` / `find_all_file_nodes`: (1) exact match after query-side normalization against the process cwd, (2) a deterministic path-segment suffix fallback in both directions (relative query ↔ absolute stored), unique match wins, ambiguous → `None`. The root problem: ingest-time stored paths and query-time paths diverge when the consumer's cwd ≠ the ingest-time index root, and `normalize_path_for_index` resolves relative queries against process cwd (`src/graph/files.rs:61`).

This note surveys how other indexers and code-graph tools define *file path identity*, then derives a concrete contract recommendation for magellan.

---

## 1. graphify (Graphify-Labs/graphify)

**What it actually is:** not the older Python/Neo4j tool one might expect — it is *graphify* by Graphify Labs (PyPI package `graphifyy`, ~97k★), a fully-local tree-sitter-based code knowledge-graph builder shipped as a `/graphify` skill for Claude Code/Cursor/Codex. Output is `graphify-out/graph.json` + `manifest.json`, no database server.
Source: https://github.com/Graphify-Labs/graphify

**Path identity scheme:** graphify made exactly the migration magellan is contemplating. In `graphify/detect.py`:

- `save_manifest(..., root=root)` writes manifest keys **relativized against the scan root, forward-slash POSIX-style**, "so the on-disk manifest is portable across machines and checkout locations (#777). Out-of-root entries are written as absolute."
- `load_manifest(..., root=root)` **re-anchors** relative keys against the provided root (`_to_absolute_from_storage`), while **legacy absolute keys pass through unchanged** for backward compatibility.
- Keys are **NFC-normalized** on both save and load, because macOS returns NFD paths from `os.getcwd()` while explicit path literals are typically NFC; raw string comparison between the two forms made incremental `--update` re-extract everything (issue #2221, fix in #2224).
- Node IDs in the graph itself (`graphify/build.py`) are also built root-relative: `Path(p).relative_to(root).as_posix()`, with a resolved-both-sides retry if the lexical `relative_to` fails, and a migration shim for pre-#1504 node IDs.

Sources:
- https://raw.githubusercontent.com/Graphify-Labs/graphify/v8/graphify/detect.py (`save_manifest`/`load_manifest`/`_to_absolute_from_storage` docstrings)
- https://github.com/Graphify-Labs/graphify/issues/2221 (NFC/NFD manifest mismatch on macOS)
- https://raw.githubusercontent.com/Graphify-Labs/graphify/v8/graphify/build.py (node id derivation, ~line 172)

**Trade-off:** root-relative storage gives portability (moved checkouts, shared `graphify-out/`) at the cost of needing the root at load time; the dual-format reader (relative re-anchored, legacy absolute pass-through) is the standard migration pattern. Graphify has *no* suffix matching anywhere — mismatches are resolved only via the recorded root.

## 2. SCIP (Sourcegraph Code Intelligence Protocol)

**Path identity scheme:** SCIP hard-codes the strictest version of root-relative identity. In `scip.proto`, `Metadata.project_root` is a "URI-encoded absolute path to the root directory of this index" and every `Document.relative_path` must satisfy five invariants:

1. relative to `Metadata.project_root`;
2. no leading `/`;
3. points to a regular file, not a symlink;
4. `/` separator, **including on Windows**;
5. canonical — no empty components, no `.` or `..`.

Source: https://github.com/scip-code/scip/blob/main/scip.proto (`Metadata.project_root`, `Document.relative_path`)

**Trade-off:** the index is fully relocatable and machine-independent; identity is `(project_root, relative_path)` and the root is *metadata inside the index*, not ambient process state. There is no fallback matching — a consumer that can't anchor paths to `project_root` simply has the wrong index. This is the strongest existence proof that "record the root in the index, store everything root-relative, canonical form" is the industry-standard contract.

## 3. LSIF (Language Server Index Format)

**Path identity scheme:** LSIF (SCIP's predecessor, same design lineage) keys documents by URI, and the spec states the dump carries a `workspaceRoot`: "it is the workspace root URI used when creating the dump. It allows for a relative interpretation of other URI's in the dump like document URIs." Cross-dump linking happens via *monikers* (package-scoped symbol identities), not paths, so path identity only needs to be stable within one dump.

Source: https://microsoft.github.io/language-server-protocol/specifications/lsif/0.6.0/specification/ (Meta Data section)

**Trade-off:** same as SCIP — explicit root recorded in the payload, relative document URIs. LSIF additionally shows the separation of concerns: paths locate documents; monikers locate symbols. Magellan conflates the two when `symbol_id_by_name(path, name)` must first resolve the path.

## 4. Kythe

**Path identity scheme:** Kythe never uses host filesystem paths as identity at all. Every node is a VName: `(signature, corpus, root, path, language)` where `corpus` is the node's containing corpus (typically the VCS repository label), `root` a root path within the corpus, and `path` relative to **both** corpus and root. A separate `vcs` node kind ("a reference to a particular revision stored in a version control system") attaches provenance, so identity is stable across machines, checkouts, and absolute locations by construction.

Source: https://kythe.io/docs/schema/ (VName conventions; node kinds `file`, `vcs`)

**Trade-off:** maximal portability and cross-repo joins, at the cost of an extraction pipeline that must know the corpus label up front. The lesson for magellan is narrower: the *corpus/root* pair is exactly "which index is this, and where was its root" — Kythe proves that recording provenance (repo/revision) in the index makes path identity independent of checkout location.

## 5. zoekt (Sourcegraph code search)

**Path identity scheme:** zoekt shards store each document's name **exactly as the indexer supplied it**, alongside structured `Repository` metadata (repo name/ID, branches, URL templates); `ShardBuilder` keeps `repoList []zoekt.Repository`, per-doc branch masks, and derives shard filenames from the URL-escaped repo name (`shardName()`). Because `zoekt-git-index` walks a git tree, document names are repo-tree-relative by construction; the local filesystem location of the checkout never enters the index.

Source: https://raw.githubusercontent.com/sourcegraph/zoekt/main/index/shard_builder.go (`repoList`, `branchMasks`, `shardName`)

**Trade-off:** identity is `(repo, branch, repo-relative path)` — indexing git content rather than filesystem paths sidesteps the cwd/checkout problem entirely. Not directly applicable to a filesystem indexer like magellan, but reinforces: the index should describe *where content came from*, and paths should be relative to that origin.

## 6. livegrep

**Path identity scheme:** livegrep's `codesearch` backend indexes repositories declared in a protobuf/JSON config (path + name + metadata); the resulting `.idx` file is **standalone** — "Index files are standalone, and you no longer need access to the source code repositories, or even a configuration file, once an index has been built." Result identity is `(tree/repo name, path, version)` from config metadata; linking out uses per-repo `url_pattern` templates.

Source: https://raw.githubusercontent.com/livegrep/livegrep/main/README.md ("Using Index Files", "Local repository browser")

**Trade-off:** like zoekt, paths are relative to a declared repo root from config, never to process cwd. The standalone-index property is one magellan's DB already shares — but magellan currently *does* leak host paths into the standalone artifact, which livegrep/zoekt avoid.

## 7. universal-ctags

**Path identity scheme:** ctags records file paths **as given on the command line** and offers `--tag-relative=(yes|no|always|never)` to control whether recorded paths are "relative to the directory containing the tag file" vs "relative to the current directory" vs absolute. The client (editor) resolves them; Vim additionally has its own `tagrelative` option. ctags is the canonical example of the *ambient-cwd* failure mode magellan just fixed: the tag file's meaning depends on where ctags was invoked and where the consumer runs.

Source: https://docs.ctags.io/en/latest/man/ctags.1.html (`--tag-relative`)

**Trade-off:** zero ingest-time decisions, but pushes ambiguity to every consumer. The `--tag-relative` family exists precisely because unspecified anchors cause breakage — and the "relative to the tag file's directory" option is the closest analog to magellan recording the index root.

## 8. GNU Global (gtags/GTAGS)

**Path identity scheme:** gtags builds its databases (GTAGS/GPATH/GRTAGS) at the **project root**, and `global` prints **relative paths by default** (`-a` for absolute). `global -p` prints the project root; result format is configurable as `relative | absolute | through` ("the relative path from the project root directory"). The DB can live outside the tree via `GTAGSROOT` + `GTAGSDBPATH`, which is an explicit recorded anchor separating "where the DB is" from "what the paths are relative to".

Source: https://www.gnu.org/software/global/globaldoc_toc.html (§1.2 Concept of project; gtags/global man sections, `--print-dbpath`, path format options)

**Trade-off:** the oldest working version of the pattern: one well-known anchor directory (project root), all stored paths relative to it, consumers locate the anchor (`global -p`) rather than guessing. Notably, Global resolves the anchor by *walking up to find the DB*, never by suffix-matching stored paths.

## 9. rust-analyzer VFS

**Path identity scheme:** rust-analyzer interns paths to `FileId`s in the VFS; `VfsPath` is internally an absolute `AbsPathBuf` (from the `paths` crate, which guarantees absoluteness at the type level). Relative addressing exists only as `AnchoredPath` — "path relative to a `FileSet` root" — so every relative reference carries its anchor explicitly. cwd is never consulted; the project model (cargo metadata) supplies absolute roots.

Source: https://github.com/rust-lang/rust-analyzer/blob/master/crates/vfs/src/lib.rs (crate docs: `FileId`, `VfsPath`, `AnchoredPath`, `file_set::FileSet`)

**Trade-off:** interning makes identity an integer within a session (paths can't drift once interned), and the type system (`AbsPathBuf` vs `AnchoredPath`) makes "relative to what?" unrepresentable-when-wrong. The lesson for magellan: make the anchor part of the value, not a convention — or failing that, make stored keys unambiguous by construction (root-relative + recorded root).

## 10. GitHub code search (Blackbird)

**Path identity scheme:** Blackbird shards by git blob OID (content address) and keeps *locations* — "which path, branch, and repository" — as metadata; document identity is content-derived, and path is just one facet of location metadata alongside repo name/owner/visibility. Paths are repo-tree-relative by construction.

Source: https://github.blog/engineering/the-technology-behind-githubs-new-code-search/ ("Indexing 45 million repositories")

**Trade-off:** content-addressed identity is out of scope for magellan, but the design underlines the same point: `(repo, commit, tree-relative path)` is the only path scheme that survives arbitrary checkout locations.

---

## Comparison table

| Tool | Stored path form | Anchor recorded in index? | Consumer-side resolution | Fallback guessing? |
|---|---|---|---|---|
| graphify | root-relative POSIX (+ legacy absolute) | implicit (graphify-out/ location) | re-anchor against provided root; NFC-normalized | No |
| SCIP | `relative_path` vs `Metadata.project_root`, canonical, `/` | **Yes** — `project_root` in payload | join to `project_root` | No |
| LSIF | document URI relative to `workspaceRoot` | **Yes** — `workspaceRoot` in dump | join to `workspaceRoot`; symbols via monikers | No |
| Kythe | VName `(corpus, root, path)` + `vcs` revision | **Yes** — corpus/root in every VName | corpus-relative by construction | No |
| zoekt | repo-tree-relative name + repo metadata | **Yes** — Repository record per shard | repo-relative by construction | No |
| livegrep | config-declared repo path/name | Yes — config metadata in standalone idx | repo-relative by construction | No |
| ctags | as-invoked; `--tag-relative` to tag-file dir or cwd | Partial (tag file location) | editor-side, Vim `tagrelative` | Effectively yes (consumer guesses) |
| gtags/GLOBAL | project-root-relative (default output) | Yes — project root at DB location, `global -p` | locate DB by walking up; paths relative to root | No |
| rust-analyzer | absolute interned `AbsPathBuf`; `AnchoredPath` for relative | Yes — anchor is part of the type | interned FileId; anchor explicit | No |
| Blackbird | repo-tree-relative + blob OID | Yes — repo/commit metadata | repo-relative by construction | No |

**Is suffix-fallback a known pattern?** No serious indexer uses suffix matching to establish path identity. Every surveyed system either (a) records the anchor in the index and stores root-relative keys (SCIP, LSIF, Kythe, zoekt, livegrep, gtags, graphify), or (b) makes the anchor part of the value/type (rust-analyzer), or (c) punts ambiguity to the consumer, which is the ctags failure mode. Suffix matching is considered an anti-pattern for identity because same-named files (`src/lib.rs` in N crates, `__init__.py`, `index.ts`) are the norm in real repos; a unique-match guard bounds the damage but can silently go from unique → ambiguous → `None` as the index grows, which is exactly the silent-degradation shape of the original bug. It is defensible **only** as a migration shim for legacy DBs that lack a recorded root.

---

## Recommendations for magellan's path contract

| # | Recommendation | Precedent | Effort |
|---|---|---|---|
| R1 | **Record `index_root` (canonicalized, absolute, NFC) in `graph_meta` at ingest time.** This is the single highest-value change: it converts the ambient-cwd problem into a data problem. | SCIP `Metadata.project_root`; LSIF `workspaceRoot`; Kythe corpus/root; gtags project root | Small |
| R2 | **Normalize all newly-stored file paths to index-root-relative, POSIX `/`-separated, canonical form** (no `.`/`..`, no `//`, NFC). Keep reading legacy absolute/relative stored keys via dual-read (re-anchor relative legacy keys against recorded `index_root`; pass absolute keys through) — graphify's exact migration pattern. | SCIP `Document.relative_path` rules 1–5; graphify `save_manifest`/`load_manifest` (#777, #2221) | Medium (write path + read shim + migration tests) |
| R3 | **Query resolution order: exact → index_root-anchored → cwd → suffix (last resort, deprecation-logged).** Today: exact(cwd) → suffix. Insert the anchor step: relative queries join to recorded `index_root`; absolute queries strip `index_root` prefix. With R2 this makes the suffix branch nearly dead; keep it (unique-match-guarded) only for pre-R2 databases, and log when it fires so remaining callers can be fixed. | gtags `through` (root-relative), rust-analyzer `AnchoredPath` | Small |
| R4 | **NFC-normalize path keys at write and at compare.** Magellan targets Linux primarily, but DBs move between machines; graphify #2221 shows NFC/NFD mismatches silently break incremental behavior on macOS. | graphify `_nfc()` on manifest save/load | Trivial |
| R5 | **(Optional, later) Record VCS provenance** (`git rev-parse HEAD`, remote URL) in `graph_meta` so consumers can detect stale indexes and so multi-checkout identity is well-defined. | Kythe `vcs` node; zoekt branch metadata | Medium |
| R6 | **Do not extend suffix-fallback further; plan its retirement.** Mark it transitional in the contract docs: acceptable for reading legacy DBs, never for establishing identity in new code. Every surveyed indexer resolved this by recording the anchor, not by better guessing. | whole survey | Doc |

### What "strictly better" looks like (concrete contract)

1. **Ingest:** resolve scan root once → `index_root` (canonical, NFC, absolute) → store in `graph_meta`. Every file path is stored as `relpath(file, index_root)` in POSIX form, canonical, NFC. Out-of-root files stored absolute (graphify's rule).
2. **Load/query:** read `index_root` once per session. Resolve a query path as: exact match → join/strip against `index_root` → legacy absolute pass-through → (legacy DBs only) cwd-normalized exact → (legacy DBs only) suffix fallback with unique-match guard + warning log.
3. **Never** resolve *stored* paths against the opener's cwd (already fixed in `ad28c295` via `normalize_stored_path`); with R1+R2, stored paths are always interpreted against the recorded root instead.
4. Tests: TempDir-anchored fixtures (as in `ad28c295`) plus a "moved checkout" test — copy DB + tree to a new absolute location, verify all lookups succeed via re-anchoring. This is the scenario no fallback heuristic can fake, and the one R1+R2 makes pass by construction.

## Sources (all fetched 2026-07-27)

- graphify repo & README — https://github.com/Graphify-Labs/graphify
## Positioning note (2026-07-27)

graphify's edges are tree-sitter AST guesses (`calls`/`imports`/`inherits`).
This stack's differentiator is compiler-grounded structure: real ICFG with
precise call-site stitching (mirage `icfg`), and CFG extracted via the actual
toolchains — Rust nightly MIR (`src/graph/external_tools/rust/mir_invoker.rs`),
Java bytecode `.class` parsing (`src/graph/external_tools/java/class_parser.rs`),
C/C++ via `compile_commands.json` + clang (`compile_commands.rs`). Path-identity
lessons transfer; the graph-semantics bar here is higher than anything surveyed.

- graphify `detect.py` (manifest key scheme, #777, #2221) — https://raw.githubusercontent.com/Graphify-Labs/graphify/v8/graphify/detect.py
- graphify `build.py` (root-relative node IDs, pre-#1504 migration) — https://raw.githubusercontent.com/Graphify-Labs/graphify/v8/graphify/build.py
- graphify issue #2221 (NFC/NFD) — https://github.com/Graphify-Labs/graphify/issues/2221
- SCIP schema — https://github.com/scip-code/scip/blob/main/scip.proto
- LSIF 0.6.0 spec — https://microsoft.github.io/language-server-protocol/specifications/lsif/0.6.0/specification/
- Kythe schema (VName, vcs) — https://kythe.io/docs/schema/
- zoekt `index/shard_builder.go` — https://raw.githubusercontent.com/sourcegraph/zoekt/main/index/shard_builder.go
- livegrep README — https://raw.githubusercontent.com/livegrep/livegrep/main/README.md
- universal-ctags man page (`--tag-relative`) — https://docs.ctags.io/en/latest/man/ctags.1.html
- GNU Global manual — https://www.gnu.org/software/global/globaldoc_toc.html
- rust-analyzer VFS — https://github.com/rust-lang/rust-analyzer/blob/master/crates/vfs/src/lib.rs
- GitHub Blackbird — https://github.blog/engineering/the-technology-behind-githubs-new-code-search/
