use anyhow::Result;
use magellan::OutputFormat;
use std::path::PathBuf;

// ============================================================================
// Command Parsers - Individual command parsing functions
// ============================================================================

// ============================================================================
// Core Parsers
// ============================================================================

/// Helper to parse a required string argument
///
/// Returns the next argument value and increments index by 2,
/// or returns an error if no value is provided.
pub fn parse_required_arg(args: &[String], i: &mut usize, flag: &str) -> Result<String> {
    if *i + 1 >= args.len() {
        return Err(anyhow::anyhow!("{} requires an argument", flag));
    }
    let value = args[*i + 1].clone();
    *i += 2;
    Ok(value)
}

/// Helper to parse output format from string
///
/// Accepts: "human", "json", "pretty"
pub fn parse_output_format(value: &str) -> Result<OutputFormat> {
    match value {
        "human" => Ok(OutputFormat::Human),
        "json" => Ok(OutputFormat::Json),
        "pretty" => Ok(OutputFormat::Pretty),
        _ => Err(anyhow::anyhow!(
            "Invalid output format: {}. Must be human, json, or pretty",
            value
        )),
    }
}

/// Helper to parse a `PathBuf` argument
pub fn parse_path_arg(args: &[String], i: &mut usize, flag: &str) -> Result<PathBuf> {
    let value = parse_required_arg(args, i, flag)?;
    Ok(PathBuf::from(value))
}

/// Parse comma-separated DB paths or discover .db files in a directory
pub fn parse_db_paths(value: &str) -> Result<Vec<PathBuf>> {
    let path = PathBuf::from(value);
    if path.is_dir() {
        // Discover all .db files in the directory
        let mut paths = Vec::new();
        for entry in std::fs::read_dir(&path)? {
            let entry = entry?;
            let p = entry.path();
            if p.extension().is_some_and(|e| e == "db") {
                paths.push(p);
            }
        }
        paths.sort();
        Ok(paths)
    } else if value.contains(',') {
        Ok(value.split(',').map(PathBuf::from).collect())
    } else {
        Ok(vec![path])
    }
}
