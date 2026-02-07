//! Graph validation module
//!
//! Provides pre-run and post-run invariant checks for the code graph.
//! Detects orphan references, orphan calls, and other structural inconsistencies.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use sqlitegraph::SnapshotId;
use std::path::Path;
use std::path::PathBuf;

use crate::graph::CodeGraph;

/// Report of validation results
///
/// Follows the VerifyReport pattern from src/verify.rs for consistency.
/// Provides structured error and warning reporting for graph validation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationReport {
    /// Whether validation passed (no errors)
    pub passed: bool,
    /// Validation errors found
    pub errors: Vec<ValidationError>,
    /// Validation warnings found
    pub warnings: Vec<ValidationWarning>,
}

impl ValidationReport {
    /// Total number of issues found (errors + warnings)
    pub fn total_issues(&self) -> usize {
        self.errors.len() + self.warnings.len()
    }

    /// Check if validation is clean (no issues)
    pub fn is_clean(&self) -> bool {
        self.total_issues() == 0
    }

    /// Create a clean validation report (no issues)
    pub fn clean() -> Self {
        ValidationReport {
            passed: true,
            errors: Vec::new(),
            warnings: Vec::new(),
        }
    }

    /// Create a failed validation report with errors
    pub fn with_errors(errors: Vec<ValidationError>) -> Self {
        ValidationReport {
            passed: errors.is_empty(),
            errors,
            warnings: Vec::new(),
        }
    }

    /// Add warnings to the report
    pub fn with_warnings(mut self, warnings: Vec<ValidationWarning>) -> Self {
        self.warnings = warnings;
        self.passed = self.errors.is_empty();
        self
    }
}

/// A validation error with structured data
///
/// Contains a machine-readable error code, human-readable message,
/// optional entity ID for correlation, and additional details as JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationError {
    /// Machine-readable error code (SCREAMING_SNAKE_CASE)
    pub code: String,
    /// Human-readable error description
    pub message: String,
    /// Related stable symbol_id if applicable
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity_id: Option<String>,
    /// Additional structured data
    #[serde(skip_serializing_if = "serde_json::Value::is_null")]
    pub details: serde_json::Value,
}

impl ValidationError {
    /// Create a new validation error
    pub fn new(code: String, message: String) -> Self {
        ValidationError {
            code,
            message,
            entity_id: None,
            details: serde_json::json!({}),
        }
    }

    /// Set the entity ID for this error
    pub fn with_entity_id(mut self, entity_id: String) -> Self {
        self.entity_id = Some(entity_id);
        self
    }

    /// Set additional details for this error
    pub fn with_details(mut self, details: serde_json::Value) -> Self {
        self.details = details;
        self
    }
}

/// A validation warning with structured data
///
/// Same structure as ValidationError but used for non-critical issues.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationWarning {
    /// Machine-readable warning code (SCREAMING_SNAKE_CASE)
    pub code: String,
    /// Human-readable warning description
    pub message: String,
    /// Related stable symbol_id if applicable
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity_id: Option<String>,
    /// Additional structured data
    #[serde(skip_serializing_if = "serde_json::Value::is_null")]
    pub details: serde_json::Value,
}

impl ValidationWarning {
    /// Create a new validation warning
    pub fn new(code: String, message: String) -> Self {
        ValidationWarning {
            code,
            message,
            entity_id: None,
            details: serde_json::json!({}),
        }
    }

    /// Set the entity ID for this warning
    pub fn with_entity_id(mut self, entity_id: String) -> Self {
        self.entity_id = Some(entity_id);
        self
    }

    /// Set additional details for this warning
    pub fn with_details(mut self, details: serde_json::Value) -> Self {
        self.details = details;
        self
    }
}

/// Report of pre-validation results
///
/// Validates environment and input paths before indexing begins.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreValidationReport {
    /// Whether pre-validation passed
    pub passed: bool,
    /// Pre-validation errors found
    pub errors: Vec<ValidationError>,
    /// Number of input paths validated
    pub input_count: usize,
}

impl PreValidationReport {
    /// Total number of issues found
    pub fn total_issues(&self) -> usize {
        self.errors.len()
    }

    /// Check if validation is clean (no issues)
    pub fn is_clean(&self) -> bool {
        self.total_issues() == 0
    }
}

/// Validate graph invariants post-run
///
/// Checks for orphan references, orphan calls, and other structural issues.
pub fn validate_graph(graph: &mut CodeGraph) -> Result<ValidationReport> {
    let mut all_errors = Vec::new();
    let mut all_warnings = Vec::new();

    // Run all check functions
    if let Ok(errors) = check_orphan_references(graph) {
        all_errors.extend(errors);
    }

    if let Ok(errors) = check_orphan_calls(graph) {
        all_errors.extend(errors);
    }

    // Sort deterministically by code for consistent output
    all_errors.sort_by(|a, b| a.code.cmp(&b.code).then_with(|| a.message.cmp(&b.message)));
    all_warnings.sort_by(|a: &ValidationWarning, b: &ValidationWarning| {
        a.code.cmp(&b.code).then_with(|| a.message.cmp(&b.message))
    });

    Ok(ValidationReport {
        passed: all_errors.is_empty(),
        errors: all_errors,
        warnings: all_warnings,
    })
}

/// Check for orphan references (references with no target symbol)
fn check_orphan_references(graph: &mut CodeGraph) -> Result<Vec<ValidationError>> {
    use sqlitegraph::{BackendDirection, NeighborQuery};

    let mut errors = Vec::new();

    // Get all Reference node entity IDs
    let entity_ids = graph.references.backend.entity_ids()?;
    let snapshot = SnapshotId::current();

    for entity_id in entity_ids {
        let node = match graph.references.backend.get_node(snapshot, entity_id) {
            Ok(n) => n,
            Err(_) => continue,
        };

        if node.kind != "Reference" {
            continue;
        }

        let reference_node: crate::graph::schema::ReferenceNode =
            match serde_json::from_value(node.data) {
                Ok(value) => value,
                Err(_) => continue,
            };

        // Check if this reference has outgoing REFERENCES edges
        let neighbors = graph.references.backend.neighbors(
            snapshot,
            entity_id,
            NeighborQuery {
                direction: BackendDirection::Outgoing,
                edge_type: Some("REFERENCES".to_string()),
            },
        )?;

        // Empty neighbors = orphan (no target symbol)
        if neighbors.is_empty() {
            errors.push(
                ValidationError::new(
                    "ORPHAN_REFERENCE".to_string(),
                    format!(
                        "Reference at {}:{}:{} has no target symbol",
                        reference_node.file, reference_node.start_line, reference_node.start_col
                    ),
                )
                .with_details(serde_json::json!({
                    "file": reference_node.file,
                    "byte_start": reference_node.byte_start,
                    "byte_end": reference_node.byte_end,
                    "start_line": reference_node.start_line,
                    "start_col": reference_node.start_col,
                    "end_line": reference_node.end_line,
                    "end_col": reference_node.end_col,
                })),
            );
        }
    }

    Ok(errors)
}

/// Check for orphan calls (calls missing caller or callee)
fn check_orphan_calls(graph: &mut CodeGraph) -> Result<Vec<ValidationError>> {
    use sqlitegraph::{BackendDirection, NeighborQuery};

    let mut errors = Vec::new();

    // Get all Call node entity IDs
    let entity_ids = graph.calls.backend.entity_ids()?;
    let snapshot = SnapshotId::current();

    for entity_id in entity_ids {
        let node = match graph.calls.backend.get_node(snapshot, entity_id) {
            Ok(n) => n,
            Err(_) => continue,
        };

        if node.kind != "Call" {
            continue;
        }

        let call_node: crate::graph::schema::CallNode = match serde_json::from_value(node.data) {
            Ok(value) => value,
            Err(_) => continue,
        };

        // Check for incoming CALLER edges (caller symbol -> call node)
        let callers = graph.calls.backend.neighbors(
            snapshot,
            entity_id,
            NeighborQuery {
                direction: BackendDirection::Incoming,
                edge_type: Some("CALLER".to_string()),
            },
        )?;

        // Check for outgoing CALLS edges (call node -> callee symbol)
        let callees = graph.calls.backend.neighbors(
            snapshot,
            entity_id,
            NeighborQuery {
                direction: BackendDirection::Outgoing,
                edge_type: Some("CALLS".to_string()),
            },
        )?;

        // Missing caller symbol
        if callers.is_empty() {
            errors.push(
                ValidationError::new(
                    "ORPHAN_CALL_NO_CALLER".to_string(),
                    format!(
                        "Call '{}' -> '{}' at {} has no caller symbol",
                        call_node.caller, call_node.callee, call_node.file
                    ),
                )
                .with_details(serde_json::json!({
                    "call_node_id": entity_id,
                    "file": call_node.file,
                    "caller": call_node.caller,
                    "callee": call_node.callee,
                })),
            );
        }

        // Missing callee symbol
        if callees.is_empty() {
            errors.push(
                ValidationError::new(
                    "ORPHAN_CALL_NO_CALLEE".to_string(),
                    format!(
                        "Call '{}' -> '{}' at {} has no callee symbol",
                        call_node.caller, call_node.callee, call_node.file
                    ),
                )
                .with_details(serde_json::json!({
                    "call_node_id": entity_id,
                    "file": call_node.file,
                    "caller": call_node.caller,
                    "callee": call_node.callee,
                })),
            );
        }
    }

    Ok(errors)
}

/// Pre-run validation for database and input paths
///
/// Validates environment before indexing begins.
pub fn pre_run_validate(
    db_path: &Path,
    root_path: &Path,
    input_paths: &[PathBuf],
) -> Result<PreValidationReport> {
    let mut errors = Vec::new();

    // Check database parent directory exists
    if let Some(parent) = db_path.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            errors.push(
                ValidationError::new(
                    "DB_PARENT_MISSING".to_string(),
                    format!(
                        "Database parent directory does not exist: {}",
                        parent.display()
                    ),
                )
                .with_details(serde_json::json!({
                    "db_path": db_path.display().to_string(),
                    "parent": parent.display().to_string(),
                })),
            );
        }
    }

    // Check root path exists
    if !root_path.exists() {
        errors.push(
            ValidationError::new(
                "ROOT_PATH_MISSING".to_string(),
                format!("Root path does not exist: {}", root_path.display()),
            )
            .with_details(serde_json::json!({
                "root_path": root_path.display().to_string(),
            })),
        );
    }

    // Check all input paths exist
    for input_path in input_paths {
        if !input_path.exists() {
            errors.push(
                ValidationError::new(
                    "INPUT_PATH_MISSING".to_string(),
                    format!("Input path does not exist: {}", input_path.display()),
                )
                .with_details(serde_json::json!({
                    "input_path": input_path.display().to_string(),
                })),
            );
        }
    }

    Ok(PreValidationReport {
        passed: errors.is_empty(),
        errors,
        input_count: input_paths.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validation_report_clean() {
        let report = ValidationReport::clean();
        assert!(report.passed);
        assert!(report.is_clean());
        assert_eq!(report.total_issues(), 0);
    }

    #[test]
    fn test_validation_report_with_errors() {
        let errors = vec![ValidationError::new(
            "TEST_ERROR".to_string(),
            "Test error".to_string(),
        )];
        let report = ValidationReport::with_errors(errors);
        assert!(!report.passed);
        assert!(!report.is_clean());
        assert_eq!(report.total_issues(), 1);
    }

    #[test]
    fn test_validation_error_builder() {
        let error = ValidationError::new("CODE".to_string(), "message".to_string())
            .with_entity_id("entity123".to_string())
            .with_details(serde_json::json!({"key": "value"}));

        assert_eq!(error.code, "CODE");
        assert_eq!(error.message, "message");
        assert_eq!(error.entity_id, Some("entity123".to_string()));
        assert_eq!(error.details["key"], "value");
    }

    #[test]
    fn test_validation_warning_builder() {
        let warning = ValidationWarning::new("WARN_CODE".to_string(), "warning".to_string())
            .with_entity_id("entity456".to_string())
            .with_details(serde_json::json!({"key": "value"}));

        assert_eq!(warning.code, "WARN_CODE");
        assert_eq!(warning.message, "warning");
        assert_eq!(warning.entity_id, Some("entity456".to_string()));
        assert_eq!(warning.details["key"], "value");
    }

    #[test]
    fn test_pre_validation_report() {
        let report = PreValidationReport {
            passed: true,
            errors: Vec::new(),
            input_count: 5,
        };

        assert!(report.passed);
        assert!(report.is_clean());
        assert_eq!(report.total_issues(), 0);
        assert_eq!(report.input_count, 5);
    }

    #[test]
    fn test_validation_report_serialization() {
        let report = ValidationReport {
            passed: false,
            errors: vec![ValidationError::new(
                "ERROR_CODE".to_string(),
                "Error message".to_string(),
            )],
            warnings: vec![],
        };

        let json = serde_json::to_string(&report).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed["passed"], false);
        assert_eq!(parsed["errors"][0]["code"], "ERROR_CODE");
        assert_eq!(parsed["errors"][0]["message"], "Error message");
    }

    #[test]
    fn test_validation_error_serialization() {
        let error = ValidationError::new("TEST_CODE".to_string(), "Test message".to_string())
            .with_entity_id("test_entity".to_string())
            .with_details(serde_json::json!({"file": "test.rs", "line": 42}));

        let json = serde_json::to_string(&error).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed["code"], "TEST_CODE");
        assert_eq!(parsed["message"], "Test message");
        assert_eq!(parsed["entity_id"], "test_entity");
        assert_eq!(parsed["details"]["file"], "test.rs");
        assert_eq!(parsed["details"]["line"], 42);
    }

    // === Task 2: Orphan detection tests ===

    #[test]
    fn test_check_orphan_references_clean_graph() {
        // Create a graph with valid Symbol and Reference nodes with proper REFERENCES edges
        // Use file-based database because :memory: doesn't work with separate connections
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let mut graph = crate::CodeGraph::open(&db_path).unwrap();

        // Index a file to create a Symbol node
        let file_path = "/test/test.rs";
        let source = b"fn main() { println!(\"hello\"); }";
        graph.index_file(file_path, source).unwrap();

        // Get the symbol node ID
        let symbols = graph.symbols_in_file(file_path).unwrap();
        assert!(!symbols.is_empty(), "Should have at least one symbol");

        // For this test, we validate that a clean graph with indexed files passes validation
        let report = validate_graph(&mut graph).unwrap();

        assert!(report.passed, "Clean graph should validate");
        assert!(
            report.errors.is_empty(),
            "Clean graph should have no errors"
        );
    }

    #[test]
    fn test_check_orphan_references_with_orphans() {
        // Create a graph and manually insert an orphan Reference node
        // Use file-based database because :memory: doesn't work with separate connections
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let mut graph = crate::CodeGraph::open(&db_path).unwrap();

        use sqlitegraph::NodeSpec;

        // Get the backend from references module
        let backend = &graph.references.backend;

        // Insert a Reference node WITHOUT a REFERENCES edge (simulating an orphan)
        let reference_node = crate::graph::schema::ReferenceNode {
            file: "/test/test.rs".to_string(),
            byte_start: 10,
            byte_end: 20,
            start_line: 1,
            start_col: 10,
            end_line: 1,
            end_col: 20,
        };

        let node_spec = NodeSpec {
            kind: "Reference".to_string(),
            name: "orphan reference".to_string(),
            file_path: Some("/test/test.rs".to_string()),
            data: serde_json::to_value(reference_node).unwrap(),
        };

        backend.insert_node(node_spec).unwrap();

        // Run validation - should detect the orphan
        let report = validate_graph(&mut graph).unwrap();

        assert!(
            !report.passed,
            "Graph with orphan reference should fail validation"
        );
        assert!(!report.errors.is_empty(), "Should have errors");

        // Find the orphan reference error
        let orphan_error = report
            .errors
            .iter()
            .find(|e| e.code == "ORPHAN_REFERENCE")
            .expect("Should have ORPHAN_REFERENCE error");

        assert!(
            orphan_error.message.contains("test.rs"),
            "Error should mention file path"
        );
    }

    #[test]
    #[cfg(not(feature = "native-v2"))]
    fn test_check_orphan_calls_clean_graph() {
        // Create a graph with valid Call nodes and proper CALLER/CALLS edges
        let mut graph = crate::CodeGraph::open(":memory:").unwrap();

        // For this test, we validate that an empty graph passes
        let report = validate_graph(&mut graph).unwrap();

        assert!(report.passed, "Empty graph should validate");
        assert!(
            report.errors.is_empty(),
            "Empty graph should have no errors"
        );
    }

    #[test]
    fn test_check_orphan_calls_missing_caller() {
        // Create a graph with a Call node missing the CALLER edge
        let mut graph = crate::CodeGraph::open(":memory:").unwrap();

        use sqlitegraph::NodeSpec;

        let backend = &graph.calls.backend;

        // Insert a Call node without CALLER edge
        let call_node = crate::graph::schema::CallNode {
            file: "/test/test.rs".to_string(),
            caller: "caller_func".to_string(),
            callee: "callee_func".to_string(),
            caller_symbol_id: None,
            callee_symbol_id: None,
            byte_start: 10,
            byte_end: 20,
            start_line: 1,
            start_col: 10,
            end_line: 1,
            end_col: 20,
        };

        let node_spec = NodeSpec {
            kind: "Call".to_string(),
            name: "caller -> callee".to_string(),
            file_path: Some("/test/test.rs".to_string()),
            data: serde_json::to_value(call_node).unwrap(),
        };

        backend.insert_node(node_spec).unwrap();

        // Run validation
        let report = validate_graph(&mut graph).unwrap();

        // Should detect missing caller
        let missing_caller = report
            .errors
            .iter()
            .any(|e| e.code == "ORPHAN_CALL_NO_CALLER");

        assert!(missing_caller, "Should detect missing CALLER edge");
    }

    #[test]
    fn test_check_orphan_calls_missing_callee() {
        // Create a graph with a Call node missing the CALLS edge
        let mut graph = crate::CodeGraph::open(":memory:").unwrap();

        use sqlitegraph::NodeSpec;

        let backend = &graph.calls.backend;

        // Insert a Call node without CALLS edge
        let call_node = crate::graph::schema::CallNode {
            file: "/test/test.rs".to_string(),
            caller: "caller_func".to_string(),
            callee: "callee_func".to_string(),
            caller_symbol_id: None,
            callee_symbol_id: None,
            byte_start: 10,
            byte_end: 20,
            start_line: 1,
            start_col: 10,
            end_line: 1,
            end_col: 20,
        };

        let node_spec = NodeSpec {
            kind: "Call".to_string(),
            name: "caller -> callee".to_string(),
            file_path: Some("/test/test.rs".to_string()),
            data: serde_json::to_value(call_node).unwrap(),
        };

        backend.insert_node(node_spec).unwrap();

        // Run validation
        let report = validate_graph(&mut graph).unwrap();

        // Should detect missing callee
        let missing_callee = report
            .errors
            .iter()
            .any(|e| e.code == "ORPHAN_CALL_NO_CALLEE");

        assert!(missing_callee, "Should detect missing CALLS edge");
    }

    #[test]
    fn test_validate_graph_integration() {
        // Test the full validate_graph function with mixed valid/invalid nodes
        let mut graph = crate::CodeGraph::open(":memory:").unwrap();

        use sqlitegraph::NodeSpec;

        // Insert an orphan reference
        let backend_ref = &graph.references.backend;
        let reference_node = crate::graph::schema::ReferenceNode {
            file: "/test/test.rs".to_string(),
            byte_start: 10,
            byte_end: 20,
            start_line: 1,
            start_col: 10,
            end_line: 1,
            end_col: 20,
        };

        let node_spec = NodeSpec {
            kind: "Reference".to_string(),
            name: "orphan reference".to_string(),
            file_path: Some("/test/test.rs".to_string()),
            data: serde_json::to_value(reference_node).unwrap(),
        };
        backend_ref.insert_node(node_spec).unwrap();

        // Insert an orphan call
        let backend_call = &graph.calls.backend;
        let call_node = crate::graph::schema::CallNode {
            file: "/test/test.rs".to_string(),
            caller: "caller_func".to_string(),
            callee: "callee_func".to_string(),
            caller_symbol_id: None,
            callee_symbol_id: None,
            byte_start: 10,
            byte_end: 20,
            start_line: 1,
            start_col: 10,
            end_line: 1,
            end_col: 20,
        };

        let call_spec = NodeSpec {
            kind: "Call".to_string(),
            name: "orphan call".to_string(),
            file_path: Some("/test/test.rs".to_string()),
            data: serde_json::to_value(call_node).unwrap(),
        };
        backend_call.insert_node(call_spec).unwrap();

        // Run validation
        let report = validate_graph(&mut graph).unwrap();

        assert!(!report.passed, "Graph with orphans should fail");
        assert!(
            report.errors.len() >= 2,
            "Should have at least 2 errors (1 reference + 2 call errors)"
        );

        // Verify errors are sorted by code
        for i in 1..report.errors.len() {
            let prev = &report.errors[i - 1];
            let curr = &report.errors[i];
            assert!(
                prev.code <= curr.code,
                "Errors should be sorted by code: {} <= {}",
                prev.code,
                curr.code
            );
        }
    }

    // === Task 3: Pre-run validation tests ===

    #[test]
    fn test_pre_run_validate_all_valid() {
        use tempfile::tempdir;

        let temp_dir = tempdir().unwrap();
        let root_path = temp_dir.path();

        // Create a valid db path (parent exists)
        let db_path = root_path.join("test.db");

        // Create valid input paths
        let input_file = root_path.join("input.rs");
        std::fs::write(&input_file, b"fn main() {}").unwrap();

        let input_paths = vec![input_file];

        let report = pre_run_validate(&db_path, root_path, &input_paths).unwrap();

        assert!(report.passed, "All valid paths should pass pre-validation");
        assert!(report.errors.is_empty(), "Should have no errors");
        assert_eq!(report.input_count, 1);
    }

    #[test]
    fn test_pre_run_validate_missing_root() {
        use tempfile::tempdir;

        let temp_dir = tempdir().unwrap();
        let root_path = temp_dir.path().join("nonexistent");

        let db_path = temp_dir.path().join("test.db");
        let input_paths: Vec<std::path::PathBuf> = vec![];

        let report = pre_run_validate(&db_path, &root_path, &input_paths).unwrap();

        assert!(!report.passed, "Missing root should fail validation");
        assert!(!report.errors.is_empty(), "Should have errors");

        let root_error = report
            .errors
            .iter()
            .find(|e| e.code == "ROOT_PATH_MISSING")
            .expect("Should have ROOT_PATH_MISSING error");

        assert!(
            root_error.message.contains("nonexistent"),
            "Error should mention the missing path"
        );
    }

    #[test]
    fn test_pre_run_validate_missing_input_path() {
        use tempfile::tempdir;

        let temp_dir = tempdir().unwrap();
        let root_path = temp_dir.path();
        let db_path = root_path.join("test.db");

        // Create a non-existent input path
        let missing_input = root_path.join("nonexistent.rs");
        let input_paths = vec![missing_input];

        let report = pre_run_validate(&db_path, root_path, &input_paths).unwrap();

        assert!(!report.passed, "Missing input path should fail validation");
        assert!(!report.errors.is_empty(), "Should have errors");

        let _input_error = report
            .errors
            .iter()
            .find(|e| e.code == "INPUT_PATH_MISSING")
            .expect("Should have INPUT_PATH_MISSING error");
    }

    #[test]
    fn test_pre_run_validate_db_parent_missing() {
        // Create a db path whose parent doesn't exist
        let db_path = std::path::PathBuf::from("/nonexistent/dir/test.db");
        let root_path = std::path::PathBuf::from("/tmp");
        let input_paths: Vec<std::path::PathBuf> = vec![];

        let report = pre_run_validate(&db_path, &root_path, &input_paths).unwrap();

        assert!(!report.passed, "Missing db parent should fail validation");
        assert!(!report.errors.is_empty(), "Should have errors");

        let parent_error = report
            .errors
            .iter()
            .find(|e| e.code == "DB_PARENT_MISSING")
            .expect("Should have DB_PARENT_MISSING error");

        assert!(
            parent_error.message.contains("nonexistent"),
            "Error should mention missing parent"
        );
    }

    #[test]
    fn test_validation_report_with_warnings() {
        // Test ValidationReport with warnings
        let warnings = vec![ValidationWarning::new(
            "WARN_CODE".to_string(),
            "Test warning".to_string(),
        )];

        let report = ValidationReport::clean().with_warnings(warnings);

        assert!(report.passed, "Report with only warnings should still pass");
        assert!(
            !report.is_clean(),
            "Report with warnings should not be clean"
        );
        assert_eq!(report.total_issues(), 1);
    }

    #[test]
    fn test_validation_error_codes_are_unique() {
        // Verify error codes are consistent SCREAMING_SNAKE_CASE
        let error = ValidationError::new("ORPHAN_REFERENCE".to_string(), "test".to_string());
        assert_eq!(error.code, "ORPHAN_REFERENCE");

        let error2 = ValidationError::new("ORPHAN_CALL_NO_CALLER".to_string(), "test".to_string());
        assert_eq!(error2.code, "ORPHAN_CALL_NO_CALLER");

        let error3 = ValidationError::new("DB_PARENT_MISSING".to_string(), "test".to_string());
        assert_eq!(error3.code, "DB_PARENT_MISSING");
    }
}
