//! Database path resolution helper.
//!
//! Centralizes the "--db is optional" logic so query commands
//! fall back to the registry or `.magellan/magellan.db` in cwd.

use anyhow::{Context, Result};
use std::path::PathBuf;

use crate::service::registry::Registry;

/// Resolve database path from explicit argument, registry, or cwd heuristic.
///
/// 1. If `explicit` is `Some`, return it directly.
/// 2. Load the project registry and look for a project whose root matches cwd.
///    If found, return the canonical DB path for that project.
/// 3. Fallback to `.magellan/magellan.db` in the current working directory.
///
/// This is idempotent — repeated calls for the same cwd return the same path.
pub fn resolve_db_path(explicit: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(path) = explicit {
        return Ok(path);
    }

    let registry =
        Registry::load().context("Failed to load project registry for default DB resolution")?;

    let cwd = std::env::current_dir().context("Failed to get current working directory")?;

    if let Some(entry) = registry.find_by_root(&cwd) {
        let canon = Registry::canonical_db_path(&entry.name);
        Registry::ensure_db_dir(&entry.name)?;
        return Ok(canon);
    }

    Ok(PathBuf::from(".magellan/magellan.db"))
}
