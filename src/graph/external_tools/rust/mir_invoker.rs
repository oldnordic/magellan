//! Rust MIR dumping via nightly rustc
//!
//! Dumps human-readable MIR (Mid-level IR) for a Rust source file using
//! `rustup run nightly rustc -Zunpretty=mir`. The MIR text is then parsed
//! by `mir_parser` to extract control-flow graphs.
//!
//! This is a runtime-only dependency on nightly — magellan itself compiles
//! on stable. Nightly is detected via `tool_detector::find_rustc_nightly()`.

use anyhow::Result;
use std::path::Path;

use crate::graph::external_tools::tool_detector;

/// Errors from MIR dumping
#[derive(Debug, thiserror::Error)]
pub enum MirDumpError {
    #[error("Nightly rustc not found: {0}")]
    NightlyNotFound(String),

    #[error("MIR dump failed (compilation error): {0}")]
    CompilationFailed(String),

    #[error("MIR dump produced no output")]
    EmptyOutput,

    #[error("MIR dump timed out after {0}s")]
    Timeout(u64),
}

/// Dump MIR text for a single Rust source file.
///
/// Runs `rustup run nightly rustc -Zunpretty=mir <source_path>` and captures
/// stdout (the MIR text). The file must be self-contained (no external crate
/// dependencies) — for crate-level MIR, use `cargo +nightly rustc` instead.
///
/// Returns the raw MIR text content.
pub fn dump_mir(source_path: &Path) -> Result<String, MirDumpError> {
    // Step 1: Verify nightly is available
    let nightly_rustc = tool_detector::find_rustc_nightly().map_err(|_e| {
        MirDumpError::NightlyNotFound(format!(
            "Nightly toolchain not found. {}",
            tool_detector::get_rustc_nightly_install_instructions()
        ))
    })?;

    // Step 2: Run rustc with -Zunpretty=mir
    // We use the nightly rustc binary directly (found by find_rustc_nightly)
    // rather than `rustup run nightly` to avoid the extra indirection.
    let output = std::process::Command::new(&nightly_rustc)
        .arg("-Zunpretty=mir")
        .arg(source_path)
        .output()
        .map_err(|e| MirDumpError::CompilationFailed(format!("Failed to execute rustc: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(MirDumpError::CompilationFailed(stderr.to_string()));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    if stdout.trim().is_empty() {
        return Err(MirDumpError::EmptyOutput);
    }

    Ok(stdout.to_string())
}

/// Dump MIR text for a single Rust source file with a timeout.
///
/// Same as `dump_mir` but uses `invoke_tool_with_timeout` to prevent
/// hangs on files that trigger compiler bugs or infinite loops.
pub fn dump_mir_with_timeout(
    source_path: &Path,
    timeout_secs: u64,
) -> Result<String, MirDumpError> {
    use std::time::Duration;

    use crate::graph::external_tools::tool_invoker::invoke_tool_with_timeout;

    let nightly_rustc = tool_detector::find_rustc_nightly().map_err(|_e| {
        MirDumpError::NightlyNotFound(format!(
            "Nightly toolchain not found. {}",
            tool_detector::get_rustc_nightly_install_instructions()
        ))
    })?;

    let result = invoke_tool_with_timeout(
        &nightly_rustc,
        &["-Zunpretty=mir", source_path.to_str().unwrap_or("")],
        Duration::from_secs(timeout_secs),
    )
    .map_err(|e| MirDumpError::CompilationFailed(format!("Tool invocation failed: {}", e)))?;

    if !result.success() {
        let stderr = result.stderr_str().unwrap_or("unknown error");
        return Err(MirDumpError::CompilationFailed(stderr.to_string()));
    }

    let stdout = result.stdout_str().map_err(|_| {
        MirDumpError::CompilationFailed("MIR stdout was not valid UTF-8".to_string())
    })?;
    if stdout.trim().is_empty() {
        return Err(MirDumpError::EmptyOutput);
    }

    Ok(stdout.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dump_mir_on_simple_file() {
        // Create a temp file with a simple function
        let temp_dir = std::env::temp_dir();
        let test_file = temp_dir.join("magellan_mir_test_simple.rs");

        std::fs::write(
            &test_file,
            "fn main() {}\nfn demo(x: i32) -> i32 { if x > 0 { x + 1 } else { x - 1 } }\n",
        )
        .expect("Failed to write test file");

        // This test requires nightly to be installed. If it's not, skip.
        let nightly_available = tool_detector::find_rustc_nightly().is_ok();
        if !nightly_available {
            let _ = std::fs::remove_file(&test_file);
            eprintln!("Skipping test_dump_mir_on_simple_file — nightly not installed");
            return;
        }

        let result = dump_mir_with_timeout(&test_file, 15);

        // Clean up
        let _ = std::fs::remove_file(&test_file);

        let mir = result.expect("MIR dump should succeed on a valid file");

        // Verify the output looks like MIR
        assert!(
            mir.contains("fn "),
            "MIR should contain function definitions"
        );
        assert!(
            mir.contains("bb0:"),
            "MIR should contain basic blocks. Got:\n{}",
            &mir[..mir.len().min(500)]
        );
    }
}
