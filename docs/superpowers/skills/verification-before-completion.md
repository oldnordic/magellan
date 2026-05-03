# superpowers:verification-before-completion

**Purpose:** Mandatory verification gates that must pass before any task can be marked "complete."

## The Rule

You MUST run ALL verification checks before claiming a task is complete. Do not skip any check due to time pressure, confidence, or previous "looks good" reports.

## Verification Gates

### Gate 1: No Stubs
```bash
grep -rE 'TODO\(|unimplemented!|panic!' src/ --include='*.rs'
```
**Required:** Zero matches. If found, fix or remove before proceeding.

### Gate 2: Build Passes
```bash
cargo check --all-features
```
**Required:** Exit code 0.

### Gate 3: Tests Pass
```bash
cargo test --all-features
```
**Required:** Exit code 0. All tests pass, no ignored/skipped tests.

### Gate 4: Clippy Clean
```bash
cargo clippy --all-targets --all-features
```
**Required:** Zero errors. Warnings must be intentional or fixed.

### Gate 5: Database Health
```bash
magellan status --db .magellan/magellan.db
```
**Required:** files > 0, symbols > 0.

### Gate 6: New Symbols Indexed (if applicable)
```bash
magellan find --name "<new_symbol_name>" --output human
```
**Required:** Symbol found with correct location.

### Gate 7: Build Succeeds
```bash
cargo build --release
```
**Required:** Exit code 0.

## The Script

You can run the full pipeline with:
```bash
./scripts/validate-completion.sh
```

This runs gates 1-5 automatically. Gates 6-7 require manual check.

## On Failure

If ANY gate fails:
1. Report exactly which gate failed
2. Report the specific error output
3. Fix the issue
4. Re-run the gate
5. Only proceed when ALL gates pass

Do NOT explain why you can't fix the failure. Fix it.

## Token Pressure

If tokens are running low and you're tempted to skip verification:
- **STOP.** Skipping verification creates technical debt.
- **Split the work.** If tokens are insufficient, split the task.
- **Use checkpoints.** Every 100 lines of code = one verification gate.

The 20% tokens reserved for verification are not optional.

## Context

This skill is part of the LLM enforcement system. See `docs/superpowers/MASTER_PLAN.md` for the full implementation philosophy.