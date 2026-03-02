//! LSIF import functionality
//!
//! Imports LSIF data from external packages for cross-repository symbol resolution.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::fs::File;
use std::io::{BufRead, BufReader};
use serde_json;

use super::schema::{Vertex, PackageData};

/// Import LSIF data from a file
///
/// # Arguments
/// * `lsif_path` - Path to LSIF JSONL file
///
/// # Returns
/// Imported package information
pub fn import_lsif(lsif_path: &Path) -> Result<ImportedPackage> {
    let file = File::open(lsif_path)
        .with_context(|| format!("Failed to open LSIF file: {:?}", lsif_path))?;
    
    let reader = BufReader::new(file);
    let mut package_info: Option<PackageData> = None;
    let mut symbol_count = 0usize;
    let mut document_count = 0usize;

    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }

        // Try to parse as vertex first
        if let Ok(vertex) = serde_json::from_str::<Vertex>(&line) {
            match vertex {
                Vertex::Package { data, .. } => {
                    package_info = Some(data);
                }
                Vertex::Document { .. } => {
                    document_count += 1;
                }
                Vertex::Symbol { .. } => {
                    symbol_count += 1;
                }
                _ => {}
            }
        }
    }

    let package = package_info.ok_or_else(|| {
        anyhow::anyhow!("No package information found in LSIF file")
    })?;

    Ok(ImportedPackage {
        package,
        symbol_count,
        document_count,
        source_path: lsif_path.to_path_buf(),
    })
}

/// Information about an imported package
#[derive(Debug, Clone)]
pub struct ImportedPackage {
    /// Package metadata
    pub package: PackageData,
    /// Number of symbols in the package
    pub symbol_count: usize,
    /// Number of documents in the package
    pub document_count: usize,
    /// Source LSIF file path
    pub source_path: PathBuf,
}

/// Import multiple LSIF files from a directory
///
/// # Arguments
/// * `lsif_dir` - Directory containing .lsif files
///
/// # Returns
/// List of imported packages
pub fn import_lsif_directory(lsif_dir: &Path) -> Result<Vec<ImportedPackage>> {
    let mut packages = Vec::new();

    if !lsif_dir.exists() {
        return Ok(packages);
    }

    for entry in std::fs::read_dir(lsif_dir)? {
        let entry = entry?;
        let path = entry.path();
        
        if path.extension().and_then(|e| e.to_str()) == Some("lsif") {
            match import_lsif(&path) {
                Ok(pkg) => packages.push(pkg),
                Err(e) => eprintln!("Warning: Failed to import {:?}: {}", path, e),
            }
        }
    }

    Ok(packages)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use crate::lsif::export::export_lsif;
    use crate::graph::CodeGraph;

    #[test]
    fn test_import_lsif_basic() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let lsif_path = temp_dir.path().join("test.lsif");

        // Create and export a test graph
        let mut graph = CodeGraph::open(&db_path).unwrap();
        let test_file = temp_dir.path().join("test.rs");
        std::fs::write(&test_file, "fn main() {}").unwrap();
        let _ = graph.scan_directory(temp_dir.path(), None);
        
        let _ = export_lsif(&mut graph, &lsif_path, "test-crate", "0.1.0");

        // Import the LSIF file
        let result = import_lsif(&lsif_path);
        assert!(result.is_ok());

        let pkg = result.unwrap();
        assert_eq!(pkg.package.name, "test-crate");
        assert_eq!(pkg.package.version, "0.1.0");
        assert!(pkg.symbol_count >= 1);
    }

    #[test]
    fn test_import_lsif_directory() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let lsif_path = temp_dir.path().join("test.lsif");

        // Create and export a test graph
        let mut graph = CodeGraph::open(&db_path).unwrap();
        let test_file = temp_dir.path().join("test.rs");
        std::fs::write(&test_file, "fn main() {}").unwrap();
        let _ = graph.scan_directory(temp_dir.path(), None);
        let _ = export_lsif(&mut graph, &lsif_path, "test-crate", "0.1.0");

        // Import from directory
        let result = import_lsif_directory(temp_dir.path());
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 1);
    }
}
