# Stack Evolution Map (Feanor AI Rust Stack)

**Purpose:** Single-page map of the stack’s evolution and how each project contributes to the full system.

## Thesis (Through-Line)
Build a deterministic, auditable, Rust-native substrate where LLMs operate as operators—not guessers. Every action is grounded in database-backed facts with stable spans, tool outputs, and validation gates. Context compaction becomes safe because truth lives in storage, not in model memory.

---

## Layered Stack (Bottom → Top)

### 1) Truth & Storage Layer
- **SQLiteGraph** (`/home/feanor/Projects/sqlitegraph`)
  - Dual backend (SQLite + Native V2), MVCC, WAL, vector search.
  - Persistent, deterministic graph + vector substrate.
  - Provides embedded durability for knowledge, provenance, and tool outputs.
- **ChronographDB / GeoGraphDB concept** (`/home/feanor/Projects/geometric_db_concept`)
  - Research prototype exploring spatial graph storage (Octree + CSR + MVCC).
  - Future direction: geometry-aware graph layouts, GPU‑friendly spatial reasoning.

### 2) Code Grounding & Safe Editing
- **Magellan** (`/home/feanor/Projects/magellan`)
  - Tree‑sitter indexer to map symbols, references, call graph.
  - Produces stable spans + metadata for reliable refactoring.
- **Splice** (`/home/feanor/Projects/splice`)
  - Span‑safe refactoring with LSP validation gates.
  - Execution logs for auditability + rollback.

### 3) Tool Governance & Unified Contracts
- **Unified JSON Schema** (`/home/feanor/Projects/magellan/docs/UNIFIED_JSON_SCHEMA.md`)
  - Shared response schema: spans, execution IDs, deterministic metadata.
- **SynCore** (`/home/feanor/Projects/syncore`)
  - MCP server with response envelope contracts, streaming limits, tool hygiene.
  - Makes tool outputs safe for LLM consumption at scale.

### 4) Memory‑First Operator Loop
- **OdinCode** (`/home/feanor/Projects/odincode`)
  - Execution DB stores tool calls/results/LLM reasoning.
  - LLM gets compact summaries, queries full context on demand.
  - Eliminates repeated file reads and guesswork.

### 5) LLM Toolchain (Deterministic Micro‑tools)
- **llmsearch, llmastsearch, llm-discover, llm-transform** (`/home/feanor/Projects/llm*`)
  - Deterministic, JSON‑first tools for file discovery, text/AST search, and span edits.
  - Designed to interoperate via unified schema with Magellan/Splice.

### 6) Inference & Compute Layer
- **ROCmForge** (`/home/feanor/Projects/ROCmForge`)
  - GPU‑first inference engine with ROCm/HIP, GGUF loading, KV cache.
- **SimdFlow** (`/home/feanor/Projects/SimdFlow`)
  - CPU/iGPU‑first inference plan: cache‑blocked matmul, SIMD kernels, fused dequant.

---

## Evolution Highlights (Concept → System)

1. **Truth over tokens**: Replace re-reading and guessing with indexed spans + stored outputs.
2. **Deterministic tool substrate**: Every tool returns structured JSON with stable IDs.
3. **Execution memory**: Store tool results + reasoning in DB, query later, avoid context bloat.
4. **Validation gates**: LSP + compiler checks enforce correctness before edits land.
5. **Full-stack Rust**: From storage to inference, minimizing glue code and maximizing auditability.

---

## Current Alignment Gaps (High-Level)
- **Splice + LLM tools** not fully aligned to unified schema (wrapper + span IDs).
- **Tool outputs** vary in `file_path`/`span` structure.
- **Execution log schemas** not yet normalized across tools.

(See: `UNIFIED_JSON_SCHEMA_GAPS.md` in each project’s `docs/`.)

---

## Endgame (Target System)
A portable ABI that exposes the full toolchain (LangChain/LangGraph‑style) without losing determinism:
- All tool calls are logged and auditable.
- Memory persists across sessions and compactions.
- LLMs operate on verified, retrievable facts.
- Compute layer integrates memory (KV + graph) for inference‑time grounding.

---

## Suggested Next Consolidation Steps
1. Align Splice and LLM tools to the unified JSON schema.
2. Normalize execution logs and tool outputs across repos.
3. Wire OdinCode memory query to all tool results.
4. Define ABI contracts for external clients.

