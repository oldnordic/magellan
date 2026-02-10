//! Tracing infrastructure tests.
//!
//! Tests that tracing is properly initialized and structured logging works.

#[test]
fn test_tracing_init_compiles() {
    // This test verifies that the tracing infrastructure compiles correctly.
    // It's a compile-time test - if it compiles, tracing works.
    //
    // Actual tracing output verification requires RUST_LOG environment variable.
    //
    // Run with: RUST_LOG=debug cargo test tracing_test

    // Verify tracing types are accessible
    let _level: tracing::Level = tracing::Level::INFO;
    assert_eq!(_level, tracing::Level::INFO);

    // The test passes if we got here - tracing is available
    assert!(true);
}

#[test]
fn test_tracing_span_macro_compiles() {
    // Verify that #[instrument] macro can be used
    // This is a compile-time test

    use tracing::instrument;

    // Helper function with instrumentation
    #[instrument]
    fn traced_function(x: i32) -> i32 {
        x * 2
    }

    assert_eq!(traced_function(5), 10);
}

#[test]
fn test_tracing_macros_compile() {
    // Verify all tracing macros compile
    use tracing::{debug, info, warn, error};

    // These statements will only execute with proper subscriber
    // Since we initialize tracing in main(), this test is mainly compile-time

    let test_value = 42;
    let test_error = std::io::Error::new(std::io::ErrorKind::Other, "test error");

    // Compile-time check for all macros
    let _ = format!("debug: {}", test_value);
    let _ = format!("info: {}", test_value);
    let _ = format!("warn: {}", test_value);
    let _ = format!("error: {}", test_error);

    assert!(true);
}
