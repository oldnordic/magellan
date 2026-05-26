// Minimal LCOV tracefile parser — replaces the `lcov` crate dependency
// Reads standard LCOV text format (SF:, DA:, BRDA:, end_of_record)
//
// SAFETY: None; this is pure string parsing with no unsafe blocks.

use anyhow::{Context, Result};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use crate::ingest_coverage_cmd::LcovData;

/// Parse an LCOV tracefile into line and branch hit maps.
///
/// Supports standard record types:
/// - `SF:<path>` — source file
/// - `DA:<line>,<count>` — line hit data
/// - `BRDA:<line>,<block>,<branch>,<taken>` — branch data
/// - `end_of_record` — flush current record block
///
/// Lines that do not match these prefixes are silently skipped.
pub fn parse_lcov_file(path: &Path) -> Result<LcovData> {
    let file = File::open(path).with_context(|| format!("Failed to open LCOV file: {:?}", path))?;
    let reader = BufReader::new(file);

    let mut data = LcovData::default();
    let mut current_file = String::new();

    for line in reader.lines() {
        let line = line.with_context(|| "Failed to read LCOV file line")?;
        if line.is_empty() {
            continue;
        }

        if let Some(val) = line.strip_prefix("SF:") {
            current_file = val.to_string();
            continue;
        }

        if let Some(rest) = line.strip_prefix("DA:") {
            let mut parts = rest.splitn(2, ',');
            let line_num: u32 = parts
                .next()
                .and_then(|s| s.parse().ok())
                .ok_or_else(|| anyhow::anyhow!("Invalid DA line: {}", line))?;
            let count: u64 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);

            if !current_file.is_empty() {
                data.line_hits
                    .insert((current_file.clone(), line_num), count);
            }
            continue;
        }

        if let Some(rest) = line.strip_prefix("BRDA:") {
            let mut parts = rest.splitn(4, ',');
            let line_num: u32 = parts
                .next()
                .and_then(|s| s.parse().ok())
                .ok_or_else(|| anyhow::anyhow!("Invalid BRDA line: {}", line))?;
            let _block = parts.next(); // ignored
            let _branch = parts.next(); // ignored
            let taken: i64 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
            let taken_val = if taken < 0 { 0 } else { taken as u64 };

            if !current_file.is_empty() {
                let key = (current_file.clone(), line_num);
                data.branch_hits
                    .entry(key)
                    .and_modify(|v: &mut u64| *v = (*v).max(taken_val))
                    .or_insert(taken_val);
            }
            continue;
        }

        // end_of_record resets current_file context for future records
        if line == "end_of_record" {
            current_file.clear();
        }
        // All other lines (LH, LF, FNF, etc.) are silently ignored
    }

    Ok(data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_parse_basic_lcov() {
        let content = r#"SF:/home/user/project/src/main.rs
DA:1,1
DA:2,0
DA:10,5
end_of_record
SF:/home/user/project/src/lib.rs
DA:3,2
BRDA:3,0,0,1
BRDA:3,0,1,0
end_of_record
"#;
        let mut tmp = NamedTempFile::new().unwrap();
        tmp.write_all(content.as_bytes()).unwrap();
        let tmp_path = tmp.into_temp_path();

        let data = parse_lcov_file(&tmp_path).unwrap();

        assert_eq!(data.line_hits.len(), 4);
        assert_eq!(
            data.line_hits
                .get(&("/home/user/project/src/main.rs".to_string(), 1u32)),
            Some(&1)
        );
        assert_eq!(
            data.line_hits
                .get(&("/home/user/project/src/main.rs".to_string(), 2u32)),
            Some(&0)
        );
        assert_eq!(
            data.line_hits
                .get(&("/home/user/project/src/main.rs".to_string(), 10u32)),
            Some(&5)
        );
        assert_eq!(
            data.line_hits
                .get(&("/home/user/project/src/lib.rs".to_string(), 3u32)),
            Some(&2)
        );

        assert_eq!(data.branch_hits.len(), 1);
        // Branch 0 had taken=1 and Branch 1 had taken=0; max should be 1
        assert_eq!(
            data.branch_hits
                .get(&("/home/user/project/src/lib.rs".to_string(), 3u32)),
            Some(&1)
        );
    }

    #[test]
    fn test_parse_empty_file() {
        let mut tmp = NamedTempFile::new().unwrap();
        tmp.write_all(b"").unwrap();
        let tmp_path = tmp.into_temp_path();
        let data = parse_lcov_file(&tmp_path).unwrap();
        assert!(data.line_hits.is_empty());
        assert!(data.branch_hits.is_empty());
    }

    #[test]
    fn test_parse_skips_unknown_lines() {
        let content = r#"TN:
SF:/src/a.rs
LH:1
DA:1,1
LF:2
end_of_record
"#;
        let mut tmp = NamedTempFile::new().unwrap();
        tmp.write_all(content.as_bytes()).unwrap();
        let tmp_path = tmp.into_temp_path();
        let data = parse_lcov_file(&tmp_path).unwrap();
        assert_eq!(
            data.line_hits.get(&("/src/a.rs".to_string(), 1u32)),
            Some(&1)
        );
        assert!(data.branch_hits.is_empty());
    }
}
