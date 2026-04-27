# Testing Magellan

This document lists the current verification commands for the supported
SQLite-first workflow.

## Fast Checks

```bash
cargo check
cargo check --all-targets
cargo check --features external-tools-cfg --all-targets
```

## Focused Tests

```bash
cargo test --lib
cargo test --test coverage_weighted_cfg_tests
cargo test --test fqn_integration_tests
cargo test --test multi_language_integration_tests
cargo test --test phase1_persistence_compatibility
cargo test --features external-tools-cfg --test external_tools_tests
```

## Full Suite

```bash
cargo test
```

Known current caveat: `tests/stress_concurrent_edits.rs::stress_database_integrity`
can deadlock and fail after its timeout. Do not report full-suite success unless
that test has also passed in the current run.

## CLI Smoke Checks

```bash
cargo run -- status --db /tmp/magellan-smoke.db
cargo run -- watch --root . --db /tmp/magellan-smoke.db --scan-initial --validate-only
cargo run -- --help
cargo run -- --help-full
cargo run -- --backends
```

## Coverage Checks

```bash
cargo test --test coverage_weighted_cfg_tests
magellan ingest-coverage --db code.db --lcov coverage/lcov.info
magellan status --db code.db --output pretty
```

## External Tools CFG

```bash
cargo check --features external-tools-cfg --all-targets
cargo test --features external-tools-cfg --test external_tools_tests
```

These tests should gracefully handle missing external tools where the code is
designed to fall back.
