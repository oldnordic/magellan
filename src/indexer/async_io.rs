//! Async file reading for Magellan
//!
//! Provides async file read utilities without requiring async graph operations.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::time::Duration;

use tokio::time::timeout;

/// Read file asynchronously
pub async fn read_file_async(path: &Path) -> Result<Vec<u8>> {
    tokio::fs::read(path)
        .await
        .with_context(|| format!("Failed to read file: {:?}", path))
}

/// Read multiple files asynchronously in parallel
pub async fn read_files_async(paths: Vec<PathBuf>) -> Result<Vec<(PathBuf, Vec<u8>)>> {
    let mut tasks = Vec::new();
    
    for path in paths {
        let task = tokio::spawn(async move {
            let content = read_file_async(&path).await?;
            Ok::<_, anyhow::Error>((path, content))
        });
        tasks.push(task);
    }

    let mut results = Vec::new();
    for task in tasks {
        match task.await {
            Ok(Ok(result)) => results.push(result),
            Ok(Err(e)) => eprintln!("Error reading file: {}", e),
            Err(e) => eprintln!("Task failed: {}", e),
        }
    }

    Ok(results)
}

/// Read file with timeout
pub async fn read_file_with_timeout(path: &Path, timeout_secs: u64) -> Result<Vec<u8>> {
    let result = timeout(
        Duration::from_secs(timeout_secs),
        read_file_async(path),
    )
    .await;

    match result {
        Ok(Ok(content)) => Ok(content),
        Ok(Err(e)) => Err(e),
        Err(_) => Err(anyhow::anyhow!(
            "File read timed out after {} seconds",
            timeout_secs
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_read_file_async() {
        let temp_dir = TempDir::new().unwrap();
        let test_file = temp_dir.path().join("test.txt");
        std::fs::write(&test_file, "test content").unwrap();

        let result = read_file_async(&test_file).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), b"test content");
    }

    #[tokio::test]
    async fn test_read_files_async() {
        let temp_dir = TempDir::new().unwrap();
        let mut paths = Vec::new();
        
        for i in 0..5 {
            let test_file = temp_dir.path().join(format!("test_{}.txt", i));
            std::fs::write(&test_file, format!("content {}", i)).unwrap();
            paths.push(test_file);
        }

        let result = read_files_async(paths).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 5);
    }
}
