# superpowers:no-stubs-enforcement

**Purpose:** Zero tolerance for stub code, TODO, unimplemented, or placeholder implementations.

## The Rule

The following are **STRICTLY FORBIDDEN** in this codebase:
- `TODO()` or `todo!()` macros
- `unimplemented!()` anywhere except genuinely unreachable code
- `panic!()` except in truly unrecoverable situations
- Placeholder functions returning dummy values
- Mock/stub implementations marked "for now"
- Commented-out code "for later reference"
- `// TODO:`, `// FIXME:`, `// HACK:` comments

## Why

Stub code rots. "For now" becomes "forever." The 4D coordinates bug proved this - incomplete implementations break downstream users who assume features work.

LLMs are particularly prone to leaving stubs when tokens run low. This skill enforces completion or removal.

## Detection

Run this before claiming completion:
```bash
grep -rE 'TODO\(|unimplemented!|panic!' src/ --include='*.rs'
```

**Zero matches required.** If found, you MUST either:
1. Implement the feature properly, OR
2. Remove it from the codebase entirely

"Doing it later" = "never" in this codebase.

## Enforcement

When a stub is detected:
1. Report the exact location: file:line:column
2. State what must be done: implement OR remove
3. Do NOT proceed until resolved
4. Verify with grep again after fix

## Verification Checklist

Before claiming ANY task complete, verify:
- [ ] `grep -rE 'TODO\(|unimplemented!|panic!' src/ --include='*.rs'` returns nothing
- [ ] No placeholder comments exist
- [ ] No dummy return values
- [ ] All functions have real implementations
- [ ] Error handling is proper (not just `unwrap()`)

## Context

This skill is part of the LLM enforcement system. See `docs/superpowers/MASTER_PLAN.md` for the full implementation philosophy.

Remember: A tool that claims to have a feature but has a stub is a broken tool. LLMs trust the toolchain. Trust requires correctness.