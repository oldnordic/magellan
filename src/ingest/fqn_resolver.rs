//! FQN resolver for qualified identifier → symbol facts.
//!
//! Provides best-effort resolution of multi-part identifiers such as
//! `math::add`, `pkg.Func`, `Class::method`, `obj.method()`.
//!
//! Pure function: no side effects, no filesystem access.

use crate::ingest::SymbolFact;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub fn build_fqn_map(symbols: &[SymbolFact]) -> HashMap<String, &SymbolFact> {
    let mut map = HashMap::new();
    for symbol in symbols {
        for key in [
            symbol.name.as_deref(),
            symbol.fqn.as_deref(),
            symbol.canonical_fqn.as_deref(),
            symbol.display_fqn.as_deref(),
        ]
        .into_iter()
        .flatten()
        {
            map.entry(key.to_string()).or_insert(symbol);
        }
    }
    map
}

/// Resolve a possibly-qualified identifier text to a `SymbolFact`.
///
/// Resolution order:
/// 1. Exact FQN in `symbols_by_fqn` (e.g. `math::add`, `pkg.Func`).
/// 2. Common Rust crate-relative prefixes (`crate::math::add`, `super::math::add`).
/// 3. Bare last component against `fallback_symbols`, preferring a symbol that
///    appears in `current_file` when the bare name is ambiguous.
///
/// # Arguments
/// * `text` - Full source text of the identifier/call node.
/// * `node_kind` - Tree-sitter node kind.
/// * `current_file` - The file containing the reference (for disambiguation).
/// * `symbols_by_fqn` - Map of fully-qualified names to symbol facts.
/// * `fallback_symbols` - All candidate symbols (used for bare-name fallback).
///
/// # Returns
/// The best matching symbol fact, if any.
pub fn resolve_qualified_symbol<'a>(
    text: &str,
    node_kind: &str,
    current_file: &Path,
    symbols_by_fqn: &HashMap<String, &'a SymbolFact>,
    fallback_symbols: &[&'a SymbolFact],
) -> Option<&'a SymbolFact> {
    // Normalize separators across languages.
    let normalized_text = match node_kind {
        "selector_expression" | "member_expression" | "attribute" => text,
        "scoped_identifier" => text,
        "qualified_identifier" => text,
        "qualified_name" => text,
        _ => text,
    };

    // 1. Exact FQN match.
    if let Some(fact) = symbols_by_fqn.get(normalized_text) {
        return Some(*fact);
    }

    // 2. Rust crate-relative prefixes (cheap heuristic).
    for prefix in ["crate::", "super::"] {
        let prefixed = format!("{}{}", prefix, normalized_text);
        if let Some(fact) = symbols_by_fqn.get(&prefixed) {
            return Some(*fact);
        }
    }

    // 3. Bare last component fallback.
    let separator = if normalized_text.contains("::") {
        "::"
    } else if normalized_text.contains('.') {
        "."
    } else {
        return None;
    };

    let last = normalized_text
        .rsplit(separator)
        .next()
        .unwrap_or(normalized_text);

    let matches: Vec<&SymbolFact> = fallback_symbols
        .iter()
        .filter(|s| s.name.as_deref() == Some(last))
        .copied()
        .collect();

    if matches.is_empty() {
        return None;
    }

    // Prefer symbols defined in the current file to reduce cross-file false positives.
    let current_file_pb = PathBuf::from(current_file);
    matches
        .iter()
        .find(|s| s.file_path == current_file_pb)
        .copied()
        .or_else(|| Some(matches[0]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ingest::{SymbolFact, SymbolKind};

    fn fact(name: &str, file: &str, kind: SymbolKind) -> SymbolFact {
        let kind_normalized = match kind {
            SymbolKind::Function => "fn".to_string(),
            SymbolKind::Class => "struct".to_string(),
            _ => "unknown".to_string(),
        };
        SymbolFact {
            name: Some(name.to_string()),
            kind,
            kind_normalized,
            file_path: PathBuf::from(file),
            byte_start: 0,
            byte_end: 1,
            start_line: 1,
            start_col: 0,
            end_line: 1,
            end_col: 1,
            fqn: None,
            canonical_fqn: None,
            display_fqn: None,
        }
    }

    #[test]
    fn test_exact_fqn_match() {
        let add = fact("add", "math.rs", SymbolKind::Function);
        let map: HashMap<String, &SymbolFact> =
            [("math::add".to_string(), &add)].into_iter().collect();
        assert_eq!(
            resolve_qualified_symbol(
                "math::add",
                "scoped_identifier",
                Path::new("main.rs"),
                &map,
                &[]
            )
            .map(|s| s.file_path.as_path()),
            Some(Path::new("math.rs"))
        );
    }

    #[test]
    fn test_bare_last_component_prefers_current_file() {
        let local_add = fact("add", "main.rs", SymbolKind::Function);
        let remote_add = fact("add", "math.rs", SymbolKind::Function);
        let map = HashMap::new();
        let fallback = vec![&local_add, &remote_add];
        let resolved = resolve_qualified_symbol(
            "math::add",
            "scoped_identifier",
            Path::new("main.rs"),
            &map,
            &fallback,
        )
        .unwrap();
        // Even though the qualifier says "math", we have no FQN data so we
        // prefer the local symbol. This is the conservative fallback.
        assert_eq!(resolved.file_path, PathBuf::from("main.rs"));
    }
}
