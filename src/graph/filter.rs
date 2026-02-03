//! File filtering for gitignore-style rules and CLI include/exclude globs.
//!
//! Provides deterministic file filtering with the following precedence:
//! 1. Hard internal ignores (db files, .git/, target/, etc.)
//! 2. Gitignore-style rules (.gitignore, .ignore)
//! 3. CLI include patterns (if any provided)
//! 4. CLI exclude patterns
//!
//! All filtering is pure function: same inputs always produce same output.

use anyhow::Result;
use ignore::gitignore::Gitignore;
use std::path::{Path, PathBuf};

use crate::diagnostics::{SkipReason, WatchDiagnostic};
use crate::ingest::detect_language;

/// Internal directories that are always ignored (hard-coded).
const INTERNAL_IGNORE_DIRS: &[&str] = &[
    ".git",
    ".codemcp",
    "target",
    "node_modules",
    ".venv",
    "venv",
    "__pycache__",
];

/// File extensions that are always ignored (hard-coded).
const INTERNAL_IGNORE_EXTS: &[&str] = &[
    ".db",
    ".db-journal",
    ".db-wal",
    ".db-shm",
    ".sqlite",
    ".sqlite3",
];

/// Filter configuration for scanning/watching.
///
/// Contains all filtering state in one place for deterministic behavior.
pub struct FileFilter {
    /// Root directory for path normalization
    root: PathBuf,
    /// Gitignore-style matcher (compiled from .gitignore/.ignore files)
    gitignore: Option<Gitignore>,
    /// CLI include patterns (empty = include all)
    include_patterns: Vec<globset::GlobMatcher>,
    /// CLI exclude patterns
    exclude_patterns: Vec<globset::GlobMatcher>,
}

impl FileFilter {
    /// Create a new filter for the given root directory.
    ///
    /// # Arguments
    /// * `root` - Root directory for path normalization
    /// * `include_patterns` - Optional CLI include globs (empty = include all)
    /// * `exclude_patterns` - CLI exclude globs
    ///
    /// # Returns
    /// A new FileFilter ready for use
    pub fn new(
        root: &Path,
        include_patterns: &[String],
        exclude_patterns: &[String],
    ) -> Result<Self> {
        // Use absolute path if possible, but don't fail if path doesn't exist
        let root = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());

        // Compile gitignore rules from .gitignore and .ignore files
        let gitignore = Self::load_gitignore(&root)?;

        // Compile include patterns
        let include_matchers = if include_patterns.is_empty() {
            Vec::new()
        } else {
            Self::compile_globs(&root, include_patterns)?
        };

        // Compile exclude patterns
        let exclude_matchers = Self::compile_globs(&root, exclude_patterns)?;

        Ok(Self {
            root,
            gitignore,
            include_patterns: include_matchers,
            exclude_patterns: exclude_matchers,
        })
    }

    /// Load gitignore-style rules from .gitignore and .ignore files.
    fn load_gitignore(root: &Path) -> Result<Option<Gitignore>> {
        let mut builder = ignore::gitignore::GitignoreBuilder::new(root);

        // Add .gitignore if it exists
        let gitignore_path = root.join(".gitignore");
        if gitignore_path.exists() {
            // The builder.add() returns Option<Error> - Some(Error) if failed
            if let Some(err) = builder.add(&gitignore_path) {
                // Log but don't fail - malformed gitignore shouldn't crash indexing
                eprintln!("Warning: Failed to load .gitignore: {}", err);
            }
        }

        // Add .ignore if it exists (ripgrep-style user ignores)
        let ignore_path = root.join(".ignore");
        if ignore_path.exists() {
            if let Some(err) = builder.add(&ignore_path) {
                eprintln!("Warning: Failed to load .ignore: {}", err);
            }
        }

        // Build the matcher (always succeeds, even with no rules)
        Ok(Some(builder.build()?))
    }

    /// Compile glob patterns into matchers.
    fn compile_globs(_root: &Path, patterns: &[String]) -> Result<Vec<globset::GlobMatcher>> {
        let mut matchers = Vec::new();

        for pattern in patterns {
            // Convert pattern to GlobSet-compatible format
            // Patterns are relative to root with / separators
            let glob = match globset::Glob::new(pattern) {
                Ok(g) => g,
                Err(e) => {
                    return Err(anyhow::anyhow!("Invalid glob pattern '{}': {}", pattern, e));
                }
            };
            matchers.push(glob.compile_matcher());
        }

        Ok(matchers)
    }

    /// Check if a path should be skipped, returning the reason if so.
    ///
    /// This is the main filtering function. It checks rules in precedence order
    /// and returns the first applicable skip reason.
    ///
    /// # Arguments
    /// * `path` - Full path to check
    ///
    /// # Returns
    /// * `None` - Path should be processed
    /// * `Some(reason)` - Path should be skipped
    pub fn should_skip(&self, path: &Path) -> Option<SkipReason> {
        // 1. Check if it's a regular file
        if !path.is_file() {
            return Some(SkipReason::NotAFile);
        }

        // 2. Internal hard-coded ignores (always first)
        if self.is_internal_ignore(path) {
            return Some(SkipReason::IgnoredInternal);
        }

        // 3. Gitignore-style rules
        if let Some(ref gitignore) = self.gitignore {
            // The ignore crate needs paths relative to the root for matching
            // Try to make the path relative, fall back to absolute if it fails
            let check_path = if let Ok(rel) = path.strip_prefix(&self.root) {
                rel
            } else {
                // Try with canonicalized root
                if let Ok(canonical_root) = std::fs::canonicalize(&self.root) {
                    if let Ok(rel) = path.strip_prefix(&canonical_root) {
                        rel
                    } else {
                        path
                    }
                } else {
                    path
                }
            };

            // First check if the file itself is ignored
            let is_ignored = gitignore.matched(check_path, path.is_dir());
            if is_ignored.is_ignore() {
                return Some(SkipReason::IgnoredByGitignore);
            }

            // Also check if any parent directory is ignored
            // This handles patterns like "build/" which should match files under build/
            // We need to check all ancestor paths to catch directory ignores
            let mut current = check_path.parent();
            while let Some(ancestor) = current {
                let ancestor_ignored = gitignore.matched(ancestor, true);
                if ancestor_ignored.is_ignore() {
                    return Some(SkipReason::IgnoredByGitignore);
                }
                current = ancestor.parent();
                // Break if we've reached the root (empty path)
                if ancestor.as_os_str().is_empty() {
                    break;
                }
            }
        }

        // 4. Check if language is supported
        if detect_language(path).is_none() {
            return Some(SkipReason::UnsupportedLanguage);
        }

        // 5. CLI include patterns (if any provided)
        if !self.include_patterns.is_empty() {
            let rel_path = self.relative_path(path);
            let matches_include = self.include_patterns.iter().any(|m| m.is_match(&rel_path));

            if !matches_include {
                return Some(SkipReason::ExcludedByGlob);
            }
        }

        // 6. CLI exclude patterns
        if !self.exclude_patterns.is_empty() {
            let rel_path = self.relative_path(path);
            if self.exclude_patterns.iter().any(|m| m.is_match(&rel_path)) {
                return Some(SkipReason::ExcludedByGlob);
            }
        }

        // Passed all filters
        None
    }

    /// Check if a path matches internal ignore rules.
    fn is_internal_ignore(&self, path: &Path) -> bool {
        // Check if it's a database file by looking at the full filename
        if let Some(file_name) = path.file_name() {
            let file_name_str = file_name.to_string_lossy();
            for ext in INTERNAL_IGNORE_EXTS {
                if file_name_str.ends_with(ext) {
                    return true;
                }
            }
        }

        // Also check extension (for cases like .db)
        if let Some(ext) = path.extension() {
            let ext_str = ext.to_string_lossy();
            if INTERNAL_IGNORE_EXTS.contains(&ext_str.as_ref()) {
                return true;
            }
        }

        // Check if path contains any ignored directory
        if let Ok(rel_path) = path.strip_prefix(&self.root) {
            for component in rel_path.components() {
                if let std::path::Component::Normal(dir) = component {
                    let dir_str = dir.to_string_lossy();
                    if INTERNAL_IGNORE_DIRS.contains(&dir_str.as_ref()) {
                        return true;
                    }
                }
            }
        }

        false
    }

    /// Get path relative to root, with forward slashes.
    fn relative_path(&self, path: &Path) -> String {
        path.strip_prefix(&self.root)
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_else(|_| path.to_string_lossy().into_owned())
    }

    /// Check if a specific path is the database file itself.
    ///
    /// This is a special case to avoid watching the database we're writing to.
    pub fn is_database_file(&self, path: &Path) -> bool {
        let path_str = path.to_string_lossy();
        let path_lower = path_str.to_lowercase();
        path_lower.ends_with(".db")
            || path_lower.ends_with(".db-journal")
            || path_lower.ends_with(".db-wal")
            || path_lower.ends_with(".db-shm")
            || path_lower.ends_with(".sqlite")
            || path_lower.ends_with(".sqlite3")
    }
}

/// Create a diagnostic for a skipped file.
pub fn skip_diagnostic(root: &Path, path: &Path, reason: SkipReason) -> WatchDiagnostic {
    let rel_path = path
        .strip_prefix(root)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| path.to_string_lossy().into_owned());

    WatchDiagnostic::skipped(rel_path, reason)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_internal_ignore_dirs() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        // Create filter with no patterns
        let filter = FileFilter::new(root, &[], &[]).unwrap();

        // Create internal directories
        fs::create_dir_all(root.join(".git")).unwrap();
        fs::create_dir_all(root.join("target")).unwrap();
        fs::create_dir_all(root.join("node_modules")).unwrap();

        // Create files in internal directories
        fs::write(root.join(".git/config"), "test").unwrap();
        fs::write(root.join("target/lib.rs"), "fn test() {}").unwrap();
        fs::write(root.join("node_modules/index.js"), "test").unwrap();

        // All should be skipped
        assert_eq!(
            filter.should_skip(&root.join(".git/config")),
            Some(SkipReason::IgnoredInternal)
        );
        assert_eq!(
            filter.should_skip(&root.join("target/lib.rs")),
            Some(SkipReason::IgnoredInternal)
        );
        assert_eq!(
            filter.should_skip(&root.join("node_modules/index.js")),
            Some(SkipReason::IgnoredInternal)
        );
    }

    #[test]
    fn test_internal_ignore_extensions() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();
        let filter = FileFilter::new(root, &[], &[]).unwrap();

        // Create database files
        fs::write(root.join("test.db"), "data").unwrap();
        fs::write(root.join("test.sqlite"), "data").unwrap();
        fs::write(root.join("test.db-journal"), "data").unwrap();

        // All should be skipped
        assert_eq!(
            filter.should_skip(&root.join("test.db")),
            Some(SkipReason::IgnoredInternal)
        );
        assert_eq!(
            filter.should_skip(&root.join("test.sqlite")),
            Some(SkipReason::IgnoredInternal)
        );
    }

    #[test]
    fn test_unsupported_language() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();
        let filter = FileFilter::new(root, &[], &[]).unwrap();

        // Create unsupported language files
        fs::write(root.join("test.txt"), "text").unwrap();
        fs::write(root.join("Makefile"), "all:").unwrap();

        // Should be skipped as unsupported language
        assert_eq!(
            filter.should_skip(&root.join("test.txt")),
            Some(SkipReason::UnsupportedLanguage)
        );
        assert_eq!(
            filter.should_skip(&root.join("Makefile")),
            Some(SkipReason::UnsupportedLanguage)
        );
    }

    #[test]
    fn test_supported_language_not_skipped() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();
        let filter = FileFilter::new(root, &[], &[]).unwrap();

        // Create supported language files
        fs::write(root.join("test.rs"), "fn test() {}").unwrap();
        fs::write(root.join("test.py"), "def test(): pass").unwrap();

        // Should NOT be skipped
        assert_eq!(filter.should_skip(&root.join("test.rs")), None);
        assert_eq!(filter.should_skip(&root.join("test.py")), None);
    }

    #[test]
    fn test_gitignore_filtering() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        // Create .gitignore first
        fs::write(root.join(".gitignore"), "ignored.rs\nbuild/\n").unwrap();

        // Create files
        fs::write(root.join("ignored.rs"), "fn test() {}").unwrap();
        fs::write(root.join("included.rs"), "fn test() {}").unwrap();
        fs::create_dir_all(root.join("build")).unwrap();
        fs::write(root.join("build/output.rs"), "fn test() {}").unwrap();

        // Create filter AFTER files exist (gitignore reads from filesystem)
        let filter = FileFilter::new(root, &[], &[]).unwrap();

        // Debug: check what the gitignore thinks
        // The build/ directory should cause files under it to be ignored
        // Let's just verify the file-level ignore works
        assert_eq!(
            filter.should_skip(&root.join("ignored.rs")),
            Some(SkipReason::IgnoredByGitignore),
            "ignored.rs should be ignored by .gitignore pattern"
        );

        // included.rs should NOT be skipped
        assert_eq!(
            filter.should_skip(&root.join("included.rs")),
            None,
            "included.rs should not be ignored"
        );

        // build/output.rs should be skipped by build/ pattern
        assert_eq!(
            filter.should_skip(&root.join("build/output.rs")),
            Some(SkipReason::IgnoredByGitignore),
            "build/output.rs should be ignored by build/ pattern"
        );
    }

    #[test]
    fn test_include_patterns() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        // Create directories and files
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(root.join("tests")).unwrap();
        fs::write(root.join("src/lib.rs"), "fn test() {}").unwrap();
        fs::write(root.join("tests/test.rs"), "fn test() {}").unwrap();
        fs::write(root.join("main.rs"), "fn main() {}").unwrap();

        // Filter to only include src/**
        let filter = FileFilter::new(root, &["src/**".to_string()], &[]).unwrap();

        // src/lib.rs should NOT be skipped
        assert_eq!(filter.should_skip(&root.join("src/lib.rs")), None);

        // tests/test.rs should be skipped (not in include)
        assert_eq!(
            filter.should_skip(&root.join("tests/test.rs")),
            Some(SkipReason::ExcludedByGlob)
        );

        // main.rs should be skipped (not in include)
        assert_eq!(
            filter.should_skip(&root.join("main.rs")),
            Some(SkipReason::ExcludedByGlob)
        );
    }

    #[test]
    fn test_exclude_patterns() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        // Create directories and files
        fs::create_dir_all(root.join("src")).unwrap();
        fs::write(root.join("src/lib.rs"), "fn test() {}").unwrap();
        fs::write(root.join("src/test.rs"), "fn test() {}").unwrap();

        // Exclude test files
        let filter = FileFilter::new(root, &[], &["**/*test*.rs".to_string()]).unwrap();

        // src/lib.rs should NOT be skipped
        assert_eq!(filter.should_skip(&root.join("src/lib.rs")), None);

        // src/test.rs should be skipped
        assert_eq!(
            filter.should_skip(&root.join("src/test.rs")),
            Some(SkipReason::ExcludedByGlob)
        );
    }

    #[test]
    fn test_include_and_exclude_patterns() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        // Create files
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(root.join("tests")).unwrap();
        fs::write(root.join("src/lib.rs"), "fn test() {}").unwrap();
        fs::write(root.join("src/test.rs"), "fn test() {}").unwrap();
        fs::write(root.join("tests/integration.rs"), "fn test() {}").unwrap();

        // Include only src/**, exclude test files
        let filter =
            FileFilter::new(root, &["src/**".to_string()], &["**/*test*.rs".to_string()]).unwrap();

        // src/lib.rs should NOT be skipped
        assert_eq!(filter.should_skip(&root.join("src/lib.rs")), None);

        // src/test.rs should be skipped (matches exclude)
        assert_eq!(
            filter.should_skip(&root.join("src/test.rs")),
            Some(SkipReason::ExcludedByGlob)
        );

        // tests/integration.rs should be skipped (not in include)
        assert_eq!(
            filter.should_skip(&root.join("tests/integration.rs")),
            Some(SkipReason::ExcludedByGlob)
        );
    }

    #[test]
    fn test_is_database_file() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();
        let filter = FileFilter::new(root, &[], &[]).unwrap();

        assert!(filter.is_database_file(Path::new("test.db")));
        assert!(filter.is_database_file(Path::new("test.sqlite")));
        assert!(filter.is_database_file(Path::new("test.db-journal")));
        assert!(!filter.is_database_file(Path::new("test.rs")));
        assert!(!filter.is_database_file(Path::new("database.rs")));
    }

    #[test]
    fn test_skip_diagnostic() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path();

        let diagnostic = skip_diagnostic(
            root,
            &root.join("target/lib.rs"),
            SkipReason::IgnoredInternal,
        );

        assert_eq!(diagnostic.path(), "target/lib.rs");
    }
}
