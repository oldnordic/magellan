use crate::common::safe_slice;
use crate::graph::canonical_fqn::FqnBuilder;
use crate::ingest::{ScopeSeparator, ScopeStack, SymbolFact, SymbolKind};
use std::path::{Path, PathBuf};

pub(crate) fn extract_name(
    node: &tree_sitter::Node,
    source: &[u8],
    node_kind: &str,
) -> Option<String> {
    if node_kind == "namespace_definition" {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "namespace_identifier" {
                let name_bytes = safe_slice(source, child.start_byte(), child.end_byte())?;
                return std::str::from_utf8(name_bytes).ok().map(|s| s.to_string());
            }
        }
        return None;
    }

    find_name_recursive(node, source)
}

pub(crate) fn find_name_recursive(node: &tree_sitter::Node, source: &[u8]) -> Option<String> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "identifier" | "type_identifier" => {
                let name_bytes = safe_slice(source, child.start_byte(), child.end_byte())?;
                return std::str::from_utf8(name_bytes).ok().map(|s| s.to_string());
            }
            "scoped_identifier" | "qualified_identifier" => {
                if let Some(name_node) = child.child_by_field_name("name") {
                    if name_node.kind() == "identifier" || name_node.kind() == "type_identifier" {
                        let name_bytes =
                            safe_slice(source, name_node.start_byte(), name_node.end_byte())?;
                        if let Ok(s) = std::str::from_utf8(name_bytes) {
                            return Some(s.to_string());
                        }
                    }
                }
            }
            "function_declarator"
            | "parameter_list"
            | "field_declaration_list"
            | "template_parameter_list" => {
                if let Some(name) = find_name_recursive(&child, source) {
                    return Some(name);
                }
            }
            _ => {}
        }
    }

    None
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
        "function_definition" => SymbolKind::Function,
        "class_specifier" => SymbolKind::Class,
        "struct_specifier" => SymbolKind::Class,
        "namespace_definition" => SymbolKind::Namespace,
        _ => return None,
    };

    let name = extract_name(node, source, kind)?;
    let normalized_kind = symbol_kind.normalized_key().to_string();
    let fqn = scope_stack.fqn_for_symbol(&name);

    let builder = FqnBuilder::new(
        package_name.to_string(),
        file_path.to_string_lossy().to_string(),
        ScopeSeparator::DoubleColon,
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

    if kind == "template_declaration" {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            walk_tree_with_scope(&child, source, file_path, facts, scope_stack, package_name);
        }
        return;
    }

    if kind == "namespace_definition" {
        if let Some(name) = extract_name(node, source, kind) {
            if !name.is_empty() {
                if let Some(fact) =
                    extract_symbol_with_fqn(node, source, file_path, scope_stack, package_name)
                {
                    facts.push(fact);
                }
                scope_stack.push(&name);
            }

            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                walk_tree_with_scope(&child, source, file_path, facts, scope_stack, package_name);
            }

            if !name.is_empty() {
                scope_stack.pop();
            }
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
    let mut scope_stack = ScopeStack::new(ScopeSeparator::DoubleColon);
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
