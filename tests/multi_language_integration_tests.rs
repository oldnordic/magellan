//! Integration tests for multi-language support
//!
//! Tests end-to-end indexing of mixed-language codebases.

use tempfile::TempDir;

#[test]
fn test_multi_language_scan() {
    // Test scanning a directory with files from multiple supported languages
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    let mut graph = magellan::CodeGraph::open(&db_path).unwrap();

    // Create test files in different languages
    // Rust
    let rust_file = temp_dir.path().join("main.rs");
    std::fs::write(
        &rust_file,
        r#"
fn main() {
    println!("Hello");
}

struct Point {
    x: i32,
}
"#,
    )
    .unwrap();

    // Python
    let python_file = temp_dir.path().join("utils.py");
    std::fs::write(
        &python_file,
        r#"
def helper():
    pass

class DataProcessor:
    def process(self):
        pass
"#,
    )
    .unwrap();

    // C
    let c_file = temp_dir.path().join("config.c");
    std::fs::write(
        &c_file,
        r#"
int get_config() {
    return 0;
}

struct Config {
    int value;
};

enum Status {
    OK,
    Error
};
"#,
    )
    .unwrap();

    // C++
    let cpp_file = temp_dir.path().join("processor.cpp");
    std::fs::write(
        &cpp_file,
        r#"
class Processor {
public:
    void process() {}
};

namespace utils {
    struct Helper {};
}
"#,
    )
    .unwrap();

    // Java
    let java_file = temp_dir.path().join("Main.java");
    std::fs::write(
        &java_file,
        r#"
class Main {
    public static void run() {}
}

interface Handler {
    void handle();
}

enum Color {
    RED, GREEN
}
"#,
    )
    .unwrap();

    // JavaScript
    let js_file = temp_dir.path().join("app.js");
    std::fs::write(
        &js_file,
        r#"
function init() {
    return;
}

class App {
    render() {}
}

export function helper() {}
"#,
    )
    .unwrap();

    // TypeScript
    let ts_file = temp_dir.path().join("types.ts");
    std::fs::write(
        &ts_file,
        r#"
interface Options {
    name: string;
}

type Config = string | number;

enum Status {
    Pending,
    Done
}

namespace utils {
    class Helper {}
}
"#,
    )
    .unwrap();

    // Scan directory
    let file_count = graph.scan_directory(temp_dir.path(), None).unwrap();

    // Should have indexed 7 source files (not .db)
    assert_eq!(file_count, 7, "Should scan 7 source files");

    // Verify each file has symbols
    let rust_symbols = graph.symbols_in_file(rust_file.to_str().unwrap()).unwrap();
    assert!(
        rust_symbols.len() >= 2,
        "Rust file should have at least 2 symbols"
    );

    let python_symbols = graph
        .symbols_in_file(python_file.to_str().unwrap())
        .unwrap();
    assert!(
        python_symbols.len() >= 2,
        "Python file should have at least 2 symbols"
    );

    let c_symbols = graph.symbols_in_file(c_file.to_str().unwrap()).unwrap();
    assert!(
        c_symbols.len() >= 3,
        "C file should have at least 3 symbols"
    );

    let cpp_symbols = graph.symbols_in_file(cpp_file.to_str().unwrap()).unwrap();
    assert!(
        cpp_symbols.len() >= 2,
        "C++ file should have at least 2 symbols"
    );

    let java_symbols = graph.symbols_in_file(java_file.to_str().unwrap()).unwrap();
    assert!(
        java_symbols.len() >= 3,
        "Java file should have at least 3 symbols"
    );

    let js_symbols = graph.symbols_in_file(js_file.to_str().unwrap()).unwrap();
    assert!(
        js_symbols.len() >= 2,
        "JavaScript file should have at least 2 symbols"
    );

    let ts_symbols = graph.symbols_in_file(ts_file.to_str().unwrap()).unwrap();
    assert!(
        ts_symbols.len() >= 4,
        "TypeScript file should have at least 4 symbols"
    );

    // Verify symbol counts per file
    // Rust: main (Function), Point (Class)
    let rust_functions: Vec<_> = rust_symbols
        .iter()
        .filter(|s| s.kind == magellan::SymbolKind::Function)
        .collect();
    assert_eq!(rust_functions.len(), 1);

    let rust_classes: Vec<_> = rust_symbols
        .iter()
        .filter(|s| s.kind == magellan::SymbolKind::Class)
        .collect();
    assert_eq!(rust_classes.len(), 1);

    // Python: helper (Function), DataProcessor (Class), process (Function - method in class)
    let python_functions: Vec<_> = python_symbols
        .iter()
        .filter(|s| s.kind == magellan::SymbolKind::Function)
        .collect();
    assert_eq!(
        python_functions.len(),
        2,
        "Should have 2 functions (helper + process)"
    );

    let python_classes: Vec<_> = python_symbols
        .iter()
        .filter(|s| s.kind == magellan::SymbolKind::Class)
        .collect();
    assert_eq!(python_classes.len(), 1);

    // C: get_config (Function), Config (Class), Status (Enum)
    let c_enums: Vec<_> = c_symbols
        .iter()
        .filter(|s| s.kind == magellan::SymbolKind::Enum)
        .collect();
    assert_eq!(c_enums.len(), 1);

    // C++: Processor (Class), process (Function), utils (Namespace), Helper (Class)
    let cpp_namespaces: Vec<_> = cpp_symbols
        .iter()
        .filter(|s| s.kind == magellan::SymbolKind::Namespace)
        .collect();
    assert_eq!(
        cpp_namespaces.len(),
        1,
        "Should have exactly 1 namespace (utils)"
    );

    // Java: Main (Class), run (Method), Handler (Interface), handle (Method), Color (Enum)
    let java_interfaces: Vec<_> = java_symbols
        .iter()
        .filter(|s| s.kind == magellan::SymbolKind::Interface)
        .collect();
    assert_eq!(java_interfaces.len(), 1);

    let java_methods: Vec<_> = java_symbols
        .iter()
        .filter(|s| s.kind == magellan::SymbolKind::Method)
        .collect();
    assert_eq!(
        java_methods.len(),
        2,
        "Should have 2 methods (run + handle)"
    );

    // TypeScript: Options (Interface), Config (TypeAlias), Status (Enum), utils (Namespace)
    let ts_interfaces: Vec<_> = ts_symbols
        .iter()
        .filter(|s| s.kind == magellan::SymbolKind::Interface)
        .collect();
    assert_eq!(ts_interfaces.len(), 1);

    let ts_type_aliases: Vec<_> = ts_symbols
        .iter()
        .filter(|s| s.kind == magellan::SymbolKind::TypeAlias)
        .collect();
    assert_eq!(ts_type_aliases.len(), 1);

    let ts_namespaces: Vec<_> = ts_symbols
        .iter()
        .filter(|s| s.kind == magellan::SymbolKind::Namespace)
        .collect();
    assert_eq!(ts_namespaces.len(), 1);
}

#[test]
fn test_scan_filters_unsupported_files() {
    // Verify that unsupported file types are not scanned
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    let mut graph = magellan::CodeGraph::open(&db_path).unwrap();

    // Create source file
    let rs_file = temp_dir.path().join("code.rs");
    std::fs::write(&rs_file, b"fn test() {}").unwrap();

    // Create unsupported files
    let txt_file = temp_dir.path().join("README.txt");
    std::fs::write(&txt_file, b"Some text").unwrap();

    let md_file = temp_dir.path().join("doc.md");
    std::fs::write(&md_file, b"# Documentation").unwrap();

    let json_file = temp_dir.path().join("data.json");
    std::fs::write(&json_file, b"{}").unwrap();

    // Scan directory
    let file_count = graph.scan_directory(temp_dir.path(), None).unwrap();

    // Should only scan the .rs file
    assert_eq!(file_count, 1, "Should only scan 1 source file");

    // Verify the .rs file was indexed
    let symbols = graph.symbols_in_file(rs_file.to_str().unwrap()).unwrap();
    assert_eq!(symbols.len(), 1);
}

#[test]
fn test_language_detection_in_indexing() {
    // Test that index_file correctly dispatches to the right parser
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    let mut graph = magellan::CodeGraph::open(&db_path).unwrap();

    // Test Python file
    let py_file = temp_dir.path().join("test.py");
    let py_source = b"
def python_function():
    pass

class PythonClass:
    pass
";
    graph
        .index_file(py_file.to_str().unwrap(), py_source)
        .unwrap();
    let py_symbols = graph.symbols_in_file(py_file.to_str().unwrap()).unwrap();
    assert_eq!(py_symbols.len(), 2);

    // Test C++ file
    let cpp_file = temp_dir.path().join("test.cpp");
    let cpp_source = b"
class CppClass {};
namespace CppNamespace {}
";
    graph
        .index_file(cpp_file.to_str().unwrap(), cpp_source)
        .unwrap();
    let cpp_symbols = graph.symbols_in_file(cpp_file.to_str().unwrap()).unwrap();
    assert_eq!(cpp_symbols.len(), 2);

    // Test TypeScript file
    let ts_file = temp_dir.path().join("test.ts");
    let ts_source = b"
interface TsInterface {}
type TsType = string;
enum TsEnum {}
namespace TsNamespace {}
";
    graph
        .index_file(ts_file.to_str().unwrap(), ts_source)
        .unwrap();
    let ts_symbols = graph.symbols_in_file(ts_file.to_str().unwrap()).unwrap();
    assert_eq!(ts_symbols.len(), 4);
}

#[test]
fn test_graceful_degradation_for_unsupported_language() {
    // Test that unsupported languages return empty without crashing
    let temp_dir = TempDir::new().unwrap();
    let db_path = temp_dir.path().join("test.db");

    let mut graph = magellan::CodeGraph::open(&db_path).unwrap();

    // Create a Go file (unsupported language)
    let go_file = temp_dir.path().join("test.go");
    let go_source = b"package main\n\nfunc main() {}";
    let result = graph.index_file(go_file.to_str().unwrap(), go_source);

    // Should succeed but return 0 symbols (graceful degradation)
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), 0);
}
