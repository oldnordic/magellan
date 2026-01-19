//! Graph validation module
//!
//! Provides pre-run and post-run invariant checks for the code graph.
//! Detects orphan references, orphan calls, and other structural inconsistencies.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use sqlitegraph::GraphBackend;
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
    all_errors.sort_by(|a, b| {
        a.code.cmp(&b.code)
            .then_with(|| a.message.cmp(&b.message))
    });
    all_warnings.sort_by(|a: &ValidationWarning, b: &ValidationWarning| {
        a.code.cmp(&b.code)
            .then_with(|| a.message.cmp(&b.message))
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

    for entity_id in entity_ids {
        let node = match graph.references.backend.get_node(entity_id) {
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

    for entity_id in entity_ids {
        let node = match graph.calls.backend.get_node(entity_id) {
            Ok(n) => n,
            Err(_) => continue,
        };

        if node.kind != "Call" {
            continue;
        }

        let call_node: crate::graph::schema::CallNode =
            match serde_json::from_value(node.data) {
                Ok(value) => value,
                Err(_) => continue,
            };

        // Check for incoming CALLER edges (caller symbol -> call node)
        let callers = graph.calls.backend.neighbors(
            entity_id,
            NeighborQuery {
                direction: BackendDirection::Incoming,
                edge_type: Some("CALLER".to_string()),
            },
        )?;

        // Check for outgoing CALLS edges (call node -> callee symbol)
        let callees = graph.calls.backend.neighbors(
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
                    format!(
                        "Input path does not exist: {}",
                        input_path.display()
                    ),
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
        let errors = vec![
            ValidationError::new("TEST_ERROR".to_string(), "Test error".to_string()),
        ];
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
            errors: vec![
                ValidationError::new("ERROR_CODE".to_string(), "Error message".to_string()),
            ],
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
}
