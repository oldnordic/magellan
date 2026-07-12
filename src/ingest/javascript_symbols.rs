use crate::common::safe_slice;
use crate::graph::canonical_fqn::FqnBuilder;
use crate::ingest::{ScopeSeparator, ScopeStack, SymbolFact, SymbolKind};
use std::path::{Path, PathBuf};

pub(crate) fn extract_name(node: &tree_sitter::Node, source: &[u8]) -> Option<String> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "identifier" | "property_identifier" => {
                let name_bytes = safe_slice(source, child.start_byte(), child.end_byte())?;
                return std::str::from_utf8(name_bytes).ok().map(|s| s.to_string());
            }
            _ => {}
        }
    }

    None
}

pub(crate) fn extract_function_name(node: &tree_sitter::Node, source: &[u8]) -> Option<String> {
    extract_name(node, source)
}

fn extract_symbol_with_fqn(
    node: &tree_sitter::Node,
    source: &[u8],
    file_path: &Path,
    scope_stack: &ScopeStack,
    package_name: &str,
) -> Option<SymbolFact> {
    let kind = node.kind();

    let symbol_kind = match kind {
        "function_declaration" => SymbolKind::Function,
        "method_definition" => SymbolKind::Method,
        "class_declaration" => SymbolKind::Class,
        _ => return None,
    };

    let name = extract_name(node, source)?;
    let normalized_kind = symbol_kind.normalized_key().to_string();
    let fqn = scope_stack.fqn_for_symbol(&name);

    let builder = FqnBuilder::new(
        package_name.to_string(),
        file_path.to_string_lossy().to_string(),
        ScopeSeparator::Dot,
    );
    let canonical_fqn = builder.canonical(scope_stack, symbol_kind.clone(), &name);
    let display_fqn = builder.display(scope_stack, symbol_kind.clone(), &name);

    Some(SymbolFact {
        file_path: file_path.to_path_buf(),
        kind: symbol_kind,
        kind_normalized: normalized_kind,
        name: Some(name),
        fqn: Some(fqn),
        canonical_fqn: Some(canonical_fqn),
        display_fqn: Some(display_fqn),
        byte_start: node.start_byte(),
        byte_end: node.end_byte(),
        start_line: node.start_position().row + 1,
        start_col: node.start_position().column,
        end_line: node.end_position().row + 1,
        end_col: node.end_position().column,
    })
}

fn walk_tree_with_scope(
    node: &tree_sitter::Node,
    source: &[u8],
    file_path: &Path,
    facts: &mut Vec<SymbolFact>,
    scope_stack: &mut ScopeStack,
    package_name: &str,
) {
    let kind = node.kind();

    if kind == "export_statement" {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            walk_tree_with_scope(&child, source, file_path, facts, scope_stack, package_name);
        }
        return;
    }

    if kind == "class_declaration" {
        if let Some(name) = extract_name(node, source) {
            if let Some(fact) =
                extract_symbol_with_fqn(node, source, file_path, scope_stack, package_name)
            {
                facts.push(fact);
            }
            scope_stack.push(&name);
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                walk_tree_with_scope(&child, source, file_path, facts, scope_stack, package_name);
            }
            scope_stack.pop();
            return;
        }
    }

    if let Some(fact) = extract_symbol_with_fqn(node, source, file_path, scope_stack, package_name)
    {
        facts.push(fact);
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_tree_with_scope(&child, source, file_path, facts, scope_stack, package_name);
    }
}

pub(crate) fn build_symbol_facts_from_tree(
    root_node: &tree_sitter::Node,
    file_path: PathBuf,
    source: &[u8],
) -> Vec<SymbolFact> {
    let mut facts = Vec::new();
    let mut scope_stack = ScopeStack::new(ScopeSeparator::Dot);
    let package_name = ".";

    walk_tree_with_scope(
        root_node,
        source,
        file_path.as_path(),
        &mut facts,
        &mut scope_stack,
        package_name,
    );

    facts
}
