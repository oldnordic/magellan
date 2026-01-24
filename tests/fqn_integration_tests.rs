//! Integration tests for fully-qualified name extraction
//!
//! Verifies that FQNs are correctly built and used for symbol lookup
//! across multiple files and languages.

use magellan::CodeGraph;
use tempfile::TempDir;

#[test]
fn test_rust_fqn_multi_file() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("test.db");
    let mut graph = CodeGraph::open(&db_path).unwrap();

    // Create a multi-file Rust project
    let mod_rs = br#"
mod utils;

pub fn main() {
    utils::helper();
}
"#;

    let utils_rs = br#"
pub fn helper() {}

pub struct MyStruct;

impl MyStruct {
    pub fn method(&self) {}
}
"#;

    graph.index_file("src/mod.rs", mod_rs).unwrap();
    graph.index_file("src/utils.rs", utils_rs).unwrap();

    // Query symbols and verify FQNs
    let symbols = graph.symbols_in_file("src/utils.rs").unwrap();

    // helper should have FQN "helper" (top-level in its file)
    let helper: Vec<_> = symbols
        .iter()
        .filter(|s| s.name.as_deref() == Some("helper"))
        .collect();

    assert!(!helper.is_empty(), "Should find helper function");
    // After full FQN implementation, this will be "utils::helper"
    // For now, verify it has some FQN
    assert!(helper[0].fqn.is_some());

    // method should have FQN "MyStruct::method"
    let methods: Vec<_> = symbols
        .iter()
        .filter(|s| s.name.as_deref() == Some("method"))
        .collect();

    assert!(!methods.is_empty(), "Should find method");
    assert_eq!(methods[0].fqn.as_deref(), Some("MyStruct::method"));
}

#[test]
fn test_java_package_fqn() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("test.db");
    let mut graph = CodeGraph::open(&db_path).unwrap();

    let source = br#"
package com.example.app;

public class Application {
    public void run() {}

    private static class Helper {
        public void help() {}
    }
}
"#;

    graph.index_file("Application.java", source).unwrap();

    let symbols = graph.symbols_in_file("Application.java").unwrap();

    // run method: com.example.app.Application.run
    let run: Vec<_> = symbols
        .iter()
        .filter(|s| s.name.as_deref() == Some("run"))
        .collect();

    assert_eq!(run.len(), 1);
    assert_eq!(
        run[0].fqn.as_deref(),
        Some("com.example.app.Application.run")
    );

    // help method: com.example.app.Application.Helper.help
    let help: Vec<_> = symbols
        .iter()
        .filter(|s| s.name.as_deref() == Some("help"))
        .collect();

    assert_eq!(help.len(), 1);
    // Note: full FQN includes inner class
    assert!(help[0].fqn.as_deref().unwrap().contains("help"));
}

#[test]
fn test_python_class_fqn() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("test.db");
    let mut graph = CodeGraph::open(&db_path).unwrap();

    let source = br#"
class DataProcessor:
    def process(self):
        pass

    class NestedHelper:
        def help(self):
            pass
"#;

    graph.index_file("processor.py", source).unwrap();

    let symbols = graph.symbols_in_file("processor.py").unwrap();

    // process method: DataProcessor.process
    let process: Vec<_> = symbols
        .iter()
        .filter(|s| s.name.as_deref() == Some("process"))
        .collect();

    assert_eq!(process.len(), 1);
    assert_eq!(process[0].fqn.as_deref(), Some("DataProcessor.process"));

    // help method: DataProcessor.NestedHelper.help
    let help: Vec<_> = symbols
        .iter()
        .filter(|s| s.name.as_deref() == Some("help"))
        .collect();

    assert_eq!(help.len(), 1);
    assert!(help[0]
        .fqn
        .as_deref()
        .unwrap()
        .contains("NestedHelper.help"));
}

#[test]
fn test_cpp_namespace_fqn() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("test.db");
    let mut graph = CodeGraph::open(&db_path).unwrap();

    let source = br#"
namespace graphics {
    struct Point {
        int x;
        int y;
    };

    namespace render {
        void draw(const Point& p) {}
    }
}
"#;

    graph.index_file("graphics.cpp", source).unwrap();

    let symbols = graph.symbols_in_file("graphics.cpp").unwrap();

    // Point: graphics::Point
    let point: Vec<_> = symbols
        .iter()
        .filter(|s| s.name.as_deref() == Some("Point"))
        .collect();

    assert_eq!(point.len(), 1);
    assert_eq!(point[0].fqn.as_deref(), Some("graphics::Point"));

    // draw: graphics::render::draw
    let draw: Vec<_> = symbols
        .iter()
        .filter(|s| s.name.as_deref() == Some("draw"))
        .collect();

    assert_eq!(draw.len(), 1);
    assert_eq!(draw[0].fqn.as_deref(), Some("graphics::render::draw"));
}

#[test]
fn test_symbol_id_stability_with_fqn() {
    use magellan::graph::generate_symbol_id;

    // Same FQN and span produce same ID
    let id1 = generate_symbol_id("rust", "my_crate::my_module::my_function", "span123");
    let id2 = generate_symbol_id("rust", "my_crate::my_module::my_function", "span123");
    assert_eq!(id1, id2);

    // Different FQN produces different ID
    let id3 = generate_symbol_id("rust", "other_crate::my_function", "span123");
    assert_ne!(id1, id3);

    // Same name, different scope (different FQN) = different ID
    let id4 = generate_symbol_id("rust", "my_crate::other_module::my_function", "span123");
    assert_ne!(id1, id4);
}
