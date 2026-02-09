//! Module path resolution for Rust import statements
//!
//! Provides module resolution for crate::, super::, self:: prefixes.

use anyhow::Result;
use sqlitegraph::GraphBackend;
use std::path::PathBuf;
use std::rc::Rc;

use crate::graph::schema::ModulePathCache;

/// Module resolver for converting relative import paths to file IDs
///
/// Handles:
/// - `crate::` prefix (absolute path from crate root)
/// - `super::` prefix (relative to parent module)
/// - `self::` prefix (relative to current module)
/// - Plain paths (relative to current module or extern crate)
pub struct ModuleResolver {
    /// Graph backend for querying file nodes
    backend: Rc<dyn GraphBackend>,
    /// Module path cache for O(1) lookups
    cache: ModulePathCache,
    /// Project root path (for resolving relative file paths)
    project_root: PathBuf,
}

impl ModuleResolver {
    /// Create a new module resolver
    pub fn new(backend: Rc<dyn GraphBackend>, project_root: PathBuf) -> Self {
        let cache = ModulePathCache::new();
        Self {
            backend,
            cache,
            project_root,
        }
    }

    /// Build the module index from the database
    ///
    /// Scans all files and builds module path -> file_id mappings.
    /// Should be called after opening the database but before resolving paths.
    pub fn build_module_index(&mut self) -> Result<()> {
        self.cache = ModulePathCache::build_from_index(&self.backend);
        Ok(())
    }

    /// Resolve an import path to a file ID
    ///
    /// # Arguments
    /// * `current_file` - Path of the file containing the import
    /// * `import_path` - Import path components (e.g., ["crate", "foo", "bar"])
    ///
    /// # Returns
    /// - Some(file_id) if the module is found
    /// - None if the module cannot be resolved
    ///
    /// # Examples
    /// - resolve_path("src/foo/bar.rs", ["crate", "baz"]) -> Some(baz_file_id)
    /// - resolve_path("src/foo/bar.rs", ["super", "qux"]) -> Some(qux_file_id)
    /// - resolve_path("src/foo/bar.rs", ["self", "local"]) -> Some(local_file_id)
    pub fn resolve_path(&self, current_file: &str, import_path: &[String]) -> Option<i64> {
        if import_path.is_empty() {
            return None;
        }

        let first = import_path[0].as_str();

        match first {
            "crate" => {
                // Absolute path from crate root
                // crate::foo::bar -> "crate::foo::bar"
                let module_path = import_path.join("::");
                self.cache.get(&module_path)
            }
            "super" => {
                // Relative to parent module
                // super::foo -> "crate::parent::foo"
                let current_module = Self::file_path_to_module_path(current_file);
                let parent_module = Self::get_parent_module(&current_module)?;
                let relative_path: Vec<String> =
                    std::iter::once(parent_module)
                        .chain(import_path[1..].iter().cloned())
                        .collect();
                let module_path = relative_path.join("::");
                self.cache.get(&module_path)
            }
            "self" => {
                // Relative to current module
                // self::foo -> "crate::current::foo"
                let current_module = Self::file_path_to_module_path(current_file);
                let relative_path: Vec<String> =
                    std::iter::once(current_module)
                        .chain(import_path[1..].iter().cloned())
                        .collect();
                let module_path = relative_path.join("::");
                self.cache.get(&module_path)
            }
            _ => {
                // Plain path - try current module first, then extern crates
                // First try as crate:: path
                let module_path = format!("crate::{}", import_path.join("::"));
                if let Some(file_id) = self.cache.get(&module_path) {
                    return Some(file_id);
                }

                // Try as extern crate (not implemented in Phase 60)
                None
            }
        }
    }

    /// Get file ID for a module path (direct cache lookup)
    ///
    /// # Arguments
    /// * `module_path` - Module path (e.g., "crate::foo::bar")
    ///
    /// # Returns
    /// - Some(file_id) if the module is in cache
    /// - None if not found
    pub fn get_file_for_module(&self, module_path: &str) -> Option<i64> {
        self.cache.get(module_path)
    }

    /// Convert file path to module path
    ///
    /// Examples:
    /// - "src/lib.rs" -> "crate"
    /// - "src/foo.rs" -> "crate::foo"
    /// - "src/foo/bar.rs" -> "crate::foo::bar"
    /// - "src/foo/mod.rs" -> "crate::foo"
    fn file_path_to_module_path(file_path: &str) -> String {
        ModulePathCache::file_path_to_module_path(file_path)
    }

    /// Get parent module from module path
    ///
    /// Examples:
    /// - "crate::foo::bar" -> "crate::foo"
    /// - "crate::foo" -> "crate"
    /// - "crate" -> None (crate has no parent)
    fn get_parent_module(module_path: &str) -> Option<String> {
        if module_path == "crate" {
            return None;
        }

        // Find last "::" and return everything before it
        module_path.rfind("::").map(|pos| module_path[..pos].to_string())
    }

    /// Clear the module cache
    pub fn clear_cache(&mut self) {
        self.cache.clear();
    }

    /// Get cache size (for testing/debugging)
    pub fn cache_size(&self) -> usize {
        self.cache.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_file_path_to_module_path() {
        assert_eq!(ModuleResolver::file_path_to_module_path("src/lib.rs"), "crate");
        assert_eq!(
            ModuleResolver::file_path_to_module_path("src/main.rs"),
            "crate"
        );
        assert_eq!(
            ModuleResolver::file_path_to_module_path("src/foo.rs"),
            "crate::foo"
        );
        assert_eq!(
            ModuleResolver::file_path_to_module_path("src/foo/mod.rs"),
            "crate::foo"
        );
        assert_eq!(
            ModuleResolver::file_path_to_module_path("src/foo/bar.rs"),
            "crate::foo::bar"
        );
        assert_eq!(
            ModuleResolver::file_path_to_module_path("src/foo/bar/mod.rs"),
            "crate::foo::bar"
        );
    }

    #[test]
    fn test_get_parent_module() {
        assert_eq!(ModuleResolver::get_parent_module("crate"), None);
        assert_eq!(
            ModuleResolver::get_parent_module("crate::foo").as_deref(),
            Some("crate")
        );
        assert_eq!(
            ModuleResolver::get_parent_module("crate::foo::bar").as_deref(),
            Some("crate::foo")
        );
    }

    #[test]
    fn test_module_path_cache() {
        let mut cache = ModulePathCache::new();
        cache.insert("crate::foo".to_string(), 123);
        cache.insert("crate::bar".to_string(), 456);

        assert_eq!(cache.get("crate::foo"), Some(123));
        assert_eq!(cache.get("crate::bar"), Some(456));
        assert_eq!(cache.get("crate::baz"), None);

        cache.clear();
        assert_eq!(cache.get("crate::foo"), None);
    }

    #[test]
    fn test_module_resolver_creation() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let graph = crate::CodeGraph::open(&db_path).unwrap();

        let resolver = ModuleResolver::new(
            graph.files.backend.clone(),
            temp_dir.path().to_path_buf(),
        );

        assert_eq!(resolver.cache_size(), 0);

        // Build index
        let mut resolver = resolver;
        resolver.build_module_index().unwrap();

        // Should have at least 0 entries (no files indexed yet)
        assert_eq!(resolver.cache_size(), 0);
    }

    #[test]
    fn test_resolve_crate_path() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let mut graph = crate::CodeGraph::open(&db_path).unwrap();

        // Create some test files
        let test_file1 = temp_dir.path().join("src/lib.rs");
        std::fs::create_dir_all(test_file1.parent().unwrap()).unwrap();
        std::fs::write(&test_file1, b"fn lib() {}").unwrap();

        // Use relative path for indexing to match how real projects work
        let file1_relative = "src/lib.rs";
        graph.index_file(file1_relative, b"fn lib() {}").unwrap();

        let test_file2 = temp_dir.path().join("src/foo.rs");
        std::fs::write(&test_file2, b"fn foo() {}").unwrap();

        // Use relative path for indexing
        let file2_relative = "src/foo.rs";
        graph.index_file(file2_relative, b"fn foo() {}").unwrap();

        // Build module index
        let mut resolver = ModuleResolver::new(
            graph.files.backend.clone(),
            temp_dir.path().to_path_buf(),
        );
        resolver.build_module_index().unwrap();

        // Resolve crate::foo
        let foo_path = vec!["crate".to_string(), "foo".to_string()];
        let foo_id = resolver.resolve_path(file1_relative, &foo_path);
        assert!(foo_id.is_some(), "Should resolve crate::foo");
    }
}
