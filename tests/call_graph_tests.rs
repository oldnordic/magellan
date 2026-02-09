//! Tests for CALLS edge type (forward call graph)
//!
//! TDD approach: Write failing test first, then implement feature.

use std::path::PathBuf;

#[test]
fn test_extract_calls_detects_function_calls() {
    // Verify that we can extract function call relationships
    // Source: caller() calls helper()
    let source = r#"
fn caller() {
    helper();
}

fn helper() {}
"#;

    let mut parser = magellan::Parser::new().unwrap();
    let symbols = parser.extract_symbols(PathBuf::from("test.rs"), source.as_bytes());

    // Should extract 2 function symbols
    assert_eq!(symbols.len(), 2, "Should have 2 functions");

    let functions: Vec<_> = symbols
        .iter()
        .filter(|s| s.kind == magellan::SymbolKind::Function)
        .collect();
    assert_eq!(functions.len(), 2);

    // NEW: Extract calls (caller → callee)
    let calls = parser.extract_calls(PathBuf::from("test.rs"), source.as_bytes(), &symbols);

    assert_eq!(calls.len(), 1, "Should have 1 call: caller → helper");
    assert_eq!(calls[0].caller, "caller");
    assert_eq!(calls[0].callee, "helper");
}

#[test]
fn test_extract_calls_ignores_type_references() {
    // Verify that type references (scoped_identifier) are NOT treated as calls
    let source = r#"
fn process(data: Data) {
    data.method();
}

struct Data;
impl Data {
    fn method(&self) {}
}
"#;

    let mut parser = magellan::Parser::new().unwrap();
    let symbols = parser.extract_symbols(PathBuf::from("test.rs"), source.as_bytes());

    // Should extract function, method, struct
    assert!(symbols.len() >= 2);

    let calls = parser.extract_calls(PathBuf::from("test.rs"), source.as_bytes(), &symbols);

    // Should have calls (process → method via data.method())
    // But NOT treat "Data" type reference as a call
    assert!(
        calls.iter().any(|c| c.callee == "method"),
        "Should call method"
    );
    assert!(
        !calls.iter().any(|c| c.callee == "Data"),
        "Should not call struct name"
    );
}

#[test]
fn test_code_graph_stores_and_queries_calls_edges() {
    // Test the full CodeGraph API for CALLS edges
    use magellan::CodeGraph;
    use tempfile::TempDir;

    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    let mut graph = CodeGraph::open(&db_path).unwrap();

    let source = r#"
fn main() {
    parse();
    execute();
}

fn parse() {}
fn execute() {
    parse();
}
"#;

    graph.index_file("test.rs", source.as_bytes()).unwrap();

    // NEW: Query calls from main
    let calls_from_main = graph.calls_from_symbol("test.rs", "main").unwrap();

    assert_eq!(calls_from_main.len(), 2, "main should call 2 functions");
    let called_names: Vec<_> = calls_from_main.iter().map(|c| c.callee.as_str()).collect();
    assert!(called_names.contains(&"parse"));
    assert!(called_names.contains(&"execute"));

    // NEW: Query calls from execute
    let calls_from_execute = graph.calls_from_symbol("test.rs", "execute").unwrap();
    assert_eq!(
        calls_from_execute.len(),
        1,
        "execute should call 1 function"
    );
    assert_eq!(calls_from_execute[0].callee, "parse");

    // NEW: Query inbound calls (who calls parse?)
    let calls_to_parse = graph.callers_of_symbol("test.rs", "parse").unwrap();
    assert_eq!(
        calls_to_parse.len(),
        2,
        "parse should be called by 2 functions"
    );
    let caller_names: Vec<_> = calls_to_parse.iter().map(|c| c.caller.as_str()).collect();
    assert!(caller_names.contains(&"main"));
    assert!(caller_names.contains(&"execute"));
}

#[test]
fn test_cross_file_method_calls_are_indexed() {
    use magellan::CodeGraph;
    use tempfile::TempDir;

    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    let mut graph = CodeGraph::open(&db_path).unwrap();

    let library_source = r#"
struct Widget;
impl Widget {
    fn render(&self) {}
}
"#;

    let caller_source = r#"
fn invoke(widget: &Widget) {
    widget.render();
}
"#;

    graph
        .index_file("lib.rs", library_source.as_bytes())
        .unwrap();
    graph
        .index_file("main.rs", caller_source.as_bytes())
        .unwrap();

    let calls_to_render = graph.callers_of_symbol("lib.rs", "render").unwrap();
    assert_eq!(
        calls_to_render.len(),
        1,
        "render should be called from another file"
    );
    assert_eq!(calls_to_render[0].caller, "invoke");
    assert_eq!(calls_to_render[0].callee, "render");
    assert_eq!(calls_to_render[0].file_path, PathBuf::from("main.rs"));
}

#[test]
fn test_extract_calls_handles_nested_calls() {
    // Verify nested function calls are detected
    let source = r#"
fn outer() {
    inner1();
    inner2();
    inner1(); // duplicate call
}

fn inner1() {}
fn inner2() {}
"#;

    let mut parser = magellan::Parser::new().unwrap();
    let symbols = parser.extract_symbols(PathBuf::from("test.rs"), source.as_bytes());

    let calls = parser.extract_calls(PathBuf::from("test.rs"), source.as_bytes(), &symbols);

    assert_eq!(
        calls.len(),
        3,
        "Should have 3 calls (inner1 twice, inner2 once)"
    );

    // Count unique calls
    let unique_calls: std::collections::HashSet<_> =
        calls.iter().map(|c| (&c.caller, &c.callee)).collect();
    assert_eq!(
        unique_calls.len(),
        2,
        "Should have 2 unique caller-callee pairs"
    );
}

#[test]
fn test_cross_file_call_resolution() {
    // Test that calls across file boundaries are correctly indexed and queried
    use magellan::CodeGraph;
    use tempfile::TempDir;

    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let mut graph = CodeGraph::open(&db_path).unwrap();

    // File 1: Define a callee function
    let caller_path = "src/caller.rs";
    let caller_source = r#"
pub fn caller() {
    callee();
}

pub fn another_caller() {
    callee();
}
"#;

    // File 2: Define the callee function
    let callee_path = "src/callee.rs";
    let callee_source = r#"
pub fn callee() {
    println!("Called from another file");
}
"#;

    // Index both files
    graph.index_file(caller_path, caller_source.as_bytes()).unwrap();
    graph.index_file(callee_path, callee_source.as_bytes()).unwrap();

    // Index calls for both files
    graph.index_calls(caller_path, caller_source.as_bytes()).unwrap();
    graph.index_calls(callee_path, callee_source.as_bytes()).unwrap();

    // Verify symbols are indexed
    let caller_symbols = graph.symbols_in_file(caller_path).unwrap();
    assert_eq!(
        caller_symbols.len(),
        2,
        "Should have 2 symbols in caller.rs"
    );

    let callee_symbols = graph.symbols_in_file(callee_path).unwrap();
    assert_eq!(
        callee_symbols.len(),
        1,
        "Should have 1 symbol in callee.rs"
    );

    // Get symbol IDs
    let _caller_id = graph
        .symbol_id_by_name(caller_path, "caller")
        .unwrap()
        .expect("caller symbol should exist");
    let _another_caller_id = graph
        .symbol_id_by_name(caller_path, "another_caller")
        .unwrap()
        .expect("another_caller symbol should exist");
    let _callee_id = graph
        .symbol_id_by_name(callee_path, "callee")
        .unwrap()
        .expect("callee symbol should exist");

    // Query calls FROM caller (outgoing)
    let calls_from_caller = graph.calls_from_symbol(caller_path, "caller").unwrap();
    assert_eq!(
        calls_from_caller.len(),
        1,
        "caller should have 1 outgoing call"
    );
    assert_eq!(
        calls_from_caller[0].callee, "callee",
        "caller should call callee"
    );
    assert_eq!(
        calls_from_caller[0].file_path,
        std::path::PathBuf::from(caller_path),
        "Call should be in caller.rs"
    );

    // Query calls FROM another_caller (outgoing)
    let calls_from_another = graph
        .calls_from_symbol(caller_path, "another_caller")
        .unwrap();
    assert_eq!(
        calls_from_another.len(),
        1,
        "another_caller should have 1 outgoing call"
    );
    assert_eq!(
        calls_from_another[0].callee, "callee",
        "another_caller should call callee"
    );

    // Query calls TO callee (incoming) - this is the key cross-file test
    let calls_to_callee = graph.callers_of_symbol(callee_path, "callee").unwrap();
    assert_eq!(
        calls_to_callee.len(),
        2,
        "callee should have 2 incoming calls from both callers"
    );

    // Verify the incoming calls are from different file
    let callers: Vec<_> = calls_to_callee.iter().map(|c| &c.caller).collect();
    assert!(
        callers.contains(&&"caller".to_string()),
        "callee should be called by caller"
    );
    assert!(
        callers.contains(&&"another_caller".to_string()),
        "callee should be called by another_caller"
    );

    // Verify all incoming calls originate from caller.rs (different file)
    for call in &calls_to_callee {
        assert_eq!(
            call.file_path,
            std::path::PathBuf::from(caller_path),
            "All calls to callee should originate from caller.rs"
        );
    }
}
