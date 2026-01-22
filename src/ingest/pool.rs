//! Thread-local parser pool for reusing tree-sitter Parser instances.
//!
//! Each file indexing operation currently creates a fresh Parser::new() instance.
//! Parser pooling reuses parser instances across files, significantly reducing
//! allocation overhead during indexing.
//!
//! # Design
//!
//! - Thread-local storage: Each thread has its own parser instances
//! - Lazy initialization: Parsers created on first use per thread
//! - No locks: RefCell provides single-threaded mutable access
//! - Language-specific: One parser per supported language
//!
//! # Usage
//!
//! ```rust
//! use crate::ingest::pool::with_parser;
//! use crate::ingest::detect::Language;
//!
//! let facts = with_parser(Language::Rust, |parser| {
//!     let tree = parser.parse(source, None)?;
//!     // ... extract symbols
//! })?;
//! ```

use crate::ingest::detect::Language;
use anyhow::Result;
use std::cell::RefCell;

// Thread-local parser storage for each supported language.
// Each thread gets its own parser instance, avoiding lock contention.
thread_local! {
    static RUST_PARSER: RefCell<Option<tree_sitter::Parser>> = RefCell::new(None);
    static PYTHON_PARSER: RefCell<Option<tree_sitter::Parser>> = RefCell::new(None);
    static C_PARSER: RefCell<Option<tree_sitter::Parser>> = RefCell::new(None);
    static CPP_PARSER: RefCell<Option<tree_sitter::Parser>> = RefCell::new(None);
    static JAVA_PARSER: RefCell<Option<tree_sitter::Parser>> = RefCell::new(None);
    static JAVASCRIPT_PARSER: RefCell<Option<tree_sitter::Parser>> = RefCell::new(None);
    static TYPESCRIPT_PARSER: RefCell<Option<tree_sitter::Parser>> = RefCell::new(None);
}

/// Initialize or get the thread-local Rust parser
fn with_rust_parser<F, R>(f: F) -> Result<R>
where
    F: FnOnce(&mut tree_sitter::Parser) -> R,
{
    RUST_PARSER.with(|parser_cell| {
        let mut parser_ref = parser_cell.borrow_mut();
        if parser_ref.is_none() {
            let mut parser = tree_sitter::Parser::new();
            parser.set_language(&tree_sitter_rust::language())?;
            *parser_ref = Some(parser);
        }
        Ok(f(parser_ref.as_mut().expect(
            "Parser invariant violated: Option must be Some() after initialization (lines 49-52)"
        )))
    })
}

/// Initialize or get the thread-local Python parser
fn with_python_parser<F, R>(f: F) -> Result<R>
where
    F: FnOnce(&mut tree_sitter::Parser) -> R,
{
    PYTHON_PARSER.with(|parser_cell| {
        let mut parser_ref = parser_cell.borrow_mut();
        if parser_ref.is_none() {
            let mut parser = tree_sitter::Parser::new();
            parser.set_language(&tree_sitter_python::language())?;
            *parser_ref = Some(parser);
        }
        Ok(f(parser_ref.as_mut().expect(
            "Python parser invariant violated: Option must be Some() after initialization"
        )))
    })
}

/// Initialize or get the thread-local C parser
fn with_c_parser<F, R>(f: F) -> Result<R>
where
    F: FnOnce(&mut tree_sitter::Parser) -> R,
{
    C_PARSER.with(|parser_cell| {
        let mut parser_ref = parser_cell.borrow_mut();
        if parser_ref.is_none() {
            let mut parser = tree_sitter::Parser::new();
            parser.set_language(&tree_sitter_c::language())?;
            *parser_ref = Some(parser);
        }
        Ok(f(parser_ref.as_mut().expect(
            "C parser invariant violated: Option must be Some() after initialization"
        )))
    })
}

/// Initialize or get the thread-local C++ parser
fn with_cpp_parser<F, R>(f: F) -> Result<R>
where
    F: FnOnce(&mut tree_sitter::Parser) -> R,
{
    CPP_PARSER.with(|parser_cell| {
        let mut parser_ref = parser_cell.borrow_mut();
        if parser_ref.is_none() {
            let mut parser = tree_sitter::Parser::new();
            parser.set_language(&tree_sitter_cpp::language())?;
            *parser_ref = Some(parser);
        }
        Ok(f(parser_ref.as_mut().expect(
            "C++ parser invariant violated: Option must be Some() after initialization"
        )))
    })
}

/// Initialize or get the thread-local Java parser
fn with_java_parser<F, R>(f: F) -> Result<R>
where
    F: FnOnce(&mut tree_sitter::Parser) -> R,
{
    JAVA_PARSER.with(|parser_cell| {
        let mut parser_ref = parser_cell.borrow_mut();
        if parser_ref.is_none() {
            let mut parser = tree_sitter::Parser::new();
            parser.set_language(&tree_sitter_java::language())?;
            *parser_ref = Some(parser);
        }
        Ok(f(parser_ref.as_mut().expect(
            "Java parser invariant violated: Option must be Some() after initialization"
        )))
    })
}

/// Initialize or get the thread-local JavaScript parser
fn with_javascript_parser<F, R>(f: F) -> Result<R>
where
    F: FnOnce(&mut tree_sitter::Parser) -> R,
{
    JAVASCRIPT_PARSER.with(|parser_cell| {
        let mut parser_ref = parser_cell.borrow_mut();
        if parser_ref.is_none() {
            let mut parser = tree_sitter::Parser::new();
            parser.set_language(&tree_sitter_javascript::language())?;
            *parser_ref = Some(parser);
        }
        Ok(f(parser_ref.as_mut().expect(
            "JavaScript parser invariant violated: Option must be Some() after initialization"
        )))
    })
}

/// Initialize or get the thread-local TypeScript parser
fn with_typescript_parser<F, R>(f: F) -> Result<R>
where
    F: FnOnce(&mut tree_sitter::Parser) -> R,
{
    TYPESCRIPT_PARSER.with(|parser_cell| {
        let mut parser_ref = parser_cell.borrow_mut();
        if parser_ref.is_none() {
            let mut parser = tree_sitter::Parser::new();
            parser.set_language(&tree_sitter_typescript::language_typescript())?;
            *parser_ref = Some(parser);
        }
        Ok(f(parser_ref.as_mut().expect(
            "TypeScript parser invariant violated: Option must be Some() after initialization"
        )))
    })
}

/// Execute a function with a thread-local parser for the given language.
///
/// This function provides lazy-initialized, thread-local parser instances
/// for all supported languages. Each thread gets its own parser instances,
/// avoiding lock contention during parallel file indexing.
///
/// # Arguments
///
/// * `language` - The programming language to get a parser for
/// * `f` - A closure that takes `&mut tree_sitter::Parser` and returns a result
///
/// # Returns
///
/// The result of the closure, or an error if parser initialization fails.
///
/// # Example
///
/// ```rust
/// use crate::ingest::pool::with_parser;
/// use crate::ingest::detect::Language;
///
/// let symbols = with_parser(Language::Rust, |parser| {
///     let tree = parser.parse(source, None)?;
///     // Extract symbols from tree
///     Ok(vec![])
/// })?;
/// ```
pub fn with_parser<F, R>(language: Language, f: F) -> Result<R>
where
    F: FnOnce(&mut tree_sitter::Parser) -> R,
{
    match language {
        Language::Rust => with_rust_parser(f),
        Language::Python => with_python_parser(f),
        Language::C => with_c_parser(f),
        Language::Cpp => with_cpp_parser(f),
        Language::Java => with_java_parser(f),
        Language::JavaScript => with_javascript_parser(f),
        Language::TypeScript => with_typescript_parser(f),
    }
}

/// Warmup all parsers to avoid first-parse latency.
///
/// This function initializes all thread-local parsers by parsing minimal
/// source code for each supported language. Call this during application
/// startup to ensure the first real parse doesn't pay the initialization cost.
///
/// # Note
///
/// This function only warms up parsers for the calling thread. Thread-local
/// parsers are initialized per-thread, so each thread needs to call this
/// function (or rely on lazy initialization during first parse).
///
/// # Returns
///
/// Ok(()) if all parsers were successfully warmed up, or an error if any
/// parser initialization failed.
///
/// # Example
///
/// ```rust,no_run
/// use crate::ingest::pool::warmup_parsers;
///
/// // During application startup
/// warmup_parsers().expect("Failed to warmup parsers");
/// ```
pub fn warmup_parsers() -> Result<()> {
    // Minimal source code snippets for each language
    let test_cases: [(Language, &[u8]); 7] = [
        (Language::Rust, b"fn test() {}"),
        (Language::Python, b"def test(): pass"),
        (Language::C, b"int test() { return 0; }"),
        (Language::Cpp, b"void test() {}"),
        (Language::Java, b"class Test {}"),
        (Language::JavaScript, b"function test() {}"),
        (Language::TypeScript, b"function test(): void {}"),
    ];

    for (lang, source) in test_cases {
        let _ = with_parser(lang, |parser| {
            parser.parse(source, None);
            Ok::<(), anyhow::Error>(())
        })?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parser_reuse() {
        // Verify that the same thread gets the same parser instance
        let addr1 = with_rust_parser(|p| p as *const _ as usize).unwrap();
        let addr2 = with_rust_parser(|p| p as *const _ as usize).unwrap();
        assert_eq!(addr1, addr2, "Parser should be reused in same thread");
    }

    #[test]
    fn test_all_languages_have_parsers() {
        // Verify each language can initialize its parser
        let languages = [
            Language::Rust,
            Language::Python,
            Language::C,
            Language::Cpp,
            Language::Java,
            Language::JavaScript,
            Language::TypeScript,
        ];

        for lang in languages {
            let result = with_parser(lang, |parser| {
                // Try to parse an empty source
                let tree = parser.parse(b"", None);
                tree.is_some()
            });
            assert!(result.is_ok(), "Language {:?} should have a working parser", lang);
            assert!(result.unwrap(), "Language {:?} should parse successfully", lang);
        }
    }

    #[test]
    fn test_parser_initialization() {
        // First call should initialize, subsequent calls should reuse
        let source = b"fn test() {}";

        let result1 = with_parser(Language::Rust, |parser| {
            parser.parse(source, None).is_some()
        })
        .unwrap();
        assert!(result1, "First parse should succeed");

        let result2 = with_parser(Language::Rust, |parser| {
            parser.parse(source, None).is_some()
        })
        .unwrap();
        assert!(result2, "Second parse should succeed with reused parser");
    }

    #[test]
    fn test_concurrent_access() {
        use std::sync::{Arc, Barrier};
        use std::thread;

        let source = b"fn test() {}";
        let barrier = Arc::new(Barrier::new(2));
        let barrier_clone = barrier.clone();

        // Spawn a thread that also uses the parser pool
        let handle = thread::spawn(move || {
            barrier_clone.wait();
            with_parser(Language::Rust, |parser| parser.parse(source, None))
                .unwrap()
                .is_some()
        });

        barrier.wait();
        let main_result = with_parser(Language::Rust, |parser| {
            parser.parse(source, None)
        })
        .unwrap()
        .is_some();

        let thread_result = handle.join().unwrap();

        assert!(main_result, "Main thread parse should succeed");
        assert!(thread_result, "Spawned thread parse should succeed");
    }

    #[test]
    fn test_multiple_languages_same_thread() {
        // Verify we can use multiple language parsers in the same thread
        let test_cases: [(Language, &[u8]); 7] = [
            (Language::Rust, b"fn test() {}"),
            (Language::Python, b"def test(): pass"),
            (Language::C, b"int test() { return 0; }"),
            (Language::Cpp, b"void test() {}"),
            (Language::Java, b"class Test {}"),
            (Language::JavaScript, b"function test() {}"),
            (Language::TypeScript, b"function test(): void {}"),
        ];

        for (lang, source) in test_cases {
            let result = with_parser(lang, |parser| parser.parse(source, None).is_some());
            assert!(
                result.is_ok() && result.unwrap(),
                "Language {:?} should parse successfully",
                lang
            );
        }
    }

    #[test]
    fn test_parse_simple_rust() {
        let source = b"pub fn hello() -> String { \"world\".to_string() }";
        let tree = with_parser(Language::Rust, |parser| {
            parser.parse(source, None)
        }).unwrap();

        assert!(tree.is_some(), "Simple Rust function should parse successfully");
    }

    #[test]
    fn test_parse_simple_python() {
        let source = b"def hello():\n    return \"world\"";
        let tree = with_parser(Language::Python, |parser| {
            parser.parse(source, None)
        }).unwrap();

        assert!(tree.is_some(), "Simple Python function should parse successfully");
    }

    #[test]
    fn test_with_parser_unified_api() {
        // Test the unified with_parser API
        let tree = with_parser(Language::Rust, |parser| {
            parser.parse(b"struct Test;", None)
        }).unwrap();

        assert!(tree.is_some(), "Parser should successfully parse");
        assert_eq!(tree.unwrap().root_node().kind(), "source_file");
    }

    #[test]
    fn test_warmup_parsers() {
        // Warmup should succeed without errors
        warmup_parsers()
            .expect("Parser warmup should succeed");

        // After warmup, all parsers should be initialized
        let test_cases: [(Language, &[u8]); 7] = [
            (Language::Rust, b"fn test() {}"),
            (Language::Python, b"def test(): pass"),
            (Language::C, b"int test() { return 0; }"),
            (Language::Cpp, b"void test() {}"),
            (Language::Java, b"class Test {}"),
            (Language::JavaScript, b"function test() {}"),
            (Language::TypeScript, b"function test(): void {}"),
        ];

        for (lang, source) in test_cases {
            let result = with_parser(lang, |parser| parser.parse(source, None).is_some());
            assert!(
                result.is_ok() && result.unwrap(),
                "Language {:?} should parse successfully after warmup",
                lang
            );
        }
    }

    #[test]
    fn test_warmup_multiple_calls() {
        // Multiple warmup calls should be safe (parsers already initialized)
        warmup_parsers().expect("First warmup should succeed");
        warmup_parsers().expect("Second warmup should succeed");
        warmup_parsers().expect("Third warmup should succeed");
    }
}
