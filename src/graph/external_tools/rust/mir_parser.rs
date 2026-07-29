//! Rust MIR text format parser and CFG extraction
//!
//! Parses the human-readable MIR output from `rustup run nightly rustc
//! -Zunpretty=mir` and extracts Control Flow Graph blocks with edges.
//!
//! MIR is the compiler's mid-level IR: post-typecheck, post-desugar, pre-codegen.
//! It exposes the real control flow including macro expansions, `?` operator
//! desugaring (→ `SwitchInt` on `Result::is_ok`), and overflow checks (→ `assert`
//! blocks). This is strictly higher fidelity than tree-sitter syntactic CFG.

use anyhow::Result;
use std::collections::HashMap;

use crate::graph::cfg_edges_extract::{CfgEdge, CfgEdgeType, CfgWithEdges};
use crate::graph::schema::CfgBlock;

/// Parse MIR text content and extract CFG for all functions.
///
/// Returns a map from function name → `CfgWithEdges`.
pub fn extract_cfg_from_mir(mir_content: &str) -> Result<HashMap<String, CfgWithEdges>> {
    let functions = split_into_functions(mir_content);
    let mut result = HashMap::new();

    for (name, body) in functions {
        let cfg = parse_function_cfg(&body)?;
        result.insert(name, cfg);
    }

    Ok(result)
}

/// Split MIR text into (function_name, function_body) pairs.
///
/// MIR functions start with `fn <name>(<params>) -> <ret_type> {` and end
/// with a matching closing `}`.
fn split_into_functions(mir: &str) -> Vec<(String, String)> {
    let mut functions = Vec::new();
    let lines: Vec<&str> = mir.lines().collect();

    let mut i = 0;
    while i < lines.len() {
        let trimmed = lines[i].trim();

        // Match: `fn <name>(...) -> <type> {` or `fn <name>(...) {`
        if let Some(name) = extract_function_name(trimmed) {
            // Collect the body until the matching closing brace.
            let mut depth: i32 = 0;
            let mut body_lines = Vec::new();

            // The opening line may contain `{`
            let opening = trimmed;
            depth += opening.matches('{').count() as i32;
            depth -= opening.matches('}').count() as i32;

            i += 1;
            while i < lines.len() && depth > 0 {
                let line = lines[i];
                depth += line.matches('{').count() as i32;
                depth -= line.matches('}').count() as i32;
                if depth > 0 {
                    body_lines.push(line);
                }
                i += 1;
            }

            functions.push((name, body_lines.join("\n")));
        } else {
            i += 1;
        }
    }

    functions
}

/// Extract the function name from a MIR function signature line.
///
/// Matches patterns like:
/// - `fn demo(_1: i32) -> i32 {`
/// - `fn main() -> () {`
/// - `fn demo(_1: i32) -> i32 {`  (with leading whitespace)
///
/// Excludes lines starting with `//` (comments) and `debug`.
fn extract_function_name(line: &str) -> Option<String> {
    let line = line.trim();

    if !line.starts_with("fn ") {
        return None;
    }

    // Skip the "fn " prefix
    let after_fn = &line[3..];

    // Find the opening paren for params
    let paren_pos = after_fn.find('(')?;
    let name = after_fn[..paren_pos].trim();

    if name.is_empty() {
        return None;
    }

    Some(name.to_string())
}

/// Parse a single function body into a CfgWithEdges.
///
/// The body is the text between the function's opening `{` and closing `}`.
fn parse_function_cfg(body: &str) -> Result<CfgWithEdges> {
    let blocks = parse_blocks(body);

    if blocks.is_empty() {
        // No explicit basic blocks — create a single implicit entry block.
        let implicit = CfgBlock {
            cfg_hash: None,
            statements: Some(vec!["implicit block (no control flow)".to_string()]),
            function_id: 0,
            kind: "Entry".to_string(),
            terminator: "Return".to_string(),
            byte_start: 0,
            byte_end: 0,
            start_line: 0,
            start_col: 0,
            end_line: 0,
            end_col: 0,
            cfg_condition: None,
        };
        return Ok(CfgWithEdges {
            blocks: vec![implicit],
            edges: vec![],
            function_id: 0,
        });
    }

    // Build a name→index map for edge resolution.
    let name_to_idx: HashMap<&str, usize> = blocks
        .iter()
        .enumerate()
        .map(|(i, b)| (b.name.as_str(), i))
        .collect();

    // Parse edges from terminators.
    let mut edges = Vec::new();
    for (idx, block) in blocks.iter().enumerate() {
        for target in parse_terminator_targets(&block.terminator) {
            if let Some(&target_idx) = name_to_idx.get(target.as_str()) {
                let edge_type = classify_edge(&block.terminator, &target);
                edges.push(CfgEdge {
                    source_idx: idx,
                    target_idx,
                    edge_type,
                });
            }
        }
    }

    // Convert MirBlock structs to CfgBlock.
    let cfg_blocks: Vec<CfgBlock> = blocks
        .iter()
        .enumerate()
        .map(|(i, b)| {
            let kind = if i == 0 { "Entry" } else { "Normal" };
            let terminator_kind = classify_terminator(&b.terminator);
            CfgBlock {
                cfg_hash: None,
                statements: Some(b.statements.clone()),
                function_id: 0,
                kind: kind.to_string(),
                terminator: terminator_kind.to_string(),
                byte_start: (i * 1000) as u64,
                byte_end: (i * 1000 + 999) as u64,
                start_line: 0,
                start_col: 0,
                end_line: 0,
                end_col: 0,
                cfg_condition: None,
            }
        })
        .collect();

    Ok(CfgWithEdges {
        blocks: cfg_blocks,
        edges,
        function_id: 0,
    })
}

/// A parsed MIR basic block with its terminator statement.
struct MirBlock {
    name: String,
    statements: Vec<String>,
    terminator: String,
}

/// Parse basic blocks from a function body.
///
/// Blocks are delimited by `bb<N>: {` ... `}`. The last non-empty statement
/// before the closing `}` is the terminator.
fn parse_blocks(body: &str) -> Vec<MirBlock> {
    let lines: Vec<&str> = body.lines().collect();
    let mut blocks = Vec::new();
    let mut current_name: Option<String> = None;
    let mut current_stmts: Vec<String> = Vec::new();
    let mut current_terminator: Option<String> = None;

    for line in lines {
        let trimmed = line.trim();

        // Skip empty lines and debug declarations
        if trimmed.is_empty() || trimmed.starts_with("debug ") || trimmed.starts_with("let ") {
            continue;
        }

        // Check for block start: `bb<N>: {` or `bb<N>: {` with content on same line
        if let Some(block_name) = parse_block_label(trimmed) {
            // Finalize previous block
            if let Some(name) = current_name.take() {
                let term = current_terminator.unwrap_or_else(|| "Unknown".to_string());
                blocks.push(MirBlock {
                    name,
                    statements: std::mem::take(&mut current_stmts),
                    terminator: term,
                });
            }
            current_name = Some(block_name);
            current_stmts.clear();
            current_terminator = None;
            continue;
        }

        // Skip the closing brace
        if trimmed == "}" {
            continue;
        }

        // Accumulate statements
        let stmt = trimmed.trim_end_matches(';').to_string();
        if stmt.is_empty() {
            continue;
        }

        current_stmts.push(stmt.clone());

        // The terminator is the last statement that matches a known terminator pattern
        if is_terminator(&stmt) {
            current_terminator = Some(stmt);
        }
    }

    // Finalize last block
    if let Some(name) = current_name.take() {
        let term = current_terminator.unwrap_or_else(|| "Unknown".to_string());
        blocks.push(MirBlock {
            name,
            statements: current_stmts,
            terminator: term,
        });
    }

    blocks
}

/// Parse a block label line like `bb0: {` → returns `Some("bb0")`.
fn parse_block_label(line: &str) -> Option<String> {
    if !line.starts_with("bb") {
        return None;
    }

    // Find the colon
    let colon_pos = line.find(':')?;
    let label = &line[..colon_pos];

    // Verify it's a valid bb label (bb followed by digits)
    if label.len() < 3 || !label[2..].chars().all(|c| c.is_ascii_digit()) {
        return None;
    }

    Some(label.to_string())
}

/// Check if a statement is a MIR terminator.
///
/// MIR terminators are: `goto`, `switchInt`, `return`, `assert`, `Drop`/`drop`,
/// `call`, `Unreachable`/`unreachable`, `resume`, `tailCall`, plus calls in
/// assignment form (`_5 = path::func(args) -> [return: bb1, unwind continue]`).
fn is_terminator(stmt: &str) -> bool {
    stmt.starts_with("goto")
        || stmt.starts_with("switchInt")
        || stmt.starts_with("return")
        || stmt.starts_with("assert")
        || stmt.starts_with("Drop")
        || stmt.starts_with("drop")
        || stmt.starts_with("call")
        || stmt.starts_with("Unreachable")
        || stmt.starts_with("unreachable")
        || stmt.starts_with("resume")
        || stmt.starts_with("tailCall")
        || is_call_assignment(stmt)
}

/// Extract target block names from a terminator statement.
///
/// Returns a list of block names (e.g. `["bb1", "bb3"]`).
fn parse_terminator_targets(terminator: &str) -> Vec<String> {
    let mut targets = Vec::new();

    // Find all `bb<N>` references in the terminator.
    // MIR terminators reference blocks as `-> bbX` or `-> [val: bbX, otherwise: bbY]`.
    let mut search_pos = 0;
    while let Some(bb_pos) = terminator[search_pos..].find("bb") {
        let abs_pos = search_pos + bb_pos;
        let rest = &terminator[abs_pos..];

        // Try to parse `bb<N>`
        if let Some(name) = parse_bb_ref(rest) {
            if !targets.contains(&name) {
                targets.push(name);
            }
            search_pos = abs_pos + 2;
        } else {
            search_pos = abs_pos + 2;
        }
    }

    targets
}

/// Parse a `bb<N>` reference from the start of a string.
fn parse_bb_ref(s: &str) -> Option<String> {
    if !s.starts_with("bb") || s.len() < 3 {
        return None;
    }

    let mut end = 2;
    while end < s.len() && s.as_bytes()[end].is_ascii_digit() {
        end += 1;
    }

    if end == 2 {
        return None;
    }

    Some(s[..end].to_string())
}

/// Extract the `otherwise` target of a switchInt terminator, e.g.
/// `switchInt(move _5) -> [0: bb5, 1: bb6, otherwise: bb4]` → `bb4`.
/// Newer rustc also emits `-> otherwise` (no explicit arms); that form has
/// no bb reference after `otherwise:` and yields None here.
fn parse_otherwise_target(terminator: &str) -> Option<String> {
    let pos = terminator.find("otherwise:")?;
    parse_bb_ref(terminator[pos + "otherwise:".len()..].trim_start())
}

/// Classify the edge type based on the terminator and target.
fn classify_edge(terminator: &str, target: &str) -> CfgEdgeType {
    if terminator.starts_with("goto") {
        CfgEdgeType::Jump
    } else if terminator.starts_with("switchInt") {
        // switchInt edges are conditional — the "otherwise" target is the
        // fallthrough/default case, others are conditional branches.
        if parse_otherwise_target(terminator).as_deref() == Some(target) {
            CfgEdgeType::ConditionalFalse
        } else {
            CfgEdgeType::ConditionalTrue
        }
    } else if terminator.starts_with("assert")
        || terminator.starts_with("Drop")
        || terminator.starts_with("drop")
    {
        CfgEdgeType::Jump
    } else if terminator.starts_with("call") || is_call_assignment(terminator) {
        CfgEdgeType::Call
    } else if terminator.starts_with("return") {
        CfgEdgeType::Return
    } else {
        CfgEdgeType::Fallthrough
    }
}

/// Classify a terminator into a human-readable kind string.
fn classify_terminator(terminator: &str) -> &'static str {
    if terminator.starts_with("goto") {
        "Jump"
    } else if terminator.starts_with("switchInt") {
        "SwitchInt"
    } else if terminator.starts_with("return") {
        "Return"
    } else if terminator.starts_with("assert") {
        "Assert"
    } else if terminator.starts_with("Drop") || terminator.starts_with("drop") {
        "Drop"
    } else if terminator.starts_with("call") || is_call_assignment(terminator) {
        "Call"
    } else if terminator.starts_with("unreachable") || terminator.starts_with("Unreachable") {
        "Unreachable"
    } else if terminator.starts_with("resume") {
        "Resume"
    } else {
        "Unknown"
    }
}

/// MIR call terminators appear in assignment form:
/// `_5 = core::str::<impl str>::trim(copy _1) -> [return: bb1, unwind continue]`.
/// Every other MIR terminator kind starts with its keyword, so `= ` plus a
/// basic-block target (`-> [` or `-> bb`) identifies the call form. The target
/// requirement keeps closures (`|x| -> i32`) from false-matching.
fn is_call_assignment(terminator: &str) -> bool {
    terminator.contains(" = ") && (terminator.contains("-> [") || terminator.contains("-> bb"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verified MIR output from `rustup run nightly rustc -Zunpretty=mir`
    /// on a function with an if/else branch and overflow checks.
    const SAMPLE_MIR: &str = r#"// WARNING: This output format is intended for human consumers only
// and is subject to change without notice. Knock yourself out.
fn main() -> () {
    let mut _0: ();

    bb0: {
        return;
    }
}

fn demo(_1: i32) -> i32 {
    debug x => _1;
    let mut _0: i32;
    let mut _2: bool;
    let mut _3: (i32, bool);
    let mut _4: (i32, bool);

    bb0: {
        _2 = Gt(copy _1, const 0_i32);
        switchInt(move _2) -> [0: bb3, otherwise: bb1];
    }

    bb1: {
        _3 = AddWithOverflow(copy _1, const 1_i32);
        assert(!move (_3.1: bool), "attempt to compute `{} + {}`, which would overflow", copy _1, const 1_i32) -> [success: bb2, unwind continue];
    }

    bb2: {
        _0 = move (_3.0: i32);
        goto -> bb5;
    }

    bb3: {
        _4 = SubWithOverflow(copy _1, const 1_i32);
        assert(!move (_4.1: bool), "attempt to compute `{} - {}`, which would overflow", copy _1, const 1_i32) -> [success: bb4, unwind continue];
    }

    bb4: {
        _0 = move (_4.0: i32);
        goto -> bb5;
    }

    bb5: {
        return;
    }
}
"#;

    #[test]
    fn test_extract_function_name() {
        assert_eq!(
            extract_function_name("fn demo(_1: i32) -> i32 {"),
            Some("demo".to_string())
        );
        assert_eq!(
            extract_function_name("fn main() -> () {"),
            Some("main".to_string())
        );
        assert_eq!(extract_function_name("// not a function"), None);
        assert_eq!(extract_function_name("debug x => _1;"), None);
    }

    #[test]
    fn test_parse_block_label() {
        assert_eq!(parse_block_label("bb0: {"), Some("bb0".to_string()));
        assert_eq!(parse_block_label("bb12: {"), Some("bb12".to_string()));
        assert_eq!(parse_block_label("bb: {"), None);
        assert_eq!(parse_block_label("not_a_block"), None);
    }

    #[test]
    fn test_parse_terminator_targets_goto() {
        let targets = parse_terminator_targets("goto -> bb5");
        assert_eq!(targets, vec!["bb5"]);
    }

    #[test]
    fn test_parse_terminator_targets_switchint() {
        let targets = parse_terminator_targets("switchInt(move _2) -> [0: bb3, otherwise: bb1]");
        assert!(targets.contains(&"bb3".to_string()));
        assert!(targets.contains(&"bb1".to_string()));
        assert_eq!(targets.len(), 2);
    }

    #[test]
    fn test_switchint_otherwise_edge_is_conditional_false() {
        let term = "switchInt(move _5) -> [0: bb5, 1: bb6, otherwise: bb4]";
        assert_eq!(
            parse_otherwise_target(term).as_deref(),
            Some("bb4"),
            "otherwise target must be parsed"
        );
        assert_eq!(
            classify_edge(term, "bb4"),
            CfgEdgeType::ConditionalFalse,
            "otherwise arm is the default/fallthrough edge"
        );
        assert_eq!(classify_edge(term, "bb5"), CfgEdgeType::ConditionalTrue);
        assert_eq!(classify_edge(term, "bb6"), CfgEdgeType::ConditionalTrue);
    }

    #[test]
    fn test_classify_call_assignment_and_lowercase_forms() {
        // Modern nightly MIR emits calls in assignment form and lowercase
        // drop/unreachable — all must classify, not fall through to Unknown.
        let call = "_5 = core::str::<impl str>::trim(copy _1) -> [return: bb1, unwind continue]";
        assert!(is_call_assignment(call));
        assert!(is_terminator(call));
        assert!(!is_terminator("_3 = |x: i32| -> i32 { x };"));
        assert_eq!(classify_terminator(call), "Call");
        assert_eq!(classify_edge(call, "bb1"), CfgEdgeType::Call);
        assert!(is_terminator("unreachable;"));
        assert_eq!(classify_terminator("unreachable;"), "Unreachable");
        assert_eq!(
            classify_terminator("drop(_2) -> [return: bb3, unwind: bb4]"),
            "Drop"
        );
        assert_eq!(
            classify_edge("drop(_2) -> [return: bb3, unwind: bb4]", "bb3"),
            CfgEdgeType::Jump
        );
    }

    #[test]
    fn test_parse_terminator_targets_assert() {
        let targets = parse_terminator_targets(
            "assert(!move (_3.1: bool), ...) -> [success: bb2, unwind continue]",
        );
        assert_eq!(targets, vec!["bb2"]);
    }

    #[test]
    fn test_is_terminator() {
        assert!(is_terminator("goto -> bb5"));
        assert!(is_terminator("switchInt(move _2) -> [0: bb3]"));
        assert!(is_terminator("return"));
        assert!(is_terminator(
            "assert(!move (_3.1: bool)) -> [success: bb2]"
        ));
        assert!(!is_terminator("_2 = Gt(copy _1, const 0_i32)"));
    }

    #[test]
    fn test_extract_cfg_from_mir_finds_two_functions() {
        let result = extract_cfg_from_mir(SAMPLE_MIR).unwrap();
        assert!(result.contains_key("main"));
        assert!(result.contains_key("demo"));
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_main_function_has_one_block() {
        let result = extract_cfg_from_mir(SAMPLE_MIR).unwrap();
        let main_cfg = &result["main"];
        assert_eq!(
            main_cfg.blocks.len(),
            1,
            "main should have 1 block (bb0 only)"
        );
        assert_eq!(main_cfg.edges.len(), 0, "main has no edges (just return)");
    }

    #[test]
    fn test_demo_function_has_six_blocks() {
        let result = extract_cfg_from_mir(SAMPLE_MIR).unwrap();
        let demo_cfg = &result["demo"];
        assert_eq!(
            demo_cfg.blocks.len(),
            6,
            "demo should have 6 blocks (bb0-bb5)"
        );
    }

    #[test]
    fn test_demo_function_has_correct_edges() {
        let result = extract_cfg_from_mir(SAMPLE_MIR).unwrap();
        let demo_cfg = &result["demo"];

        // Expected edges:
        // bb0 --switchInt--> bb1 (otherwise)
        // bb0 --switchInt--> bb3 (case 0)
        // bb1 --assert--> bb2 (success)
        // bb2 --goto--> bb5
        // bb3 --assert--> bb4 (success)
        // bb4 --goto--> bb5
        assert_eq!(
            demo_cfg.edges.len(),
            6,
            "demo should have 6 edges. Got: {:?}",
            demo_cfg.edges
        );

        // Collect all edges as (source, target) pairs.
        let edge_pairs: Vec<(usize, usize)> = demo_cfg
            .edges
            .iter()
            .map(|e| (e.source_idx, e.target_idx))
            .collect();

        // bb0 (idx 0) → bb1 (idx 1)
        assert!(
            edge_pairs.contains(&(0, 1)),
            "missing edge bb0→bb1 (idx 0→1). Got: {:?}",
            edge_pairs
        );
        // bb0 (idx 0) → bb3 (idx 3)
        assert!(
            edge_pairs.contains(&(0, 3)),
            "missing edge bb0→bb3 (idx 0→3). Got: {:?}",
            edge_pairs
        );
        // bb1 (idx 1) → bb2 (idx 2)
        assert!(
            edge_pairs.contains(&(1, 2)),
            "missing edge bb1→bb2 (idx 1→2). Got: {:?}",
            edge_pairs
        );
        // bb2 (idx 2) → bb5 (idx 5)
        assert!(
            edge_pairs.contains(&(2, 5)),
            "missing edge bb2→bb5 (idx 2→5). Got: {:?}",
            edge_pairs
        );
        // bb3 (idx 3) → bb4 (idx 4)
        assert!(
            edge_pairs.contains(&(3, 4)),
            "missing edge bb3→bb4 (idx 3→4). Got: {:?}",
            edge_pairs
        );
        // bb4 (idx 4) → bb5 (idx 5)
        assert!(
            edge_pairs.contains(&(4, 5)),
            "missing edge bb4→bb5 (idx 4→5). Got: {:?}",
            edge_pairs
        );
    }
}
