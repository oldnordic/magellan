//! Language detection from file extensions.
//!
//! Table-driven language detection. No heuristics, no guessing.
//! Unknown extensions return None, never infer from content.

use std::path::Path;

/// Programming languages supported by Magellan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    /// Rust (.rs)
    Rust,
    /// Python (.py)
    Python,
    /// C (.c, .h)
    C,
    /// C++ (.cpp, .hpp, .cc, .cxx)
    Cpp,
    /// Java (.java)
    Java,
    /// JavaScript (.js, .mjs, .cjs)
    JavaScript,
    /// TypeScript (.ts, .tsx)
    TypeScript,
}

impl Language {
    /// Convert language to string identifier.
    pub fn as_str(&self) -> &'static str {
        match self {
            Language::Rust => "rust",
            Language::Python => "python",
            Language::C => "c",
            Language::Cpp => "cpp",
            Language::Java => "java",
            Language::JavaScript => "javascript",
            Language::TypeScript => "typescript",
        }
    }
}

/// Detect programming language from file path.
///
/// Uses table-driven extension mapping. Returns None for unknown extensions.
/// Never guesses or infers from file content.
///
/// # Examples
///
/// ```
/// # use magellan::ingest::detect::{detect_language, Language};
/// # use std::path::Path;
/// assert_eq!(detect_language(Path::new("main.rs")), Some(Language::Rust));
/// assert_eq!(detect_language(Path::new("script.py")), Some(Language::Python));
/// assert_eq!(detect_language(Path::new("file.txt")), None);
/// ```
pub fn detect_language(path: &Path) -> Option<Language> {
    // Get the file extension
    let extension = path.extension()?.to_str()?;

    // Table-driven mapping (case-sensitive)
    let language = match extension {
        // Rust
        "rs" => Language::Rust,

        // Python
        "py" => Language::Python,

        // C
        "c" | "h" => Language::C,

        // C++
        "cpp" | "hpp" | "cc" | "cxx" => Language::Cpp,

        // Java
        "java" => Language::Java,

        // JavaScript
        "js" | "mjs" | "cjs" => Language::JavaScript,

        // TypeScript
        "ts" | "tsx" => Language::TypeScript,

        // Unknown extension
        _ => return None,
    };

    Some(language)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_rust() {
        assert_eq!(detect_language(Path::new("main.rs")), Some(Language::Rust));
        assert_eq!(detect_language(Path::new("lib.rs")), Some(Language::Rust));
    }

    #[test]
    fn test_detect_python() {
        assert_eq!(
            detect_language(Path::new("script.py")),
            Some(Language::Python)
        );
    }

    #[test]
    fn test_detect_c() {
        assert_eq!(detect_language(Path::new("main.c")), Some(Language::C));
        assert_eq!(detect_language(Path::new("header.h")), Some(Language::C));
    }

    #[test]
    fn test_detect_cpp() {
        assert_eq!(detect_language(Path::new("main.cpp")), Some(Language::Cpp));
        assert_eq!(
            detect_language(Path::new("header.hpp")),
            Some(Language::Cpp)
        );
        assert_eq!(detect_language(Path::new("main.cc")), Some(Language::Cpp));
        assert_eq!(detect_language(Path::new("main.cxx")), Some(Language::Cpp));
    }

    #[test]
    fn test_detect_java() {
        assert_eq!(
            detect_language(Path::new("Main.java")),
            Some(Language::Java)
        );
    }

    #[test]
    fn test_detect_javascript() {
        assert_eq!(
            detect_language(Path::new("script.js")),
            Some(Language::JavaScript)
        );
        assert_eq!(
            detect_language(Path::new("module.mjs")),
            Some(Language::JavaScript)
        );
        assert_eq!(
            detect_language(Path::new("module.cjs")),
            Some(Language::JavaScript)
        );
    }

    #[test]
    fn test_detect_typescript() {
        assert_eq!(
            detect_language(Path::new("component.ts")),
            Some(Language::TypeScript)
        );
        assert_eq!(
            detect_language(Path::new("component.tsx")),
            Some(Language::TypeScript)
        );
    }

    #[test]
    fn test_unknown_extension_returns_none() {
        assert_eq!(detect_language(Path::new("file.unknown")), None);
        assert_eq!(detect_language(Path::new("file.txt")), None);
        assert_eq!(detect_language(Path::new("file.md")), None);
    }

    #[test]
    fn test_no_extension_returns_none() {
        assert_eq!(detect_language(Path::new("Makefile")), None);
        assert_eq!(detect_language(Path::new("Dockerfile")), None);
    }

    #[test]
    fn test_empty_path_returns_none() {
        assert_eq!(detect_language(Path::new("")), None);
    }

    #[test]
    fn test_dotfile_returns_none() {
        assert_eq!(detect_language(Path::new(".gitignore")), None);
    }

    #[test]
    fn test_case_sensitive() {
        // Extensions are case-sensitive on Unix
        assert_eq!(detect_language(Path::new("file.RS")), None);
        assert_eq!(detect_language(Path::new("file.PY")), None);
    }

    #[test]
    fn test_path_with_directory() {
        assert_eq!(
            detect_language(Path::new("src/module/main.rs")),
            Some(Language::Rust)
        );
    }

    #[test]
    fn test_absolute_path() {
        assert_eq!(
            detect_language(Path::new("/usr/local/bin/script.py")),
            Some(Language::Python)
        );
    }

    #[test]
    fn test_language_as_str() {
        assert_eq!(Language::Rust.as_str(), "rust");
        assert_eq!(Language::Python.as_str(), "python");
        assert_eq!(Language::C.as_str(), "c");
        assert_eq!(Language::Cpp.as_str(), "cpp");
        assert_eq!(Language::Java.as_str(), "java");
        assert_eq!(Language::JavaScript.as_str(), "javascript");
        assert_eq!(Language::TypeScript.as_str(), "typescript");
    }
}
