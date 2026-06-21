//! Pre-commit hook installation
//!
//! Installs git pre-commit hooks that check blast scores for changed symbols.

use anyhow::Result;
use anyhow::anyhow;
use magellan::common::find_repo_root;
use std::fs;
use std::os::unix::fs::PermissionsExt;

/// Install pre-commit hook for blast-score checks
///
/// # Arguments
/// * `threshold` - Blast score threshold (default: 10.0)
/// * `strict` - If true, block commits exceeding threshold; if false, warn only
///
/// # Returns
/// Result indicating success or failure
pub fn run_install_hook(threshold: f64, strict: bool) -> Result<()> {
    let repo_root = find_repo_root(&std::env::current_dir()?)
        .ok_or_else(|| anyhow!("Not in a git repository"))?;

    let hooks_dir = repo_root.join(".git").join("hooks");
    fs::create_dir_all(&hooks_dir)?;

    let hook_path = hooks_dir.join("pre-commit");

    let hook_script = format!(
        r#"#!/bin/bash
# Magellan blast-score pre-commit hook
# Threshold: {:.1} (strict: {})

set -e

STAGED_FILES=$(git diff --cached --name-only --diff-filter=ACM)

if [ -z "$STAGED_FILES" ]; then
    exit 0
fi

# Find affected symbols in staged files
for FILE in $STAGED_FILES; do
    # Extract symbols from file (simplified - real version needs AST)
    SYMBOLS=$(magellan query --db ~/.magellan/magellan/magellan.db --file "$FILE" --output json 2>/dev/null | jq -r '.symbols[].name' 2>/dev/null || echo "")

    if [ -z "$SYMBOLS" ]; then
        continue
    fi

    for SYMBOL in $SYMBOLS; do
        SCORE=$(magellan blast-score --db ~/.magellan/magellan/magellan.db --symbol "$SYMBOL" --output json 2>/dev/null | jq -r '.data.score' 2>/dev/null || echo "0")

        if (( $(echo "$SCORE > {:.1}" | bc -l 2>/dev/null) )); then
            echo "WARNING: '$SYMBOL' has blast score $SCORE (threshold: {:.1})"
            echo "Run 'magellan context impact --symbol $SYMBOL' for full analysis"

            if [ "{}" = "true" ]; then
                echo "Blocking commit due to strict mode"
                exit 1
            fi
        fi
    done
done

exit 0
"#,
        threshold, strict, threshold, threshold, strict
    );

    fs::write(&hook_path, hook_script)?;

    // Make executable
    let mut perms = fs::metadata(&hook_path)?.permissions();
    perms.set_mode(perms.mode() | 0o111);
    fs::set_permissions(&hook_path, perms)?;

    println!("Installed pre-commit hook (threshold: {:.1}, strict: {})", threshold, strict);
    println!("Hook location: {}", hook_path.display());

    Ok(())
}
