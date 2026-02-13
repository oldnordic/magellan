# Magellan V3 Backend Migration Plan

## Current State Analysis

### Dependencies
- `sqlitegraph = "1.5.7"` (outdated)
- Features: `native-v2`, `native-v2-perf`

### Backend Architecture
- Uses conditional compilation (`#[cfg(feature = "native-v2")]`)
- `CodeGraph` struct wraps backend operations
- Two backends currently:
  1. SQLite backend (default)
  2. Native V2 backend (feature-gated)

### Technical Debt
- **1,071 unwrap() calls** throughout codebase
- Outdated sqlitegraph version (1.5.7 vs 2.0.0)
- No V3 backend support

## Migration Plan

### Phase 1: Dependency Update
1. Update `Cargo.toml`:
   - Bump `sqlitegraph` to `2.0.0`
   - Add `native-v3` feature flag
   - Keep `native-v2` for backward compatibility (deprecated)

### Phase 2: Backend Abstraction
1. Create backend trait/abstraction layer
2. Implement three backends:
   - SQLite (stable, default)
   - Native V2 (deprecated, legacy support)
   - Native V3 (new, high-performance)

### Phase 3: V3 Implementation
1. Add V3 backend initialization
2. Implement all required GraphBackend trait methods
3. Add feature-gated V3 code paths

### Phase 4: Code Quality
1. Audit and fix unwrap() calls systematically
2. Replace with proper error handling
3. Add error context where needed

### Phase 5: Testing
1. Ensure all three backends pass tests
2. Performance comparison
3. CLI compatibility verification

## Implementation Strategy

### Cargo.toml Changes
```toml
[features]
default = []
native-v2 = ["sqlitegraph/native-v2"]  # Deprecated
native-v3 = ["sqlitegraph/native-v3"]  # New, recommended

[dependencies]
sqlitegraph = { version = "2.0.0", default-features = false }
```

### Backend Selection Logic
```rust
#[cfg(feature = "native-v3")]
let backend: Rc<dyn GraphBackend> = {
    use sqlitegraph::backend::native::v3::V3Backend;
    let v3_backend = if db_path_buf.exists() {
        V3Backend::open(&db_path_buf)?
    } else {
        V3Backend::create(&db_path_buf)?
    };
    Rc::new(v3_backend)
};
```

### Error Handling Pattern
Replace:
```rust
let result = something.unwrap();
```

With:
```rust
let result = something.context("Failed to do something")?;
```

## Files to Modify

1. `Cargo.toml` - Dependency and feature updates
2. `src/graph/mod.rs` - Backend initialization
3. `src/lib.rs` - Feature flag documentation
4. Multiple files - unwrap() cleanup

## Timeline

1. Dependency update: ~30 min
2. V3 backend implementation: ~2 hours
3. Unwrap cleanup: ~4 hours (1071 calls)
4. Testing: ~2 hours

Total: ~8-10 hours of work
