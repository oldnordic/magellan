//! Watch diagnostics for structured skip reasons and error reporting.
//!
//! Phase 2 scope:
//! - Structured types for skip reasons and errors
//! - Deterministic ordering via sort_key()
//! - Human-readable stderr output
//!
//! Phase 3 scope (deferred):
//! - JSON serialization with schema_version
//! - Stable stdout/stderr discipline

use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::fmt;

/// Reason why a file was skipped during indexing.
///
/// Each variant represents a deterministic decision point in the filtering pipeline.
/// The order of variants matters for precedence when reporting skip reasons.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum SkipReason {
    /// File is not a regular file (directory, symlink, etc.)
    NotAFile,
    /// Language not supported by Magellan
    UnsupportedLanguage,
    /// Internal hard-coded ignore rules (db files, .git/, target/, etc.)
    IgnoredInternal,
    /// Matched by gitignore-style rules (.gitignore, .ignore)
    IgnoredByGitignore,
    /// Excluded by CLI --exclude glob pattern
    ExcludedByGlob,
}

impl SkipReason {
    /// Stable sort key for deterministic ordering.
    ///
    /// Lower values = higher priority in reporting.
    pub fn sort_key(&self) -> u8 {
        match self {
            SkipReason::IgnoredInternal => 0,     // Always first
            SkipReason::IgnoredByGitignore => 1,  // Then gitignore rules
            SkipReason::ExcludedByGlob => 2,      // Then CLI excludes
            SkipReason::UnsupportedLanguage => 3,  // Then language detection
            SkipReason::NotAFile => 4,            // Last
        }
    }

    /// Human-readable description for stderr output.
    pub fn description(&self) -> &'static str {
        match self {
            SkipReason::NotAFile => "not a regular file",
            SkipReason::UnsupportedLanguage => "language not supported",
            SkipReason::IgnoredInternal => "internal ignore rule",
            SkipReason::IgnoredByGitignore => "matched by gitignore",
            SkipReason::ExcludedByGlob => "excluded by pattern",
        }
    }
}

impl fmt::Display for SkipReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.description())
    }
}

impl PartialOrd for SkipReason {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SkipReason {
    fn cmp(&self, other: &Self) -> Ordering {
        // Compare by sort_key primarily
        self.sort_key().cmp(&other.sort_key())
    }
}

/// Stage in the indexing pipeline where an error occurred.
///
/// Used to distinguish where in the processing flow a failure happened.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum DiagnosticStage {
    /// Failed to read file from filesystem
    Read,
    /// Failed to parse source code
    Parse,
    /// Failed to index symbols
    IndexSymbols,
    /// Failed to index references
    IndexReferences,
    /// Failed to index calls
    IndexCalls,
    /// Other error not categorized above
    Other,
}

impl DiagnosticStage {
    /// Stable sort key for deterministic ordering.
    pub fn sort_key(&self) -> u8 {
        match self {
            DiagnosticStage::Read => 0,
            DiagnosticStage::Parse => 1,
            DiagnosticStage::IndexSymbols => 2,
            DiagnosticStage::IndexReferences => 3,
            DiagnosticStage::IndexCalls => 4,
            DiagnosticStage::Other => 5,
        }
    }

    /// Human-readable description for stderr output.
    pub fn description(&self) -> &'static str {
        match self {
            DiagnosticStage::Read => "reading file",
            DiagnosticStage::Parse => "parsing source",
            DiagnosticStage::IndexSymbols => "indexing symbols",
            DiagnosticStage::IndexReferences => "indexing references",
            DiagnosticStage::IndexCalls => "indexing calls",
            DiagnosticStage::Other => "processing",
        }
    }
}

impl fmt::Display for DiagnosticStage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.description())
    }
}

impl PartialOrd for DiagnosticStage {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for DiagnosticStage {
    fn cmp(&self, other: &Self) -> Ordering {
        self.sort_key().cmp(&other.sort_key())
    }
}

/// A diagnostic event from the watch/index pipeline.
///
/// Represents either a skipped file or a processing error.
/// Designed for deterministic sorting and eventual JSON output (Phase 3).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum WatchDiagnostic {
    /// File was skipped during scanning/watching
    Skipped {
        /// Path relative to root
        path: String,
        /// Why the file was skipped
        reason: SkipReason,
    },
    /// Error occurred while processing a file
    Error {
        /// Path relative to root
        path: String,
        /// Pipeline stage where error occurred
        stage: DiagnosticStage,
        /// Error message
        message: String,
    },
}

impl WatchDiagnostic {
    /// Get the file path for this diagnostic.
    pub fn path(&self) -> &str {
        match self {
            WatchDiagnostic::Skipped { path, .. } => path,
            WatchDiagnostic::Error { path, .. } => path,
        }
    }

    /// Stable sort key for deterministic ordering.
    ///
    /// Primary: path string (lexicographic)
    /// Secondary: variant type (Error before Skipped)
    /// Tertiary: stage/reason sort key
    pub fn sort_key(&self) -> (&str, u8, u8) {
        match self {
            WatchDiagnostic::Error { path, stage, .. } => (path, 0, stage.sort_key()),
            WatchDiagnostic::Skipped { path, reason } => (path, 1, reason.sort_key()),
        }
    }

    /// Create a Skipped diagnostic.
    pub fn skipped(path: String, reason: SkipReason) -> Self {
        WatchDiagnostic::Skipped { path, reason }
    }

    /// Create an Error diagnostic.
    pub fn error(path: String, stage: DiagnosticStage, message: String) -> Self {
        WatchDiagnostic::Error {
            path,
            stage,
            message,
        }
    }

    /// Format for human-readable stderr output (Phase 2).
    ///
    /// Examples:
    /// - "SKIP src/ignored.rs: internal ignore rule"
    /// - "ERROR src/bad.rs: parsing file: syntax error at line 5"
    pub fn format_stderr(&self) -> String {
        match self {
            WatchDiagnostic::Skipped { path, reason } => {
                format!("SKIP {}: {}", path, reason)
            }
            WatchDiagnostic::Error { path, stage, message } => {
                format!("ERROR {}: {}: {}", path, stage, message)
            }
        }
    }
}

impl fmt::Display for WatchDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.format_stderr())
    }
}

impl PartialOrd for WatchDiagnostic {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for WatchDiagnostic {
    fn cmp(&self, other: &Self) -> Ordering {
        // Use sort_key for deterministic ordering
        self.sort_key().cmp(&other.sort_key())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_skip_reason_sort_key() {
        assert!(SkipReason::IgnoredInternal.sort_key() < SkipReason::IgnoredByGitignore.sort_key());
        assert!(SkipReason::IgnoredByGitignore.sort_key() < SkipReason::ExcludedByGlob.sort_key());
    }

    #[test]
    fn test_skip_reason_ord() {
        // Lower sort_key should be "less" (comes first in ordering)
        assert!(SkipReason::IgnoredInternal < SkipReason::IgnoredByGitignore);
        assert!(SkipReason::IgnoredByGitignore < SkipReason::ExcludedByGlob);
    }

    #[test]
    fn test_diagnostic_stage_sort_key() {
        assert_eq!(DiagnosticStage::Read.sort_key(), 0);
        assert_eq!(DiagnosticStage::Parse.sort_key(), 1);
        assert_eq!(DiagnosticStage::IndexSymbols.sort_key(), 2);
    }

    #[test]
    fn test_diagnostic_stage_ord() {
        assert!(DiagnosticStage::Read < DiagnosticStage::Parse);
        assert!(DiagnosticStage::Parse < DiagnosticStage::IndexSymbols);
    }

    #[test]
    fn test_watch_diagnostic_skipped() {
        let diag = WatchDiagnostic::skipped(
            "src/test.rs".to_string(),
            SkipReason::UnsupportedLanguage,
        );

        assert_eq!(diag.path(), "src/test.rs");
        assert!(matches!(diag, WatchDiagnostic::Skipped { .. }));
    }

    #[test]
    fn test_watch_diagnostic_error() {
        let diag = WatchDiagnostic::error(
            "src/bad.rs".to_string(),
            DiagnosticStage::Parse,
            "unexpected token".to_string(),
        );

        assert_eq!(diag.path(), "src/bad.rs");
        assert!(matches!(diag, WatchDiagnostic::Error { .. }));
    }

    #[test]
    fn test_watch_diagnostic_sort_key() {
        let error = WatchDiagnostic::error(
            "src/a.rs".to_string(),
            DiagnosticStage::Parse,
            "error".to_string(),
        );
        let skipped = WatchDiagnostic::skipped(
            "src/a.rs".to_string(),
            SkipReason::UnsupportedLanguage,
        );

        // Same path, but Error (variant=0) comes before Skipped (variant=1)
        let error_key = error.sort_key();
        let skipped_key = skipped.sort_key();
        assert_eq!(error_key.0, skipped_key.0); // Same path
        assert!(error_key.1 < skipped_key.1); // Error before Skipped
    }

    #[test]
    fn test_watch_diagnostic_ord() {
        let d1 = WatchDiagnostic::skipped("src/b.rs".to_string(), SkipReason::ExcludedByGlob);
        let d2 = WatchDiagnostic::skipped("src/a.rs".to_string(), SkipReason::ExcludedByGlob);

        // Path ordering is primary
        assert!(d2 < d1); // "src/a.rs" < "src/b.rs"
    }

    #[test]
    fn test_format_stderr_skipped() {
        let diag = WatchDiagnostic::skipped("target/lib.rs".to_string(), SkipReason::IgnoredInternal);
        let formatted = diag.format_stderr();
        assert_eq!(formatted, "SKIP target/lib.rs: internal ignore rule");
    }

    #[test]
    fn test_format_stderr_error() {
        let diag = WatchDiagnostic::error(
            "src/bad.rs".to_string(),
            DiagnosticStage::Parse,
            "unexpected end of file".to_string(),
        );
        let formatted = diag.format_stderr();
        assert_eq!(formatted, "ERROR src/bad.rs: parsing source: unexpected end of file");
    }

    #[test]
    fn test_skip_reason_display() {
        assert_eq!(SkipReason::UnsupportedLanguage.to_string(), "language not supported");
        assert_eq!(SkipReason::IgnoredInternal.to_string(), "internal ignore rule");
    }

    #[test]
    fn test_diagnostic_stage_display() {
        assert_eq!(DiagnosticStage::Read.to_string(), "reading file");
        assert_eq!(DiagnosticStage::Parse.to_string(), "parsing source");
    }

    #[test]
    fn test_watch_diagnostic_sorting_vec() {
        let mut diagnostics = vec![
            WatchDiagnostic::skipped("src/c.rs".to_string(), SkipReason::ExcludedByGlob),
            WatchDiagnostic::error("src/a.rs".to_string(), DiagnosticStage::Read, "error".to_string()),
            WatchDiagnostic::skipped("src/b.rs".to_string(), SkipReason::IgnoredInternal),
        ];

        diagnostics.sort();

        // Sorted by path, then variant
        assert_eq!(diagnostics[0].path(), "src/a.rs"); // Error comes first
        assert_eq!(diagnostics[1].path(), "src/b.rs");
        assert_eq!(diagnostics[2].path(), "src/c.rs");
    }
}
