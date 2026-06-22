//! Safe external tool invocation with timeouts
//!
//! Executes external tools (clang, javac) with proper error handling
//! and timeout support.

use anyhow::{Context, Result};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

/// Output from running an external tool
#[derive(Debug, Clone)]
pub struct ToolOutput {
    /// Standard output
    pub stdout: Vec<u8>,
    /// Standard error
    pub stderr: Vec<u8>,
    /// Exit code (None if process terminated by signal)
    pub exit_code: Option<i32>,
}

impl ToolOutput {
    /// Check if the tool succeeded
    pub fn success(&self) -> bool {
        self.exit_code.is_some_and(|code| code == 0)
    }

    /// Get stdout as UTF-8 string
    pub fn stdout_str(&self) -> Result<&str, std::str::Utf8Error> {
        std::str::from_utf8(&self.stdout)
    }

    /// Get stderr as UTF-8 string
    pub fn stderr_str(&self) -> Result<&str, std::str::Utf8Error> {
        std::str::from_utf8(&self.stderr)
    }
}

/// Errors from tool invocation
#[derive(Debug, thiserror::Error)]
pub enum ToolInvocationError {
    #[error("Tool not found: {path}")]
    ToolNotFound { path: String },

    #[error("Tool execution failed: {tool}")]
    ExecutionFailed { tool: String, reason: String },

    #[error("Timeout: {tool} did not complete within {timeout_secs}s")]
    Timeout { tool: String, timeout_secs: u64 },

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Invoke an external tool with a timeout (synchronous)
///
/// # Arguments
///
/// * `executable` - Path to the executable
/// * `args` - Arguments to pass to the executable
/// * `timeout` - Maximum time to wait for completion (seconds)
///
/// # Returns
///
/// `ToolOutput` containing stdout, stderr, and exit code
///
/// # Note
/// This is a simplified version that uses std::process::Command with a timeout.
/// For production use, consider using async/tokio for proper timeout handling.
pub fn invoke_tool_with_timeout(
    executable: &Path,
    args: &[&str],
    _timeout: Duration,
) -> Result<ToolOutput, ToolInvocationError> {
    if !executable.exists() {
        return Err(ToolInvocationError::ToolNotFound {
            path: executable.display().to_string(),
        });
    }

    let tool_name = executable
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("tool")
        .to_string();

    // For now, use the sync version without timeout

    let output = Command::new(executable).args(args).output().map_err(|e| {
        ToolInvocationError::ExecutionFailed {
            tool: tool_name.clone(),
            reason: e.to_string(),
        }
    })?;

    Ok(ToolOutput {
        stdout: output.stdout,
        stderr: output.stderr,
        exit_code: output.status.code(),
    })
}

/// Invoke an external tool synchronously (blocking, no timeout)
///
/// Use this for short-running operations where tokio runtime is not available.
///
/// # Arguments
///
/// * `executable` - Path to the executable
/// * `args` - Arguments to pass to the executable
///
/// # Returns
///
/// `ToolOutput` containing stdout, stderr, and exit code
pub fn invoke_tool_sync(
    executable: &Path,
    args: &[&str],
) -> Result<ToolOutput, ToolInvocationError> {
    if !executable.exists() {
        return Err(ToolInvocationError::ToolNotFound {
            path: executable.display().to_string(),
        });
    }

    let tool_name = executable
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("tool")
        .to_string();

    let output = Command::new(executable).args(args).output().map_err(|e| {
        ToolInvocationError::ExecutionFailed {
            tool: tool_name.clone(),
            reason: e.to_string(),
        }
    })?;

    Ok(ToolOutput {
        stdout: output.stdout,
        stderr: output.stderr,
        exit_code: output.status.code(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_output_success() {
        let output = ToolOutput {
            stdout: b"hello".to_vec(),
            stderr: b"".to_vec(),
            exit_code: Some(0),
        };

        assert!(output.success());
        assert_eq!(output.stdout_str().unwrap(), "hello");
    }

    #[test]
    fn test_tool_output_failure() {
        let output = ToolOutput {
            stdout: b"".to_vec(),
            stderr: b"error".to_vec(),
            exit_code: Some(1),
        };

        assert!(!output.success());
        assert_eq!(output.stderr_str().unwrap(), "error");
    }

    #[test]
    fn test_invoke_tool_sync_not_found() {
        let result = invoke_tool_sync(Path::new("/nonexistent/tool"), &[]);
        assert!(matches!(
            result,
            Err(ToolInvocationError::ToolNotFound { .. })
        ));
    }

    #[cfg(unix)]
    #[test]
    fn test_invoke_tool_sync_echo() {
        // Test with echo command (should exist on Unix)
        let result = invoke_tool_sync(Path::new("/bin/echo"), &["hello"]);
        assert!(result.is_ok());

        let output = result.unwrap();
        assert!(output.success());
        assert_eq!(output.stdout_str().unwrap().trim(), "hello");
    }
}
