use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Represents a trait implementation relationship extracted from source.
///
/// For `impl Display for MyStruct`, this captures:
/// - `type_name`: "MyStruct"
/// - `trait_name`: "Display"
/// - Inherent impls (no trait) have `trait_name: None`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImplRelation {
    pub type_name: String,
    pub trait_name: Option<String>,
    pub file_path: String,
    pub byte_start: usize,
    pub byte_end: usize,
    pub start_line: usize,
    pub end_line: usize,
}

/// Kind of symbol extracted from source code
///
/// Language-agnostic symbol kinds that map across multiple programming languages.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SymbolKind {
    /// Function definition
    Function,
    /// Method inside a class/impl block
    Method,
    /// Class or struct-like type definition
    /// Covers: Rust struct, Python class, Java class, C++ class, JS/TS class
    Class,
    /// Interface or trait definition
    /// Covers: Rust trait, Java interface, TypeScript interface
    Interface,
    /// Enum definition
    Enum,
    /// Module or package declaration
    /// Covers: Rust mod, Python module, Java package, JS/TS module
    Module,
    /// Union definition (C/C++)
    Union,
    /// Namespace definition
    /// Covers: C++ namespace, TypeScript namespace
    Namespace,
    /// Type alias
    /// Covers: TypeScript type, Rust type alias
    TypeAlias,
    /// Unknown symbol type
    Unknown,
}

impl SymbolKind {
    /// Convert a u8 value to SymbolKind
    ///
    /// This is used for deserializing symbol kind from compact storage formats.
    pub fn from_u8(value: u8) -> Self {
        match value {
            0 => SymbolKind::Function,
            1 => SymbolKind::Method,
            2 => SymbolKind::Class,
            3 => SymbolKind::Interface,
            4 => SymbolKind::Enum,
            5 => SymbolKind::Module,
            6 => SymbolKind::Union,
            7 => SymbolKind::Namespace,
            8 => SymbolKind::TypeAlias,
            _ => SymbolKind::Unknown,
        }
    }

    /// Convert a string representation to SymbolKind
    ///
    /// Accepts both the enum variant names (e.g., "Function", "Method")
    /// and normalized keys (e.g., "fn", "method").
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "Function" => Some(SymbolKind::Function),
            "Method" => Some(SymbolKind::Method),
            "Class" => Some(SymbolKind::Class),
            "Interface" => Some(SymbolKind::Interface),
            "Enum" => Some(SymbolKind::Enum),
            "Module" => Some(SymbolKind::Module),
            "Union" => Some(SymbolKind::Union),
            "Namespace" => Some(SymbolKind::Namespace),
            "TypeAlias" => Some(SymbolKind::TypeAlias),
            "Unknown" => Some(SymbolKind::Unknown),
            "fn" => Some(SymbolKind::Function),
            "method" => Some(SymbolKind::Method),
            "struct" => Some(SymbolKind::Class),
            "trait" => Some(SymbolKind::Interface),
            "enum" => Some(SymbolKind::Enum),
            "mod" => Some(SymbolKind::Module),
            "union" => Some(SymbolKind::Union),
            "namespace" => Some(SymbolKind::Namespace),
            "type_alias" => Some(SymbolKind::TypeAlias),
            "unknown" => Some(SymbolKind::Unknown),
            _ => None,
        }
    }

    /// Return the normalized string key for this symbol kind (used for CLI/JSON)
    pub fn normalized_key(&self) -> &'static str {
        match self {
            SymbolKind::Function => "fn",
            SymbolKind::Method => "method",
            SymbolKind::Class => "struct",
            SymbolKind::Interface => "trait",
            SymbolKind::Enum => "enum",
            SymbolKind::Module => "mod",
            SymbolKind::Union => "union",
            SymbolKind::Namespace => "namespace",
            SymbolKind::TypeAlias => "type_alias",
            SymbolKind::Unknown => "unknown",
        }
    }
}

/// Separator character for FQN construction per language
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopeSeparator {
    /// Rust, C, C++ use :: separator
    DoubleColon,
    /// Python, Java, JavaScript, TypeScript use . separator
    Dot,
}

impl ScopeSeparator {
    pub fn as_str(&self) -> &'static str {
        match self {
            ScopeSeparator::DoubleColon => "::",
            ScopeSeparator::Dot => ".",
        }
    }
}

/// Stack for tracking scope nesting during tree-sitter traversal
///
/// Maintains a hierarchy of scope names (modules, types, namespaces)
/// to build fully-qualified names for symbols.
///
/// # Example
/// ```rust
/// use magellan::ingest::{ScopeSeparator, ScopeStack};
///
/// let mut stack = ScopeStack::new(ScopeSeparator::DoubleColon);
/// stack.push("my_crate");
/// stack.push("my_module");
/// assert_eq!(stack.current_fqn(), "my_crate::my_module");
/// stack.push("MyStruct");
/// assert_eq!(stack.current_fqn(), "my_crate::my_module::MyStruct");
/// ```
#[derive(Debug, Clone)]
pub struct ScopeStack {
    /// Scope components in order (e.g., ["my_crate", "my_module", "MyStruct"])
    scopes: Vec<String>,
    /// Separator for this language
    separator: ScopeSeparator,
}

impl ScopeStack {
    /// Create a new empty scope stack
    pub fn new(separator: ScopeSeparator) -> Self {
        Self {
            scopes: Vec::new(),
            separator,
        }
    }

    /// Push a new scope level onto the stack
    pub fn push(&mut self, scope: impl Into<String>) {
        self.scopes.push(scope.into());
    }

    /// Pop the most recent scope level from the stack
    pub fn pop(&mut self) -> Option<String> {
        self.scopes.pop()
    }

    /// Get the current fully-qualified name
    pub fn current_fqn(&self) -> String {
        if self.scopes.is_empty() {
            String::new()
        } else {
            self.scopes.join(self.separator.as_str())
        }
    }

    /// Get FQN for a symbol within the current scope
    pub fn fqn_for_symbol(&self, symbol_name: &str) -> String {
        let current = self.current_fqn();
        if current.is_empty() {
            symbol_name.to_string()
        } else if symbol_name.is_empty() {
            current
        } else {
            format!("{}{}{}", current, self.separator.as_str(), symbol_name)
        }
    }

    /// Get the depth of the scope stack
    pub fn depth(&self) -> usize {
        self.scopes.len()
    }

    /// Check if stack is empty
    pub fn is_empty(&self) -> bool {
        self.scopes.is_empty()
    }

    /// Get the separator for this stack
    pub fn separator(&self) -> ScopeSeparator {
        self.separator
    }

    /// Get read-only access to the scope components
    pub fn scopes(&self) -> &[String] {
        &self.scopes
    }
}

/// A fact about a symbol extracted from source code
///
/// Pure data structure. No behavior. No semantic analysis.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SymbolFact {
    /// File containing this symbol
    pub file_path: PathBuf,
    /// Kind of symbol
    pub kind: SymbolKind,
    /// Canonical kind string (fn/struct/enum/...) derived during ingest
    pub kind_normalized: String,
    /// Symbol name (if any - some symbols like impl blocks may not have names)
    pub name: Option<String>,
    /// Fully-qualified name for stable symbol_id generation
    #[serde(default)]
    pub fqn: Option<String>,
    /// Canonical fully-qualified name with file path for unambiguous identity
    #[serde(default)]
    pub canonical_fqn: Option<String>,
    /// Display fully-qualified name for human-readable output
    #[serde(default)]
    pub display_fqn: Option<String>,
    /// Byte offset where symbol starts in file
    pub byte_start: usize,
    /// Byte offset where symbol ends in file
    pub byte_end: usize,
    /// Line where symbol starts (1-indexed)
    pub start_line: usize,
    /// Column where symbol starts (0-indexed, bytes)
    pub start_col: usize,
    /// Line where symbol ends (1-indexed)
    pub end_line: usize,
    /// Column where symbol ends (0-indexed, bytes)
    pub end_col: usize,
}
