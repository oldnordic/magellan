//! External tools CFG extraction
//!
//! This module provides cross-platform CFG extraction for C/C++ and Java
//! by invoking external tools (clang, javac) and parsing their output files.
//!
//! ## Architecture
//!
//! - **tool_detector**: Cross-platform clang/javac detection
//! - **tool_invoker**: Safe process invocation with timeouts
//! - **c_cpp**: C/C++ CFG extraction via clang → LLVM IR
//! - **java**: Java CFG extraction via javac → .class bytecode
//!
//! ## Platform Support
//!
//! - **Linux**: Searches /usr/bin, /usr/local/bin, /opt/llvm/bin
//! - **Windows**: Searches Program Files, registry, common JDK paths
//! - **macOS**: Searches /opt/homebrew, /usr/local
//!
//! ## Graceful Degradation
//!
//! If external tools are not found, CFG extraction is skipped and a warning
//! is logged. Indexing continues without failing.

pub mod tool_detector;
pub mod tool_invoker;

pub mod c_cpp;
pub mod compile_commands;
pub mod java;

pub use tool_detector::{
    get_clang_install_instructions, get_javac_install_instructions, ToolDetectionError,
};
