#![cfg(feature = "external-tools-cfg")]

//! External tools CFG extraction integration tests
//!
//! Tests the complete pipeline from source files to CFG blocks
//! using external tools (clang, javac).

use std::io::Write;
use tempfile::NamedTempFile;

/// Test that clang detection works
#[test]
fn test_clang_detection() {
    // Skip test if clang is not available
    if !magellan::graph::external_tools::tool_detector::is_tool_available("clang") {
        return;
    }

    // Try to get clang version
    let version = magellan::graph::external_tools::tool_detector::check_clang_version();
    assert!(version.is_ok());
}

/// Test that javac detection works
#[test]
fn test_javac_detection() {
    // Skip test if javac is not available
    if !magellan::graph::external_tools::tool_detector::is_tool_available("javac") {
        return;
    }

    // Try to get javac version
    let version = magellan::graph::external_tools::tool_detector::check_javac_version();
    assert!(version.is_ok());
}

/// Test C/C++ CFG extraction with simple function
#[test]
fn test_cpp_cfg_extraction_simple() {
    // Skip test if clang is not available
    if !magellan::graph::external_tools::tool_detector::is_tool_available("clang") {
        return;
    }

    // Create a simple C file with control flow
    let source = r#"
int test_if_else(int x) {
    if (x > 0) {
        return x * 2;
    } else {
        return x + 1;
    }
}
"#;

    let mut temp_file = NamedTempFile::with_suffix(".c").unwrap();
    temp_file.write_all(source.as_bytes()).unwrap();
    let source_path = temp_file.path();

    // Extract CFG
    let result = magellan::graph::external_tools::c_cpp::extract_cfg_from_cpp(source_path);

    // Verify CFG extraction succeeded
    assert!(
        result.is_ok(),
        "CFG extraction should succeed for valid C code"
    );

    let cfg = result.unwrap();
    assert!(!cfg.blocks.is_empty(), "CFG should have at least one block");
    assert!(!cfg.edges.is_empty(), "CFG should have at least one edge");
}

/// Test C/C++ CFG extraction with complex control flow
#[test]
fn test_cpp_cfg_extraction_complex() {
    // Skip test if clang is not available
    if !magellan::graph::external_tools::tool_detector::is_tool_available("clang") {
        return;
    }

    // Create a C file with switch statement
    let source = r#"
int test_switch(int x) {
    switch (x) {
        case 1:
            return 10;
        case 2:
            return 20;
        default:
            return 0;
    }
}
"#;

    let mut temp_file = NamedTempFile::with_suffix(".c").unwrap();
    temp_file.write_all(source.as_bytes()).unwrap();
    let source_path = temp_file.path();

    // Extract CFG
    let result = magellan::graph::external_tools::c_cpp::extract_cfg_from_cpp(source_path);

    // Verify CFG extraction succeeded
    assert!(result.is_ok());
    let cfg = result.unwrap();
    assert!(!cfg.blocks.is_empty());
}

/// Test C++ CFG extraction
#[test]
fn test_cpp_cfg_extraction() {
    // Skip test if clang is not available
    if !magellan::graph::external_tools::tool_detector::is_tool_available("clang") {
        return;
    }

    // Create a C++ file
    let source = r#"
extern "C" int cpp_function(int x) {
    if (x > 0) {
        return x * 2;
    }
    return x;
}
"#;

    let mut temp_file = NamedTempFile::with_suffix(".cpp").unwrap();
    temp_file.write_all(source.as_bytes()).unwrap();
    let source_path = temp_file.path();

    // Extract CFG
    let result = magellan::graph::external_tools::c_cpp::extract_cfg_from_cpp(source_path);

    // Verify CFG extraction succeeded
    assert!(result.is_ok());
    let cfg = result.unwrap();
    assert!(!cfg.blocks.is_empty());
}

/// Test Java CFG extraction
#[test]
fn test_java_cfg_extraction() {
    // Skip test if javac is not available
    if !magellan::graph::external_tools::tool_detector::is_tool_available("javac") {
        return;
    }

    // Create a simple Java file
    let source = r#"
public class TestCFG {
    public static int testIfElse(int x) {
        if (x > 0) {
            return x * 2;
        } else {
            return x + 1;
        }
    }

    public static int testSwitch(int x) {
        switch (x) {
            case 1:
                return 10;
            case 2:
                return 20;
            default:
                return 0;
        }
    }
}
"#;

    let mut temp_file = NamedTempFile::with_suffix(".java").unwrap();
    temp_file.write_all(source.as_bytes()).unwrap();
    let source_path = temp_file.path();

    // Extract CFG
    let result = magellan::graph::external_tools::java::extract_cfg_from_java(source_path);

    // Verify CFG extraction succeeded
    assert!(
        result.is_ok(),
        "CFG extraction should succeed for valid Java code"
    );

    let cfg = result.unwrap();
    assert!(!cfg.blocks.is_empty(), "CFG should have at least one block");
    assert!(!cfg.edges.is_empty(), "CFG should have at least one edge");
}

/// Test graceful fallback when clang not found
#[test]
fn test_graceful_fallback_clang_missing() {
    // This test verifies the behavior when clang is not available
    // We can't easily test this in environments where clang IS available
    // so we just verify the function doesn't panic

    // Create a C file
    let source = r#"
int test_func(int x) {
    return x + 1;
}
"#;

    let mut temp_file = NamedTempFile::with_suffix(".c").unwrap();
    temp_file.write_all(source.as_bytes()).unwrap();
    let source_path = temp_file.path();

    // The function should either succeed or fail gracefully
    let _ = magellan::graph::external_tools::c_cpp::extract_cfg_from_cpp(source_path);
}

/// Test graceful fallback when javac not found
#[test]
fn test_graceful_fallback_javac_missing() {
    // This test verifies the behavior when javac is not available
    // We can't easily test this in environments where javac IS available
    // so we just verify the function doesn't panic

    // Create a Java file
    let source = r#"
public class TestFunc {
    public static int test(int x) {
        return x + 1;
    }
}
"#;

    let mut temp_file = NamedTempFile::with_suffix(".java").unwrap();
    temp_file.write_all(source.as_bytes()).unwrap();
    let source_path = temp_file.path();

    // The function should either succeed or fail gracefully
    let _ = magellan::graph::external_tools::java::extract_cfg_from_java(source_path);
}

/// Test C/C++ specific function extraction
#[test]
fn test_cpp_function_extraction() {
    // Skip test if clang is not available
    if !magellan::graph::external_tools::tool_detector::is_tool_available("clang") {
        return;
    }

    // Create a C file with multiple functions
    let source = r#"
int foo(int x) {
    return x + 1;
}

int bar(int x) {
    return x * 2;
}
"#;

    let mut temp_file = NamedTempFile::with_suffix(".c").unwrap();
    temp_file.write_all(source.as_bytes()).unwrap();
    let source_path = temp_file.path();

    // Extract CFG for specific function
    let result =
        magellan::graph::external_tools::c_cpp::extract_cfg_for_function(source_path, "foo");

    assert!(result.is_ok());
    let cfg = result.unwrap();
    assert!(!cfg.blocks.is_empty());
}

/// Test Java specific method extraction
#[test]
fn test_java_method_extraction() {
    // Skip test if javac is not available
    if !magellan::graph::external_tools::tool_detector::is_tool_available("javac") {
        return;
    }

    // Create a Java file with multiple methods
    let source = r#"
public class TestMethods {
    public static int methodA(int x) {
        return x + 1;
    }

    public static int methodB(int x) {
        return x * 2;
    }
}
"#;

    let mut temp_file = NamedTempFile::with_suffix(".java").unwrap();
    temp_file.write_all(source.as_bytes()).unwrap();
    let source_path = temp_file.path();

    // Extract CFG for specific method
    let result =
        magellan::graph::external_tools::java::extract_cfg_for_method(source_path, "methodA");

    assert!(result.is_ok());
    let cfg = result.unwrap();
    assert!(!cfg.blocks.is_empty());
}

/// Test error handling for invalid C code
#[test]
fn test_cpp_invalid_code() {
    // Skip test if clang is not available
    if !magellan::graph::external_tools::tool_detector::is_tool_available("clang") {
        return;
    }

    // Create a C file with syntax error
    let source = r#"
int broken_func(int x {
    // Missing closing parenthesis
    return x;
}
"#;

    let mut temp_file = NamedTempFile::with_suffix(".c").unwrap();
    temp_file.write_all(source.as_bytes()).unwrap();
    let source_path = temp_file.path();

    // Extract CFG should fail gracefully
    let result = magellan::graph::external_tools::c_cpp::extract_cfg_from_cpp(source_path);
    assert!(result.is_err());
}

/// Test error handling for invalid Java code
#[test]
fn test_java_invalid_code() {
    // Skip test if javac is not available
    if !magellan::graph::external_tools::tool_detector::is_tool_available("javac") {
        return;
    }

    // Create a Java file with syntax error
    let source = r#"
public class Broken {
    public static void broken( {
        // Missing closing parenthesis
    }
}
"#;

    let mut temp_file = NamedTempFile::with_suffix(".java").unwrap();
    temp_file.write_all(source.as_bytes()).unwrap();
    let source_path = temp_file.path();

    // Extract CFG should fail gracefully
    let result = magellan::graph::external_tools::java::extract_cfg_from_java(source_path);
    assert!(result.is_err());
}

/// Test tool detector provides helpful error messages
#[test]
fn test_tool_detector_error_messages() {
    // Try to find a non-existent tool
    let result = magellan::graph::external_tools::tool_detector::find_clang();
    let _ = (result.is_ok(), result);

    // Verify installation instructions are available
    let clang_instructions =
        magellan::graph::external_tools::tool_detector::get_clang_install_instructions();
    assert!(!clang_instructions.is_empty());

    let javac_instructions =
        magellan::graph::external_tools::tool_detector::get_javac_install_instructions();
    assert!(!javac_instructions.is_empty());
}

/// Test cross-platform executable detection
#[test]
fn test_cross_platform_detection() {
    // Test that get_executable_name works
    #[cfg(unix)]
    {
        let name = magellan::graph::external_tools::tool_detector::get_executable_name("clang");
        assert_eq!(name, "clang");
    }

    #[cfg(windows)]
    {
        let name = magellan::graph::external_tools::tool_detector::get_executable_name("clang");
        assert_eq!(name, "clang.exe");
    }

    // Test is_tool_available doesn't panic
    let _ = magellan::graph::external_tools::tool_detector::is_tool_available("nonexistent_tool");
}
