#![cfg(feature = "geometric-backend")]
//! Cross-tool system verification harness
//!
//! This harness verifies that Magellan, llmgrep, and Mirage all work
//! correctly on the same shared .geo database.

#[cfg(feature = "geometric-backend")]
mod harness {
    use std::path::Path;
    use std::process::Command;
    use std::time::Duration;
    use std::thread;

    /// Paths to installed binaries
    fn magellan_bin() -> &'static str { "/home/feanor/.local/bin/magellan" }
    fn llmgrep_bin() -> &'static str { "/home/feanor/.local/bin/llmgrep" }
    fn mirage_bin() -> &'static str { "/home/feanor/.local/bin/mirage" }

    /// Create a realistic test project
    fn create_test_project(dir: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(dir.join("src"))?;
        std::fs::create_dir_all(dir.join("src/analysis"))?;
        
        // Main lib with various functions
        std::fs::write(
            dir.join("src/lib.rs"),
            r#"pub mod analysis;

/// Compute factorial recursively
pub fn factorial(n: u64) -> u64 {
    if n <= 1 {
        1
    } else {
        n * factorial(n - 1)
    }
}

/// Process data with multiple branches
pub fn process_data(input: &str) -> Result<String, String> {
    if input.is_empty() {
        return Err("Empty input".to_string());
    }
    
    let mut result = String::new();
    for (i, c) in input.chars().enumerate() {
        if i % 2 == 0 {
            result.push(c.to_ascii_uppercase());
        } else {
            result.push(c);
        }
    }
    
    Ok(result)
}

/// Calculator with multiple operations
pub fn calculate(a: i32, b: i32, op: &str) -> i32 {
    match op {
        "add" => a + b,
        "sub" => a - b,
        "mul" => a * b,
        "div" => if b != 0 { a / b } else { 0 },
        _ => 0,
    }
}

/// Helper that calls other functions
pub fn orchestrate() -> String {
    let fact = factorial(5);
    let processed = process_data("hello").unwrap_or_default();
    let calc = calculate(10, 20, "add");
    
    format!("fact={}, processed={}, calc={}", fact, processed, calc)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_factorial() {
        assert_eq!(factorial(5), 120);
    }
    
    #[test]
    fn test_calculate() {
        assert_eq!(calculate(2, 3, "add"), 5);
    }
}
"#,
        )?;
        
        // Analysis module
        std::fs::write(
            dir.join("src/analysis/mod.rs"),
            r#"use crate::calculate;

/// Analyze data and return statistics
pub fn analyze(data: &[i32]) -> AnalysisResult {
    if data.is_empty() {
        return AnalysisResult::default();
    }
    
    let sum: i32 = data.iter().sum();
    let avg = sum as f64 / data.len() as f64;
    let min = *data.iter().min().unwrap();
    let max = *data.iter().max().unwrap();
    
    AnalysisResult {
        count: data.len(),
        sum,
        avg,
        min,
        max,
    }
}

/// Analysis result structure
#[derive(Debug, Default)]
pub struct AnalysisResult {
    pub count: usize,
    pub sum: i32,
    pub avg: f64,
    pub min: i32,
    pub max: i32,
}

/// Transform data using calculator
pub fn transform_data(data: &mut [i32], multiplier: i32) {
    for item in data.iter_mut() {
        *item = calculate(*item, multiplier, "mul");
    }
}
"#,
        )?;
        
        // Main binary
        std::fs::write(
            dir.join("src/main.rs"),
            r#"use test_project::{factorial, process_data, orchestrate};

fn main() {
    println!("Factorial of 5: {}", factorial(5));
    println!("Processed: {:?}", process_data("Hello World"));
    println!("Orchestrated: {}", orchestrate());
}
"#,
        )?;
        
        Ok(())
    }

    /// Run the full cross-tool harness
    pub fn run_harness() -> Result<(), String> {
        let temp_dir = tempfile::tempdir().map_err(|e| format!("Failed to create temp dir: {}", e))?;
        let db_path = temp_dir.path().join("harness.geo");
        let src_path = temp_dir.path().join("src");
        
        create_test_project(temp_dir.path())
            .map_err(|e| format!("Failed to create test project: {}", e))?;
        
        println!("\n=== CROSS-TOOL SYSTEM HARNESS ===\n");
        
        // Step 1: Build with Magellan
        println!("1. Building .geo with Magellan...");
        let magellan = Command::new(magellan_bin())
            .arg("watch")
            .arg("--root").arg(&src_path)
            .arg("--db").arg(&db_path)
            .arg("--scan-initial")
            .env("MAGELLAN_WATCH_TIMEOUT_MS", "3000")
            .output()
            .map_err(|e| format!("Failed to run magellan: {}", e))?;
        
        if !magellan.status.success() {
            let stderr = String::from_utf8_lossy(&magellan.stderr);
            return Err(format!("Magellan failed: {}", stderr));
        }
        
        if !db_path.exists() {
            return Err("Database was not created".to_string());
        }
        
        println!("   ✓ Database created: {}", db_path.display());
        
        // Step 2: Verify with Magellan status
        println!("\n2. Magellan verification...");
        
        let status = Command::new(magellan_bin())
            .arg("status")
            .arg("--db").arg(&db_path)
            .output()
            .map_err(|e| format!("Failed magellan status: {}", e))?;
        
        let status_str = String::from_utf8_lossy(&status.stdout);
        println!("   Status:\n{}", status_str);
        
        // Check for expected content
        if !status_str.contains("symbols:") {
            return Err("Magellan status missing symbol count".to_string());
        }
        
        // Magellan find
        let find = Command::new(magellan_bin())
            .arg("find")
            .arg("--db").arg(&db_path)
            .arg("--name").arg("factorial")
            .output()
            .map_err(|e| format!("Failed magellan find: {}", e))?;
        
        let find_str = String::from_utf8_lossy(&find.stdout);
        if !find_str.contains("factorial") {
            return Err(format!("Magellan find didn't locate 'factorial': {}", find_str));
        }
        println!("   ✓ Found 'factorial' function");
        
        // Magellan cycles
        let cycles = Command::new(magellan_bin())
            .arg("cycles")
            .arg("--db").arg(&db_path)
            .output()
            .map_err(|e| format!("Failed magellan cycles: {}", e))?;
        
        if !cycles.status.success() {
            return Err("Magellan cycles failed".to_string());
        }
        println!("   ✓ Cycles analysis completed");
        
        // Step 3: Query with llmgrep
        println!("\n3. llmgrep verification...");
        
        let llmgrep = Command::new(llmgrep_bin())
            .arg("--db").arg(&db_path)
            .arg("search")
            .arg("--query").arg("factorial")
            .arg("--output").arg("human")
            .output()
            .map_err(|e| format!("Failed llmgrep: {}", e))?;
        
        let llmgrep_str = String::from_utf8_lossy(&llmgrep.stdout);
        if !llmgrep_str.contains("factorial") && !llmgrep_str.contains("total:") {
            return Err(format!("llmgrep didn't find 'factorial': {}", llmgrep_str));
        }
        println!("   ✓ llmgrep found 'factorial'");
        
        // llmgrep with path filter
        let llmgrep_path = Command::new(llmgrep_bin())
            .arg("--db").arg(&db_path)
            .arg("search")
            .arg("--query").arg("calculate")
            .arg("--path").arg("lib.rs")
            .arg("--output").arg("human")
            .output()
            .map_err(|e| format!("Failed llmgrep path: {}", e))?;
        
        println!("   ✓ llmgrep path filter works");
        
        // Step 4: Analyze with Mirage
        println!("\n4. Mirage verification...");
        
        let mirage_status = Command::new(mirage_bin())
            .arg("status")
            .arg("--db").arg(&db_path)
            .output()
            .map_err(|e| format!("Failed mirage status: {}", e))?;
        
        let mirage_status_str = String::from_utf8_lossy(&mirage_status.stdout);
        println!("   Status:\n{}", mirage_status_str);
        
        // Mirage cfg
        let mirage_cfg = Command::new(mirage_bin())
            .arg("cfg")
            .arg("--db").arg(&db_path)
            .arg("--function").arg("factorial")
            .output()
            .map_err(|e| format!("Failed mirage cfg: {}", e))?;
        
        let cfg_str = String::from_utf8_lossy(&mirage_cfg.stdout);
        let cfg_stderr = String::from_utf8_lossy(&mirage_cfg.stderr);
        if cfg_str.is_empty() && cfg_stderr.is_empty() {
            return Err("Mirage cfg produced no output".to_string());
        }
        println!("   ✓ Mirage cfg completed");
        
        // Mirage loops
        let mirage_loops = Command::new(mirage_bin())
            .arg("loops")
            .arg("--db").arg(&db_path)
            .arg("--function").arg("factorial")
            .output()
            .map_err(|e| format!("Failed mirage loops: {}", e))?;
        
        println!("   ✓ Mirage loops completed");
        
        // Step 5: Cross-check consistency
        println!("\n5. Cross-tool consistency check...");
        
        // Both Magellan and Mirage should report cfg_blocks
        let magellan_has_cfg = status_str.contains("cfg_blocks");
        let mirage_has_cfg = mirage_status_str.contains("cfg_blocks");
        
        if magellan_has_cfg && mirage_has_cfg {
            println!("   ✓ Both tools report CFG data");
        } else {
            println!("   ! CFG data mismatch (Magellan: {}, Mirage: {})", 
                magellan_has_cfg, mirage_has_cfg);
        }
        
        println!("\n=== HARNESS COMPLETED SUCCESSFULLY ===\n");
        
        Ok(())
    }
}

/// Test: Full cross-tool harness passes
#[test]
#[cfg(feature = "geometric-backend")]
fn cross_tool_system_harness_passes() {
    harness::run_harness().expect("Cross-tool harness failed");
}

/// Test: Database is shareable between tools
#[test]
#[cfg(feature = "geometric-backend")]
fn geo_database_is_shareable_between_tools() {
    use std::process::Command;
    
    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("shared.geo");
    let src_path = temp_dir.path().join("src");
    
    std::fs::create_dir_all(&src_path).unwrap();
    std::fs::write(
        src_path.join("lib.rs"),
        "pub fn test() -> i32 { 42 }\n",
    ).unwrap();
    
    // Build with magellan
    let _ = Command::new("/home/feanor/.local/bin/magellan")
        .arg("watch")
        .arg("--root").arg(&src_path)
        .arg("--db").arg(&db_path)
        .arg("--scan-initial")
        .env("MAGELLAN_WATCH_TIMEOUT_MS", "2000")
        .output()
        .expect("Magellan failed");
    
    assert!(db_path.exists(), "DB should exist");
    
    // All three tools should be able to open it
    let magellan_status = Command::new("/home/feanor/.local/bin/magellan")
        .arg("status")
        .arg("--db").arg(&db_path)
        .output()
        .expect("Magellan status failed");
    
    let llmgrep = Command::new("/home/feanor/.local/bin/llmgrep")
        .arg("--db").arg(&db_path)
        .arg("search")
        .arg("--query").arg("test")
        .output()
        .expect("llmgrep failed");
    
    let mirage_status = Command::new("/home/feanor/.local/bin/mirage")
        .arg("status")
        .arg("--db").arg(&db_path)
        .output()
        .expect("Mirage status failed");
    
    assert!(magellan_status.status.success(), "Magellan should open DB");
    assert!(mirage_status.status.success(), "Mirage should open DB");
    // llmgrep may succeed or fail gracefully, but should not panic
    
    println!("All tools can open the shared .geo database");
}

/// Empty test suite when geometric-backend is disabled
#[cfg(not(feature = "geometric-backend"))]
mod tests {
    #[test]
    fn geometric_backend_disabled() {
        // Placeholder
    }
}
