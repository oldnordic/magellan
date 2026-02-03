//! Java bytecode-based CFG extraction using ASM library
//!
//! This module provides CFG extraction from compiled Java .class files using
//! the ASM library (org.ow2.asm). Bytecode-based CFG is more precise than
//! AST-based CFG for Java because it includes:
//!
//! - Compiler-generated control flow
//! - Exception handler edges (try/catch/finally)
//! - Synthetic bridge methods
//! - Lambda body desugaring
//!
//! ## Feature Flag
//!
//! This module is only compiled when the "bytecode-cfg" feature is enabled.
//!
//! To enable:
//! ```bash
//! cargo build --release --features bytecode-cfg
//! ```
//!
//! ## Limitations
//!
//! - Requires javac compilation first (source -> .class files)
//! - Only works for Java (not other JVM languages without adaptation)
//! - Optional enhancement (AST CFG from Phase 42 works for Java too)
//!
//! ## References
//!
//! - ASM library: https://asm.ow2.io/
//! - ASM Analysis (CFG): https://asm.ow2.io/asm70-guide.pdf
//! - Phase 44 plans: .planning/phases/44-bytecode-cfg-java/

#[cfg(feature = "bytecode-cfg")]
use anyhow::Result;

/// Bytecode-based CFG extractor for Java
///
/// Uses ASM's Analyzer class to construct control flow graphs from bytecode.
#[cfg(feature = "bytecode-cfg")]
pub struct JavaBytecodeCfgExtractor {
    // Placeholder - implementation in 44-02
}

#[cfg(feature = "bytecode-cfg")]
impl JavaBytecodeCfgExtractor {
    /// Create a new bytecode CFG extractor
    pub fn new() -> Result<Self> {
        Ok(Self {})
    }

    /// Extract CFG blocks from a Java .class file
    ///
    /// # Arguments
    /// * `class_path` - Path to the .class file
    /// * `method_name` - Method to analyze
    ///
    /// # Returns
    /// Vector of CfgBlock representing the method's control flow
    pub fn extract_cfg_from_class(
        &self,
        _class_path: &std::path::Path,
        _method_name: &str,
    ) -> Result<Vec<crate::graph::schema::CfgBlock>> {
        // TODO: Implement using ASM's Analyzer class
        // See 44-02-PLAN.md for implementation details
        Ok(Vec::new())
    }
}

// Stub implementation when feature is not enabled
#[cfg(not(feature = "bytecode-cfg"))]
pub struct JavaBytecodeCfgExtractor;

#[cfg(not(feature = "bytecode-cfg"))]
impl JavaBytecodeCfgExtractor {
    pub fn new() -> Result<Self> {
        Ok(Self {})
    }

    pub fn extract_cfg_from_class(
        &self,
        _class_path: &std::path::Path,
        _method_name: &str,
    ) -> Result<Vec<crate::graph::schema::CfgBlock>> {
        Ok(Vec::new())
    }
}
