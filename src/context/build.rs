//! Context index building
//!
//! Builds a summary index for fast LLM context queries.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::fs::File;
use std::io::BufReader;

use crate::graph::CodeGraph;
use super::query::{ProjectSummary, get_project_summary};

/// Context index stored alongside the database
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextIndex {
    /// Database path this index was built from
    pub db_path: String,
    /// Project summary
    pub summary: ProjectSummary,
    /// Index version
    pub version: String,
    /// When the index was built (Unix timestamp)
    pub built_at: i64,
}

impl ContextIndex {
    /// Create a new context index
    pub fn new(db_path: &Path, summary: ProjectSummary) -> Self {
        Self {
            db_path: db_path.to_string_lossy().to_string(),
            summary,
            version: env!("CARGO_PKG_VERSION").to_string(),
            built_at: chrono::Utc::now().timestamp(),
        }
    }

    /// Get the index file path for a database
    pub fn index_path(db_path: &Path) -> PathBuf {
        let mut path = db_path.to_path_buf();
        path.set_extension("context.json");
        path
    }

    /// Load index from file
    pub fn load(db_path: &Path) -> Result<Option<Self>> {
        let index_path = Self::index_path(db_path);
        
        if !index_path.exists() {
            return Ok(None);
        }

        let file = File::open(&index_path)
            .with_context(|| format!("Failed to open index file: {:?}", index_path))?;
        
        let reader = BufReader::new(file);
        let index: Self = serde_json::from_reader(reader)
            .with_context(|| format!("Failed to parse index file: {:?}", index_path))?;

        Ok(Some(index))
    }

    /// Save index to file
    pub fn save(&self, db_path: &Path) -> Result<()> {
        let index_path = Self::index_path(db_path);
        
        let file = File::create(&index_path)
            .with_context(|| format!("Failed to create index file: {:?}", index_path))?;
        
        serde_json::to_writer_pretty(file, self)
            .with_context(|| format!("Failed to write index file: {:?}", index_path))?;

        Ok(())
    }

    /// Check if index is stale (database modified after index)
    pub fn is_stale(&self, db_path: &Path) -> Result<bool> {
        let metadata = std::fs::metadata(db_path)
            .with_context(|| format!("Failed to get database metadata: {:?}", db_path))?;
        
        let db_modified = metadata.modified()
            .map(|t| t.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs() as i64)
            .unwrap_or(0);

        Ok(db_modified > self.built_at)
    }
}

/// Build context index for a database
///
/// # Arguments
/// * `graph` - Magellan code graph
/// * `db_path` - Path to the database
///
/// # Returns
/// The built context index
pub fn build_context_index(graph: &mut CodeGraph, db_path: &Path) -> Result<ContextIndex> {
    println!("Building context index...");
    
    // Build project summary
    let summary = get_project_summary(graph)?;
    
    println!("  Project: {} {}", summary.name, summary.version);
    println!("  Language: {}", summary.language);
    println!("  Files: {}", summary.total_files);
    println!("  Symbols: {}", summary.total_symbols);
    println!("    Functions: {}", summary.symbol_counts.functions);
    println!("    Structs: {}", summary.symbol_counts.structs);
    println!("    Traits: {}", summary.symbol_counts.traits);
    println!("    Enums: {}", summary.symbol_counts.enums);

    // Create and save index
    let index = ContextIndex::new(db_path, summary);
    index.save(db_path)?;

    let index_path = ContextIndex::index_path(db_path);
    println!("Index saved to: {:?}", index_path);

    Ok(index)
}

/// Get or build context index
///
/// If index exists and is fresh, load it. Otherwise, build a new one.
pub fn get_or_build_context_index(graph: &mut CodeGraph, db_path: &Path) -> Result<ContextIndex> {
    // Try to load existing index
    if let Some(index) = ContextIndex::load(db_path)? {
        // Check if index is stale
        if !index.is_stale(db_path)? {
            println!("Using existing context index");
            return Ok(index);
        }
        println!("Index is stale, rebuilding...");
    }

    // Build new index
    build_context_index(graph, db_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_context_index_save_load() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        
        // Create a dummy database file
        File::create(&db_path).unwrap();

        let summary = ProjectSummary {
            name: "test".to_string(),
            version: "0.1.0".to_string(),
            language: "Rust".to_string(),
            total_files: 10,
            total_symbols: 100,
            symbol_counts: Default::default(),
            entry_points: vec![],
            description: "test".to_string(),
        };

        let index = ContextIndex::new(&db_path, summary);
        
        // Save
        index.save(&db_path).unwrap();
        
        // Load
        let loaded = ContextIndex::load(&db_path).unwrap();
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().summary.name, "test");
    }

    #[test]
    fn test_index_path_generation() {
        let db_path = Path::new("/path/to/code.db");
        let index_path = ContextIndex::index_path(db_path);
        assert_eq!(index_path, Path::new("/path/to/code.context.json"));
    }
}
