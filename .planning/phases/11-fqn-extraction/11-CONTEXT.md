# Phase 11: FQN Extraction - Context

**Gathered:** 2026-01-19
**Status:** Ready for planning

## Phase Boundary

Symbol lookup uses fully-qualified names (FQN) as keys, eliminating collisions from simple-name-first-match wins. All 8 supported languages (C, C++, Java, JavaScript, Python, Rust, TypeScript) get FQN extraction in this phase.

## Implementation Decisions

### Migration strategy
- **Forced re-index on upgrade** — Breaking change for existing .db files
- Users will re-index their codebases after upgrading
- Simpler implementation than in-place migration
- No need to preserve old symbol_ids

### Scope tracking depth
- **Hybrid approach: module + type-level scope only**
- FQN format rules:
  - `module::Type::method` — Methods scoped to their type
  - `module::fn` — Free functions include module
  - `module::Trait::method` — Trait methods include trait
  - `module::Const` — Constants at module level
- **Excluded from FQN:**
  - impl blocks (syntactic, not semantic)
  - Generic parameters (span_id handles uniqueness)
  - Closures and anonymous functions (see edge cases)
  - Local scopes (never enter FQN)

**Invariant:** FQN represents semantic ownership, not lexical nesting.

### Anonymous symbols (closures, macros)
- **FQN = parent semantic scope** — Use parent's FQN
- **No pseudo-names** — Never invent `_closure_123` or `_anon_1`
- **span_id guarantees uniqueness** — Identity comes from span, not name
- Metadata flag: `"anonymous": true` for diagnostics
- **Silent at user level** — No warnings in normal output
- **DEBUG logging only** — Available for tracing, never in CLI output

**Golden rule:** If the developer did not name it, Magellan does not invent a name.

### Language scope rules
All 8 languages get FQN extraction in Phase 11:

| Language | Separator | Scope Levels |
|----------|-----------|--------------|
| Rust | `::` | crate::module::Type::method |
| Python | `.` | package.module.Class.method |
| Java | `.` | package.package.Class.method |
| JavaScript/TypeScript | `.` | (module handles vary by ES version) |
| C/C++ | `::` | namespace::Class::method |

### Symbol_id generation
- Keep existing formula: `hash(language, fqn, span_id)`
- FQN changes → all symbol_ids change (accepted via forced re-index)
- No backward compatibility needed for old symbol_ids

### Claude's Discretion
- Exact scope stack implementation (Vec, struct, closure capture)
- Per-language tree-sitter query patterns for scope parent traversal
- JavaScript/TypeScript module handling (CommonJS vs ES modules)
- C++ namespace nesting depth
- Test coverage for edge cases

## Specific Ideas

- "Java already handles dotted package names" — use as reference pattern
- span_id already handles intra-symbol uniqueness — don't duplicate in FQN
- "If the developer did not name it, Magellan does not invent a name" — prevents future design mistakes

## Deferred Ideas

None — discussion stayed within phase scope.

---

*Phase: 11-fqn-extraction*
*Context gathered: 2026-01-19*
