//! Magellan-specific error codes
//!
//! Error codes follow the pattern: MAG-{CATEGORY}-{3-digit number}
//!
//! Categories (1-3 uppercase letters):
//! - REF: Reference-related errors (symbol lookup, span resolution)
//! - QRY: Query-related errors (invalid queries, file not found)
//! - IO: I/O-related errors (file access, permissions)
//! - V: Validation errors (checksum mismatch, invalid spans)
//!
//! Each error code is stable and should not be reused.

/// Symbol not found
pub const MAG_REF_001_SYMBOL_NOT_FOUND: &str = "MAG-REF-001";

/// Ambiguous symbol (multiple matches)
pub const MAG_REF_002_AMBIGUOUS_SYMBOL: &str = "MAG-REF-002";

/// Invalid span (start > end, out of bounds)
pub const MAG_REF_003_INVALID_SPAN: &str = "MAG-REF-003";

/// Invalid query syntax
pub const MAG_QRY_001_INVALID_QUERY: &str = "MAG-QRY-001";

/// File not found in database
pub const MAG_QRY_002_FILE_NOT_FOUND: &str = "MAG-QRY-002";

/// Invalid query parameters
pub const MAG_QRY_003_INVALID_PARAMS: &str = "MAG-QRY-003";

/// File not found on filesystem
pub const MAG_IO_001_FILE_NOT_FOUND: &str = "MAG-IO-001";

/// Permission denied
pub const MAG_IO_002_PERMISSION_DENIED: &str = "MAG-IO-002";

/// Invalid file path
pub const MAG_IO_003_INVALID_PATH: &str = "MAG-IO-003";

/// Checksum mismatch
pub const MAG_V_001_CHECKSUM_MISMATCH: &str = "MAG-V-001";

/// Span validation failed
pub const MAG_V_002_SPAN_INVALID: &str = "MAG-V-002";

/// Database corruption detected
pub const MAG_V_003_DB_CORRUPTION: &str = "MAG-V-003";

/// Error code documentation
///
/// # Reference Errors (MAG-REF-*)
///
/// | Code | Description | Remediation |
/// |------|-------------|-------------|
/// | MAG-REF-001 | Symbol not found | Verify symbol name and file path; use `magellan find` to search |
/// | MAG-REF-002 | Ambiguous symbol | Use fully-qualified name or specify file path |
/// | MAG-REF-003 | Invalid span | Check byte offsets are within file bounds and start < end |
///
/// # Query Errors (MAG-QRY-*)
///
/// | Code | Description | Remediation |
/// |------|-------------|-------------|
/// | MAG-QRY-001 | Invalid query syntax | Check query format; see command help |
/// | MAG-QRY-002 | File not found in database | Run `magellan watch` or `magellan verify` |
/// | MAG-QRY-003 | Invalid parameters | Check required arguments for command |
///
/// # I/O Errors (MAG-IO-*)
///
/// | Code | Description | Remediation |
/// |------|-------------|-------------|
/// | MAG-IO-001 | File not found on filesystem | Check file path and permissions |
/// | MAG-IO-002 | Permission denied | Check file/directory read permissions |
/// | MAG-IO-003 | Invalid path | Verify path format and escaping |
///
/// # Validation Errors (MAG-V-*)
///
/// | Code | Description | Remediation |
/// |------|-------------|-------------|
/// | MAG-V-001 | Checksum mismatch | Re-index the file; data may be corrupted |
/// | MAG-V-002 | Span validation failed | Re-index; file may have changed |
/// | MAG-V-003 | Database corruption | Re-build database from source |
pub const ERROR_CODE_DOCUMENTATION: &str = "Error code documentation available in source";

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify all error codes are unique
    #[test]
    fn test_error_codes_are_unique() {
        let codes = vec![
            MAG_REF_001_SYMBOL_NOT_FOUND,
            MAG_REF_002_AMBIGUOUS_SYMBOL,
            MAG_REF_003_INVALID_SPAN,
            MAG_QRY_001_INVALID_QUERY,
            MAG_QRY_002_FILE_NOT_FOUND,
            MAG_QRY_003_INVALID_PARAMS,
            MAG_IO_001_FILE_NOT_FOUND,
            MAG_IO_002_PERMISSION_DENIED,
            MAG_IO_003_INVALID_PATH,
            MAG_V_001_CHECKSUM_MISMATCH,
            MAG_V_002_SPAN_INVALID,
            MAG_V_003_DB_CORRUPTION,
        ];

        let mut unique = std::collections::HashSet::new();
        for code in codes {
            assert!(
                unique.insert(code),
                "Duplicate error code detected: {}",
                code
            );
        }
    }

    /// Verify error code format
    #[test]
    fn test_error_code_format() {
        let codes = vec![
            MAG_REF_001_SYMBOL_NOT_FOUND,
            MAG_REF_002_AMBIGUOUS_SYMBOL,
            MAG_REF_003_INVALID_SPAN,
            MAG_QRY_001_INVALID_QUERY,
            MAG_QRY_002_FILE_NOT_FOUND,
            MAG_QRY_003_INVALID_PARAMS,
            MAG_IO_001_FILE_NOT_FOUND,
            MAG_IO_002_PERMISSION_DENIED,
            MAG_IO_003_INVALID_PATH,
            MAG_V_001_CHECKSUM_MISMATCH,
            MAG_V_002_SPAN_INVALID,
            MAG_V_003_DB_CORRUPTION,
        ];

        for code in codes {
            // Format: MAG-{CATEGORY}-{3-digit number}
            assert!(
                code.starts_with("MAG-"),
                "Error code must start with 'MAG-': {}",
                code
            );
            let parts: Vec<&str> = code.split('-').collect();
            assert_eq!(parts.len(), 3, "Error code must have 3 parts: {}", code);

            // Verify category is 1-3 uppercase letters
            assert!(
                parts[1].len() >= 1 && parts[1].len() <= 3,
                "Category must be 1-3 chars: {}",
                code
            );
            assert!(parts[1].chars().all(|c| c.is_ascii_uppercase()));

            // Verify number is 3 digits
            assert_eq!(parts[2].len(), 3, "Number must be 3 digits: {}", code);
            assert!(parts[2].chars().all(|c| c.is_ascii_digit()));
        }
    }
}
