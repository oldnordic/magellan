//! Fully-Qualified Name builder for canonical and display FQN computation
//!
//! This module provides the FqnBuilder struct for computing both canonical (full identity)
//! and display (human-readable) fully-qualified names for symbols.
//!
//! # Canonical FQN
//! Format: `crate_name::file_path::Kind symbol_name`
//! Example: `magellan::src/lib.rs::Function my_function`
//! Purpose: Unambiguous identity for symbols, resolves same-name symbols in different files
//!
//! # Display FQN
//! Format: `crate_name::module_path::type_name::method_name`
//! Example: `magellan::my_module::MyStruct::my_method`
//! Purpose: Human-readable form for CLI output and user interaction

use crate::validation::normalize_path;
use crate::ingest::{ScopeSeparator, ScopeStack, SymbolKind};
use std::path::Path;

/// Builder for computing canonical and display FQN for symbols
///
/// The FqnBuilder encapsulates the logic for computing both identity (canonical)
/// and human-readable (display) fully-qualified names for symbols extracted from
/// source code.
///
/// # Design
///
/// - Canonical FQN includes file path for unambiguous identity
/// - Display FQN excludes file path for readability
/// - impl block handling: "impl " prefix is kept in ScopeStack for display,
///   stripped for canonical FQN's kind field
///
/// # Examples
///
/// ```rust
/// use magellan::graph::canonical_fqn::FqnBuilder;
/// use magellan::ingest::{ScopeSeparator, ScopeStack, SymbolKind};
///
/// let mut scope_stack = ScopeStack::new(ScopeSeparator::DoubleColon);
/// scope_stack.push("my_module");
/// scope_stack.push("MyStruct");
///
/// let builder = FqnBuilder::new(
///     "my_crate".to_string(),
///     "src/lib.rs".to_string(),
///     ScopeSeparator::DoubleColon,
/// );
///
/// let canonical = builder.canonical(&scope_stack, SymbolKind::Method, "my_method");
/// assert_eq!(canonical, "my_crate::src/lib.rs::Method my_method");
///
/// let display = builder.display(&scope_stack, SymbolKind::Method, "my_method");
/// assert_eq!(display, "my_crate::my_module::MyStruct::my_method");
/// ```
#[derive(Debug, Clone)]
pub struct FqnBuilder {
    /// Name of the crate (e.g., "magellan", "serde")
    pub(crate) crate_name: String,
    /// Path to the file containing the symbol (e.g., "src/lib.rs")
    pub(crate) file_path: String,
    /// Separator for this language (:: for Rust/C/C++, . for Python/Java/JS/TS)
    pub(crate) scope_separator: ScopeSeparator,
}

impl FqnBuilder {
    /// Create a new FqnBuilder for a file
    ///
    /// # Arguments
    /// * `crate_name` - Name of the crate/package
    /// * `file_path` - Path to the file (relative or absolute)
    /// * `scope_separator` - Separator for this language (:: or .)
    ///
    /// # Examples
    ///
    /// ```rust
    /// use magellan::graph::canonical_fqn::FqnBuilder;
    /// use magellan::ingest::ScopeSeparator;
    ///
    /// let builder = FqnBuilder::new(
    ///     "my_crate".to_string(),
    ///     "src/lib.rs".to_string(),
    ///     ScopeSeparator::DoubleColon,
    /// );
    /// ```
    pub fn new(crate_name: String, file_path: String, scope_separator: ScopeSeparator) -> Self {
        Self {
            crate_name,
            file_path,
            scope_separator,
        }
    }

    /// Compute canonical FQN (full identity with file path)
    ///
    /// Format: `crate_name::file_path::Kind symbol_name`
    ///
    /// The canonical FQN includes the file path to ensure unambiguous identity
    /// for symbols with the same name in different files.
    ///
    /// # Arguments
    /// * `scope_stack` - Current scope stack during parsing
    /// * `symbol_kind` - Kind of symbol (Function, Method, Class, etc.)
    /// * `symbol_name` - Name of the symbol
    ///
    /// # Returns
    /// Canonical FQN string
    ///
    /// # Examples
    ///
    /// ```rust
    /// use magellan::graph::canonical_fqn::FqnBuilder;
    /// use magellan::ingest::{ScopeSeparator, ScopeStack, SymbolKind};
    ///
    /// let scope_stack = ScopeStack::new(ScopeSeparator::DoubleColon);
    /// let builder = FqnBuilder::new(
    ///     "magellan".to_string(),
    ///     "src/lib.rs".to_string(),
    ///     ScopeSeparator::DoubleColon,
    /// );
    ///
    /// let canonical = builder.canonical(&scope_stack, SymbolKind::Function, "main");
    /// assert_eq!(canonical, "magellan::src/lib.rs::Function main");
    /// ```
    pub fn canonical(
        &self,
        _scope_stack: &ScopeStack,
        symbol_kind: SymbolKind,
        symbol_name: &str,
    ) -> String {
        let kind_str = self.kind_string(symbol_kind);
        format!(
            "{}::{}::{} {}",
            self.crate_name,
            self.file_path,
            kind_str,
            symbol_name
        )
    }

    /// Compute display FQN (human-readable shortened form)
    ///
    /// Format: `crate_name::module_path::type_name::method_name`
    ///
    /// The display FQN excludes the file path for readability. It includes:
    /// - Crate name for disambiguation
    /// - Scope chain (modules, types, impl blocks)
    /// - Symbol name
    ///
    /// For impl blocks, the "impl " prefix is stripped from the scope name
    /// to produce cleaner display output.
    ///
    /// # Arguments
    /// * `scope_stack` - Current scope stack during parsing
    /// * `symbol_kind` - Kind of symbol (Function, Method, Class, etc.)
    /// * `symbol_name` - Name of the symbol
    ///
    /// # Returns
    /// Display FQN string
    ///
    /// # Examples
    ///
    /// ```rust
    /// use magellan::graph::canonical_fqn::FqnBuilder;
    /// use magellan::ingest::{ScopeSeparator, ScopeStack, SymbolKind};
    ///
    /// let mut scope_stack = ScopeStack::new(ScopeSeparator::DoubleColon);
    /// scope_stack.push("my_module");
    /// scope_stack.push("MyStruct");
    ///
    /// let builder = FqnBuilder::new(
    ///     "magellan".to_string(),
    ///     "src/lib.rs".to_string(),
    ///     ScopeSeparator::DoubleColon,
    /// );
    ///
    /// let display = builder.display(&scope_stack, SymbolKind::Method, "my_method");
    /// assert_eq!(display, "magellan::my_module::MyStruct::my_method");
    /// ```
    pub fn display(
        &self,
        scope_stack: &ScopeStack,
        _symbol_kind: SymbolKind,
        symbol_name: &str,
    ) -> String {
        let sep = self.scope_separator.as_str();

        // Start with crate name for disambiguation
        let mut parts = vec![self.crate_name.clone()];

        // Add scope chain (modules, impl blocks, traits)
        for scope in scope_stack.scopes() {
            // Strip "impl " prefix for cleaner display
            let clean_scope = scope.strip_prefix("impl ").unwrap_or(scope);
            parts.push(clean_scope.to_string());
        }

        // Add symbol name
        parts.push(symbol_name.to_string());

        parts.join(sep)
    }

    /// Get normalized file path for hash input
    ///
    /// Uses `validation::normalize_path()` to ensure consistent path representation
    /// across platforms and invocations. This is critical for stable SymbolId generation.
    ///
    /// # Returns
    /// Normalized file path string, or error if path cannot be normalized
    ///
    /// # Examples
    ///
    /// ```rust
    /// use magellan::graph::canonical_fqn::FqnBuilder;
    /// use magellan::ingest::ScopeSeparator;
    ///
    /// let builder = FqnBuilder::new(
    ///     "my_crate".to_string(),
    ///     "./src/lib.rs".to_string(),
    ///     ScopeSeparator::DoubleColon,
    /// );
    ///
    /// let normalized = builder.normalized_file_path().unwrap();
    /// // Path is normalized: ./ prefix stripped, absolute if exists
    /// assert!(!normalized.contains("./"));
    /// ```
    pub fn normalized_file_path(&self) -> Result<String, String> {
        normalize_path(Path::new(&self.file_path))
            .map_err(|e| format!("Path normalization failed: {}", e))
    }

    /// Get the kind string for canonical FQN
    ///
    /// Converts SymbolKind to a readable string for the canonical FQN format.
    fn kind_string(&self, kind: SymbolKind) -> String {
        match kind {
            SymbolKind::Function => "Function",
            SymbolKind::Method => "Method",
            SymbolKind::Class => "Struct",
            SymbolKind::Interface => "Trait",
            SymbolKind::Enum => "Enum",
            SymbolKind::Module => "Module",
            SymbolKind::Union => "Union",
            SymbolKind::Namespace => "Namespace",
            SymbolKind::TypeAlias => "TypeAlias",
            SymbolKind::Unknown => "Unknown",
        }
        .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_builder() -> FqnBuilder {
        FqnBuilder::new(
            "test_crate".to_string(),
            "src/test.rs".to_string(),
            ScopeSeparator::DoubleColon,
        )
    }

    #[test]
    fn test_canonical_fqn_format() {
        let builder = create_test_builder();
        let scope_stack = ScopeStack::new(ScopeSeparator::DoubleColon);

        let canonical = builder.canonical(&scope_stack, SymbolKind::Function, "test_fn");

        assert_eq!(canonical, "test_crate::src/test.rs::Function test_fn");
    }

    #[test]
    fn test_display_fqn_format() {
        let builder = create_test_builder();
        let scope_stack = ScopeStack::new(ScopeSeparator::DoubleColon);

        let display = builder.display(&scope_stack, SymbolKind::Function, "test_fn");

        assert_eq!(display, "test_crate::test_fn");
    }

    #[test]
    fn test_display_fqn_with_scope() {
        let builder = create_test_builder();
        let mut scope_stack = ScopeStack::new(ScopeSeparator::DoubleColon);
        scope_stack.push("my_module");

        let display = builder.display(&scope_stack, SymbolKind::Function, "test_fn");

        assert_eq!(display, "test_crate::my_module::test_fn");
    }

    #[test]
    fn test_impl_block_handling() {
        let builder = create_test_builder();
        let mut scope_stack = ScopeStack::new(ScopeSeparator::DoubleColon);
        scope_stack.push("MyStruct");

        // Display FQN should show clean struct name
        let display = builder.display(&scope_stack, SymbolKind::Method, "my_method");
        assert_eq!(display, "test_crate::MyStruct::my_method");
    }

    #[test]
    fn test_impl_block_with_prefix() {
        let builder = create_test_builder();
        let mut scope_stack = ScopeStack::new(ScopeSeparator::DoubleColon);
        scope_stack.push("impl MyStruct");

        // Display FQN should strip "impl " prefix
        let display = builder.display(&scope_stack, SymbolKind::Method, "my_method");
        assert_eq!(display, "test_crate::MyStruct::my_method");
    }

    #[test]
    fn test_nested_scope() {
        let builder = create_test_builder();
        let mut scope_stack = ScopeStack::new(ScopeSeparator::DoubleColon);
        scope_stack.push("outer_module");
        scope_stack.push("inner_module");
        scope_stack.push("MyStruct");

        let canonical = builder.canonical(&scope_stack, SymbolKind::Method, "my_method");
        let display = builder.display(&scope_stack, SymbolKind::Method, "my_method");

        assert_eq!(
            canonical,
            "test_crate::src/test.rs::Method my_method"
        );
        assert_eq!(
            display,
            "test_crate::outer_module::inner_module::MyStruct::my_method"
        );
    }

    #[test]
    fn test_trait_impl_scope() {
        let builder = create_test_builder();
        let mut scope_stack = ScopeStack::new(ScopeSeparator::DoubleColon);
        scope_stack.push("MyTrait");
        scope_stack.push("impl MyStruct");

        // For trait impls, display FQN shows trait then struct
        let display = builder.display(&scope_stack, SymbolKind::Method, "trait_method");
        assert_eq!(display, "test_crate::MyTrait::MyStruct::trait_method");
    }

    #[test]
    fn test_normalized_file_path() {
        let builder = FqnBuilder::new(
            "test_crate".to_string(),
            "./src/lib.rs".to_string(),
            ScopeSeparator::DoubleColon,
        );

        let normalized = builder.normalized_file_path().unwrap();
        // Should strip ./ prefix
        assert!(!normalized.starts_with("./"));
        assert!(normalized.contains("src/lib.rs"));
    }

    #[test]
    fn test_normalized_file_path_absolute() {
        let builder = FqnBuilder::new(
            "test_crate".to_string(),
            "/absolute/path/to/file.rs".to_string(),
            ScopeSeparator::DoubleColon,
        );

        let normalized = builder.normalized_file_path().unwrap();
        // Absolute path should be returned (file doesn't exist, so no canonicalization)
        assert!(normalized.contains("file.rs"));
    }

    #[test]
    fn test_empty_scope_stack() {
        let builder = create_test_builder();
        let scope_stack = ScopeStack::new(ScopeSeparator::DoubleColon);

        let canonical = builder.canonical(&scope_stack, SymbolKind::Function, "top_level");
        let display = builder.display(&scope_stack, SymbolKind::Function, "top_level");

        assert_eq!(canonical, "test_crate::src/test.rs::Function top_level");
        assert_eq!(display, "test_crate::top_level");
    }

    #[test]
    fn test_dot_separator() {
        let builder = FqnBuilder::new(
            "com.example".to_string(),
            "src/Example.java".to_string(),
            ScopeSeparator::Dot,
        );
        let mut scope_stack = ScopeStack::new(ScopeSeparator::Dot);
        scope_stack.push("mypackage");
        scope_stack.push("MyClass");

        let display = builder.display(&scope_stack, SymbolKind::Method, "myMethod");
        assert_eq!(display, "com.example.mypackage.MyClass.myMethod");

        let canonical = builder.canonical(&scope_stack, SymbolKind::Method, "myMethod");
        assert_eq!(canonical, "com.example::src/Example.java::Method myMethod");
    }

    #[test]
    fn test_deeply_nested_scope() {
        let builder = create_test_builder();
        let mut scope_stack = ScopeStack::new(ScopeSeparator::DoubleColon);
        scope_stack.push("a");
        scope_stack.push("b");
        scope_stack.push("c");
        scope_stack.push("MyStruct");
        scope_stack.push("impl MyStruct");

        let display = builder.display(&scope_stack, SymbolKind::Method, "nested_method");
        assert_eq!(
            display,
            "test_crate::a::b::c::MyStruct::MyStruct::nested_method"
        );
    }

    #[test]
    fn test_kind_strings() {
        let builder = create_test_builder();
        let scope_stack = ScopeStack::new(ScopeSeparator::DoubleColon);

        assert_eq!(
            builder.canonical(&scope_stack, SymbolKind::Function, "f"),
            "test_crate::src/test.rs::Function f"
        );
        assert_eq!(
            builder.canonical(&scope_stack, SymbolKind::Method, "m"),
            "test_crate::src/test.rs::Method m"
        );
        assert_eq!(
            builder.canonical(&scope_stack, SymbolKind::Class, "S"),
            "test_crate::src/test.rs::Struct S"
        );
        assert_eq!(
            builder.canonical(&scope_stack, SymbolKind::Interface, "T"),
            "test_crate::src/test.rs::Trait T"
        );
        assert_eq!(
            builder.canonical(&scope_stack, SymbolKind::Enum, "E"),
            "test_crate::src/test.rs::Enum E"
        );
        assert_eq!(
            builder.canonical(&scope_stack, SymbolKind::Module, "mod"),
            "test_crate::src/test.rs::Module mod"
        );
        assert_eq!(
            builder.canonical(&scope_stack, SymbolKind::Union, "U"),
            "test_crate::src/test.rs::Union U"
        );
        assert_eq!(
            builder.canonical(&scope_stack, SymbolKind::Namespace, "ns"),
            "test_crate::src/test.rs::Namespace ns"
        );
        assert_eq!(
            builder.canonical(&scope_stack, SymbolKind::TypeAlias, "TA"),
            "test_crate::src/test.rs::TypeAlias TA"
        );
    }
}
