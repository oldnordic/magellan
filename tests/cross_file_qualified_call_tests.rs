#[test]
#[allow(deprecated)]
fn test_cross_file_qualified_call_resolution() {
    // Mirrors the real bug: caller.rs calls math::add() where math.rs defines add().
    use magellan::CodeGraph;
    use tempfile::TempDir;

    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let mut graph = CodeGraph::open(&db_path).unwrap();

    let math_source = r#"
pub fn add(a: i32, b: i32) -> i32 {
    a + b
}
"#;

    let main_source = r#"
fn main() {
    let _ = math::add(1, 2);
}
"#;

    graph.index_file("math.rs", math_source.as_bytes()).unwrap();
    graph.index_file("main.rs", main_source.as_bytes()).unwrap();

    // index_file already indexes calls; do not double-index.

    // main should call math::add
    let calls_from_main = graph.calls_from_symbol("main.rs", "main").unwrap();
    assert_eq!(
        calls_from_main.len(),
        1,
        "main should have 1 outgoing call to add, got {:?}",
        calls_from_main
    );
    assert_eq!(calls_from_main[0].callee, "add");

    // add should be called from main.rs
    let calls_to_add = graph.callers_of_symbol("math.rs", "add").unwrap();
    assert_eq!(
        calls_to_add.len(),
        1,
        "add should have 1 incoming call from main, got {:?}",
        calls_to_add
    );
    assert_eq!(calls_to_add[0].caller, "main");
    assert_eq!(
        calls_to_add[0].file_path,
        std::path::PathBuf::from("main.rs")
    );
}

#[test]
#[allow(deprecated)]
fn test_cross_file_qualified_reference_resolution() {
    // References should also follow qualified identifiers across files.
    use magellan::CodeGraph;
    use tempfile::TempDir;

    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let mut graph = CodeGraph::open(&db_path).unwrap();

    let math_source = r#"
pub fn add(a: i32, b: i32) -> i32 {
    a + b
}
"#;

    let main_source = r#"
fn main() {
    let _ = math::add(1, 2);
}
"#;

    graph.index_file("math.rs", math_source.as_bytes()).unwrap();
    graph.index_file("main.rs", main_source.as_bytes()).unwrap();

    // Index references for both files
    graph
        .index_references("math.rs", math_source.as_bytes())
        .unwrap();
    graph
        .index_references("main.rs", main_source.as_bytes())
        .unwrap();

    let add_symbol = graph
        .symbol_id_by_name("math.rs", "add")
        .unwrap()
        .expect("add symbol should exist");
    let refs = graph.references_to_symbol(add_symbol).unwrap_or_default();
    assert!(
        refs.iter().any(|r| r.file_path == *"main.rs"),
        "add should have a cross-file reference from main.rs, got {:?}",
        refs
    );
}

#[test]
#[allow(deprecated)]
fn test_cross_file_qualified_call_resolution_with_module_path() {
    // Crate-relative path: main.rs calls crate::math::add().
    use magellan::CodeGraph;
    use tempfile::TempDir;

    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let mut graph = CodeGraph::open(&db_path).unwrap();

    let math_source = r#"
pub fn add(a: i32, b: i32) -> i32 {
    a + b
}
"#;

    let main_source = r#"
fn main() {
    let _ = crate::math::add(1, 2);
}
"#;

    graph.index_file("math.rs", math_source.as_bytes()).unwrap();
    graph.index_file("main.rs", main_source.as_bytes()).unwrap();

    graph
        .index_calls("math.rs", math_source.as_bytes())
        .unwrap();
    graph
        .index_calls("main.rs", main_source.as_bytes())
        .unwrap();

    let calls_from_main = graph.calls_from_symbol("main.rs", "main").unwrap();
    assert!(
        calls_from_main.iter().any(|c| c.callee == "add"),
        "main should call crate::math::add, got {:?}",
        calls_from_main
    );
}

#[test]
#[allow(deprecated)]
fn test_qualified_call_in_same_file() {
    // Sanity check: a::foo() in a single file still works.
    use magellan::CodeGraph;
    use tempfile::TempDir;

    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let mut graph = CodeGraph::open(&db_path).unwrap();

    let source = r#"
mod a {
    pub fn foo() {}
}

fn bar() {
    a::foo();
}
"#;

    graph.index_file("test.rs", source.as_bytes()).unwrap();
    graph.index_calls("test.rs", source.as_bytes()).unwrap();

    let calls_from_bar = graph.calls_from_symbol("test.rs", "bar").unwrap();
    assert!(
        calls_from_bar.iter().any(|c| c.callee == "foo"),
        "bar should call a::foo, got {:?}",
        calls_from_bar
    );
}
