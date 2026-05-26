//! Source inventory for graph memory — Phase 1
//!
//! Deterministic extraction of source document metadata:
//! - frontmatter (YAML between `---` delimiters)
//! - title (first `# ` heading or frontmatter `title` field)
//! - wikilinks (`[[...]]` pattern)
//! - tags (frontmatter `tags` or `#tag` inline patterns)
//! - content hash (xxHash3-128 for change detection)
//!
//! No LLM extraction. No candidate facts. Just source inventory.

use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use xxhash_rust::xxh3::Xxh3;

/// A source document tracked for graph memory extraction.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct SourceDocument {
    pub id: i64,
    pub path_or_uri: String,
    pub source_kind: String,
    pub content_hash: String,
    pub observed_at: i64,
    pub source_timestamp: Option<i64>,
    pub title: Option<String>,
    pub author: Option<String>,
    pub tags: Vec<String>,
    pub wikilinks: Vec<String>,
    pub frontmatter: Option<String>,
}

impl SourceDocument {
    /// Create a new source document with default id (0, auto-assigned by DB).
    pub fn new(path_or_uri: String, source_kind: String, content_hash: String) -> Self {
        let observed_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        Self {
            id: 0,
            path_or_uri,
            source_kind,
            content_hash,
            observed_at,
            source_timestamp: None,
            title: None,
            author: None,
            tags: Vec::new(),
            wikilinks: Vec::new(),
            frontmatter: None,
        }
    }
}

/// Result of scanning a directory.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct ScanResult {
    pub scanned: usize,
    pub inserted: usize,
    pub updated: usize,
    pub unchanged: usize,
    pub errors: Vec<String>,
}

/// Compute xxHash3-128 of content.
pub fn compute_hash(content: &[u8]) -> String {
    let mut hasher = Xxh3::new();
    hasher.update(content);
    format!("{:032x}", hasher.digest())
}

/// Extract YAML frontmatter from markdown content.
///
/// Returns `(frontmatter_yaml, remaining_content)` if frontmatter is found.
/// Frontmatter is delimited by `---` on its own line at the start of the file.
pub fn extract_frontmatter(content: &str) -> Option<(String, String)> {
    let lines: Vec<&str> = content.lines().collect();
    if lines.is_empty() || lines[0].trim() != "---" {
        return None;
    }

    // Find closing ---
    let mut close_idx = None;
    for (i, line) in lines.iter().enumerate().skip(1) {
        if line.trim() == "---" {
            close_idx = Some(i);
            break;
        }
    }

    let close_idx = close_idx?;
    let frontmatter = lines[1..close_idx].join("\n");
    let body = lines[close_idx + 1..].join("\n");

    Some((frontmatter, body))
}

/// Extract title from markdown content.
///
/// Priority:
/// 1. `title` field from parsed frontmatter
/// 2. First `# ` heading
/// 3. Filename (without extension) as fallback
pub fn extract_title(content: &str, frontmatter: Option<&serde_json::Value>) -> Option<String> {
    // Priority 1: frontmatter title
    if let Some(fm) = frontmatter {
        if let Some(title) = fm.get("title").and_then(|v| v.as_str()) {
            return Some(title.trim().to_string());
        }
    }

    // Priority 2: first # heading
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("# ") {
            return Some(rest.trim().to_string());
        }
    }

    None
}

/// Extract wikilinks from content.
///
/// Matches `[[...]]` pattern. Returns deduplicated links in order of first appearance.
pub fn extract_wikilinks(content: &str) -> Vec<String> {
    let mut links = Vec::new();
    let mut seen = HashSet::new();

    for line in content.lines() {
        let mut rest = line;
        while let Some(start) = rest.find("[[") {
            rest = &rest[start + 2..];
            if let Some(end) = rest.find("]]") {
                let link = rest[..end].trim().to_string();
                if !link.is_empty() && seen.insert(link.clone()) {
                    links.push(link);
                }
                rest = &rest[end + 2..];
            } else {
                break;
            }
        }
    }

    links
}

/// Extract tags from content.
///
/// Priority:
/// 1. `tags` field from parsed frontmatter (array or comma-separated string)
/// 2. Inline `#tag` patterns (excludes hex colors like #fff, #aabbcc)
pub fn extract_tags(content: &str, frontmatter: Option<&serde_json::Value>) -> Vec<String> {
    let mut tags = Vec::new();
    let mut seen = HashSet::new();

    // Priority 1: frontmatter tags
    if let Some(fm) = frontmatter {
        if let Some(tag_array) = fm.get("tags").and_then(|v| v.as_array()) {
            for tag in tag_array {
                if let Some(s) = tag.as_str() {
                    let t = s.trim().to_string();
                    if !t.is_empty() && seen.insert(t.clone()) {
                        tags.push(t);
                    }
                }
            }
        } else if let Some(tag_str) = fm.get("tags").and_then(|v| v.as_str()) {
            for tag in tag_str.split(',') {
                let t = tag.trim().to_string();
                if !t.is_empty() && seen.insert(t.clone()) {
                    tags.push(t);
                }
            }
        }
    }

    // Priority 2: inline #tag patterns
    for word in content.split_whitespace() {
        let word = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '#');
        if word.starts_with('#') && word.len() > 1 {
            let tag = &word[1..];
            // Exclude hex colors
            if !is_hex_color(tag) && seen.insert(tag.to_string()) {
                tags.push(tag.to_string());
            }
        }
    }

    tags
}

/// Check if a string looks like a hex color (3 or 6 hex digits).
fn is_hex_color(s: &str) -> bool {
    let s = s.trim();
    (s.len() == 3 || s.len() == 6) && s.chars().all(|c| c.is_ascii_hexdigit())
}

/// Parse frontmatter YAML into a JSON value.
pub fn parse_frontmatter(yaml: &str) -> Option<serde_json::Value> {
    // Simple key-value parsing for common frontmatter patterns.
    // This is not a full YAML parser — it handles `key: value` and `key: [a, b]`.
    let mut map = serde_json::Map::new();

    for line in yaml.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if let Some(colon_idx) = line.find(':') {
            let key = line[..colon_idx].trim().to_string();
            let value_str = line[colon_idx + 1..].trim();

            if value_str.starts_with('[') && value_str.ends_with(']') {
                // Array: [a, b, c]
                let inner = &value_str[1..value_str.len() - 1];
                let arr: Vec<serde_json::Value> = inner
                    .split(',')
                    .map(|s| serde_json::Value::String(s.trim().to_string()))
                    .collect();
                map.insert(key, serde_json::Value::Array(arr));
            } else if value_str.starts_with('"') && value_str.ends_with('"') {
                // Quoted string
                map.insert(
                    key,
                    serde_json::Value::String(value_str[1..value_str.len() - 1].to_string()),
                );
            } else if value_str.starts_with('\'') && value_str.ends_with('\'') {
                // Single-quoted string
                map.insert(
                    key,
                    serde_json::Value::String(value_str[1..value_str.len() - 1].to_string()),
                );
            } else if let Ok(iv) = value_str.parse::<i64>() {
                map.insert(key, serde_json::Value::Number(serde_json::Number::from(iv)));
            } else if let Ok(fv) = value_str.parse::<f64>() {
                if let Some(n) = serde_json::Number::from_f64(fv) {
                    map.insert(key, serde_json::Value::Number(n));
                } else {
                    // Non-finite float (NaN, inf) — store as string
                    map.insert(key, serde_json::Value::String(value_str.to_string()));
                }
            } else {
                map.insert(key, serde_json::Value::String(value_str.to_string()));
            }
        }
    }

    if map.is_empty() {
        None
    } else {
        Some(serde_json::Value::Object(map))
    }
}

/// Extract all metadata from a markdown file's content.
pub fn extract_metadata(content: &str) -> ExtractedMetadata {
    let (frontmatter_yaml, body) = extract_frontmatter(content)
        .map(|(fm, body)| (Some(fm), body))
        .unwrap_or_else(|| (None, content.to_string()));

    let frontmatter_json = frontmatter_yaml
        .as_ref()
        .and_then(|fm| parse_frontmatter(fm));
    let title = extract_title(&body, frontmatter_json.as_ref())
        .or_else(|| extract_title(content, frontmatter_json.as_ref()));
    let wikilinks = extract_wikilinks(content);
    let tags = extract_tags(content, frontmatter_json.as_ref());
    let author = frontmatter_json
        .as_ref()
        .and_then(|fm| fm.get("author"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    ExtractedMetadata {
        title,
        author,
        tags,
        wikilinks,
        frontmatter: frontmatter_yaml,
    }
}

/// Metadata extracted from a source document.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ExtractedMetadata {
    pub title: Option<String>,
    pub author: Option<String>,
    pub tags: Vec<String>,
    pub wikilinks: Vec<String>,
    pub frontmatter: Option<String>,
}

// ── Database operations ──

/// Ensure the source_documents table exists.
pub fn ensure_schema(conn: &Connection) -> Result<()> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS source_documents (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            path_or_uri TEXT NOT NULL UNIQUE,
            source_kind TEXT NOT NULL,
            content_hash TEXT NOT NULL,
            observed_at INTEGER NOT NULL,
            source_timestamp INTEGER,
            title TEXT,
            author TEXT,
            tags TEXT,
            wikilinks TEXT,
            frontmatter TEXT
        )",
        [],
    )
    .context("create source_documents table")?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_source_docs_path ON source_documents(path_or_uri)",
        [],
    )
    .context("create path index")?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_source_docs_hash ON source_documents(content_hash)",
        [],
    )
    .context("create hash index")?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_source_docs_kind ON source_documents(source_kind)",
        [],
    )
    .context("create kind index")?;

    Ok(())
}

/// Insert or update a source document.
///
/// If a document with the same path exists and the hash differs, update it.
/// If the hash is the same, do nothing (idempotent).
/// Returns true if inserted/updated, false if unchanged.
pub fn insert_or_update(conn: &Connection, doc: &SourceDocument) -> Result<bool> {
    let existing: Option<(i64, String)> = conn
        .query_row(
            "SELECT id, content_hash FROM source_documents WHERE path_or_uri = ?1",
            params![&doc.path_or_uri],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()
        .context("query existing document")?;

    let tags_json = serde_json::to_string(&doc.tags).unwrap_or_else(|_| "[]".to_string());
    let wikilinks_json = serde_json::to_string(&doc.wikilinks).unwrap_or_else(|_| "[]".to_string());

    match existing {
        Some((id, existing_hash)) if existing_hash != doc.content_hash => {
            // Update: hash changed
            conn.execute(
                "UPDATE source_documents SET
                    source_kind = ?1,
                    content_hash = ?2,
                    observed_at = ?3,
                    source_timestamp = ?4,
                    title = ?5,
                    author = ?6,
                    tags = ?7,
                    wikilinks = ?8,
                    frontmatter = ?9
                WHERE id = ?10",
                params![
                    &doc.source_kind,
                    &doc.content_hash,
                    doc.observed_at,
                    doc.source_timestamp,
                    &doc.title,
                    &doc.author,
                    &tags_json,
                    &wikilinks_json,
                    &doc.frontmatter,
                    id,
                ],
            )
            .context("update source document")?;
            Ok(true)
        }
        Some(_) => {
            // Unchanged: same hash
            Ok(false)
        }
        None => {
            // Insert: new document
            conn.execute(
                "INSERT INTO source_documents
                    (path_or_uri, source_kind, content_hash, observed_at, source_timestamp, title, author, tags, wikilinks, frontmatter)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![
                    &doc.path_or_uri,
                    &doc.source_kind,
                    &doc.content_hash,
                    doc.observed_at,
                    doc.source_timestamp,
                    &doc.title,
                    &doc.author,
                    &tags_json,
                    &wikilinks_json,
                    &doc.frontmatter,
                ],
            ).context("insert source document")?;
            Ok(true)
        }
    }
}

/// List source documents, optionally filtered by kind.
pub fn list_by_kind(conn: &Connection, kind: Option<&str>) -> Result<Vec<SourceDocument>> {
    let sql = if kind.is_some() {
        "SELECT id, path_or_uri, source_kind, content_hash, observed_at, source_timestamp, title, author, tags, wikilinks, frontmatter
         FROM source_documents WHERE source_kind = ?1 ORDER BY path_or_uri"
    } else {
        "SELECT id, path_or_uri, source_kind, content_hash, observed_at, source_timestamp, title, author, tags, wikilinks, frontmatter
         FROM source_documents ORDER BY path_or_uri"
    };

    let mut stmt = conn.prepare(sql).context("prepare list query")?;

    let rows = if let Some(k) = kind {
        stmt.query_map(params![k], row_to_document)?
    } else {
        stmt.query_map([], row_to_document)?
    };

    let mut docs = Vec::new();
    for row in rows {
        docs.push(row.context("read document row")?);
    }

    Ok(docs)
}

/// Find documents whose content hash no longer matches the file on disk.
pub fn find_stale(conn: &Connection) -> Result<Vec<(SourceDocument, String)>> {
    let mut stmt = conn.prepare(
        "SELECT id, path_or_uri, source_kind, content_hash, observed_at, source_timestamp, title, author, tags, wikilinks, frontmatter
         FROM source_documents"
    ).context("prepare stale query")?;

    let rows = stmt.query_map([], row_to_document)?;

    let mut stale = Vec::new();
    for row in rows {
        let doc = row.context("read document row")?;
        let path = Path::new(&doc.path_or_uri);
        if path.exists() {
            if let Ok(content) = std::fs::read(path) {
                let current_hash = compute_hash(&content);
                if current_hash != doc.content_hash {
                    stale.push((doc, current_hash));
                }
            }
        }
    }

    Ok(stale)
}

fn row_to_document(row: &rusqlite::Row) -> rusqlite::Result<SourceDocument> {
    let tags_json: String = row.get(8)?;
    let wikilinks_json: String = row.get(9)?;

    Ok(SourceDocument {
        id: row.get(0)?,
        path_or_uri: row.get(1)?,
        source_kind: row.get(2)?,
        content_hash: row.get(3)?,
        observed_at: row.get(4)?,
        source_timestamp: row.get(5)?,
        title: row.get(6)?,
        author: row.get(7)?,
        tags: serde_json::from_str(&tags_json).unwrap_or_default(),
        wikilinks: serde_json::from_str(&wikilinks_json).unwrap_or_default(),
        frontmatter: row.get(10)?,
    })
}

/// Scan a directory for source documents and insert/update them.
pub fn scan_directory(
    conn: &Connection,
    dir: &Path,
    kind: &str,
    extension: &str,
) -> Result<ScanResult> {
    let mut result = ScanResult::default();

    if !dir.exists() {
        return Ok(result);
    }

    ensure_schema(conn).context("ensure schema")?;

    for entry in walkdir::WalkDir::new(dir)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        if let Some(ext) = path.extension() {
            if ext != extension {
                continue;
            }
        } else {
            continue;
        }

        result.scanned += 1;

        match scan_file(conn, path, kind) {
            Ok(true) => result.inserted += 1,
            Ok(false) => result.unchanged += 1,
            Err(e) => result.errors.push(format!("{}: {}", path.display(), e)),
        }
    }

    Ok(result)
}

/// Scan a single file and insert/update it.
/// Returns true if inserted/updated, false if unchanged.
pub fn scan_file(conn: &Connection, path: &Path, kind: &str) -> Result<bool> {
    let content = std::fs::read(path).with_context(|| format!("read file: {}", path.display()))?;
    let hash = compute_hash(&content);

    let content_str = String::from_utf8_lossy(&content);
    let metadata = extract_metadata(&content_str);

    let mut doc = SourceDocument::new(path.to_string_lossy().to_string(), kind.to_string(), hash);

    // Source timestamp from file mtime
    doc.source_timestamp = path
        .metadata()
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64);

    doc.title = metadata.title;
    doc.author = metadata.author;
    doc.tags = metadata.tags;
    doc.wikilinks = metadata.wikilinks;
    doc.frontmatter = metadata.frontmatter;

    insert_or_update(conn, &doc).context("insert or update document")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_hash_deterministic() {
        let data = b"hello world";
        let h1 = compute_hash(data);
        let h2 = compute_hash(data);
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 32); // xxHash3-128 = 32 hex chars
    }

    #[test]
    fn test_compute_hash_different_inputs() {
        let h1 = compute_hash(b"hello");
        let h2 = compute_hash(b"world");
        assert_ne!(h1, h2);
    }

    #[test]
    fn test_extract_frontmatter_present() {
        let content = "---\ntitle: Test Page\ntags: [a, b]\n---\n\n# Body\n\nSome text.";
        let (fm, body) = extract_frontmatter(content).unwrap();
        assert!(fm.contains("title: Test Page"));
        assert!(body.contains("# Body"));
        assert!(!body.contains("---"));
    }

    #[test]
    fn test_extract_frontmatter_missing() {
        let content = "# No Frontmatter\n\nJust body.";
        assert!(extract_frontmatter(content).is_none());
    }

    #[test]
    fn test_extract_title_from_heading() {
        let content = "# My Title\n\nSome body.";
        assert_eq!(extract_title(content, None), Some("My Title".to_string()));
    }

    #[test]
    fn test_extract_title_from_frontmatter() {
        let fm = serde_json::json!({"title": "FM Title"});
        let content = "# Heading Title\n\nBody.";
        assert_eq!(
            extract_title(content, Some(&fm)),
            Some("FM Title".to_string())
        );
    }

    #[test]
    fn test_extract_wikilinks_basic() {
        let content = "See [[conceptA]] and [[conceptB]] for details.";
        let links = extract_wikilinks(content);
        assert_eq!(links, vec!["conceptA", "conceptB"]);
    }

    #[test]
    fn test_extract_wikilinks_dedup() {
        let content = "[[a]] and [[a]] again.";
        let links = extract_wikilinks(content);
        assert_eq!(links, vec!["a"]);
    }

    #[test]
    fn test_extract_tags_from_frontmatter_array() {
        let fm = serde_json::json!({"tags": ["rust", "graph"]});
        let tags = extract_tags("", Some(&fm));
        assert_eq!(tags, vec!["rust", "graph"]);
    }

    #[test]
    fn test_extract_tags_from_frontmatter_string() {
        let fm = serde_json::json!({"tags": "rust, graph"});
        let tags = extract_tags("", Some(&fm));
        assert_eq!(tags, vec!["rust", "graph"]);
    }

    #[test]
    fn test_extract_tags_inline() {
        let content = "This is about #rust and #graph and #memory.";
        let tags = extract_tags(content, None);
        assert!(tags.contains(&"rust".to_string()));
        assert!(tags.contains(&"graph".to_string()));
        assert!(tags.contains(&"memory".to_string()));
    }

    #[test]
    fn test_extract_tags_excludes_hex_colors() {
        let content = "Colors: #fff and #aabbcc are not tags.";
        let tags = extract_tags(content, None);
        assert!(!tags.contains(&"fff".to_string()));
        assert!(!tags.contains(&"aabbcc".to_string()));
    }

    #[test]
    fn test_parse_frontmatter_simple() {
        let yaml = "title: Hello\nauthor: Test\ncount: 42\n";
        let json = parse_frontmatter(yaml).unwrap();
        assert_eq!(json["title"], "Hello");
        assert_eq!(json["author"], "Test");
        assert_eq!(json["count"], 42);
    }

    #[test]
    fn test_parse_frontmatter_array() {
        let yaml = "tags: [rust, graph, memory]\n";
        let json = parse_frontmatter(yaml).unwrap();
        let tags = json["tags"].as_array().unwrap();
        assert_eq!(tags.len(), 3);
    }

    #[test]
    fn test_extract_metadata_full() {
        let content = r#"---
title: Test Article
author: Claude
tags: [rust, graph]
---

# Heading

See [[other-page]] for more.
Also check out #memory systems.
"#;

        let meta = extract_metadata(content);
        assert_eq!(meta.title, Some("Test Article".to_string()));
        assert_eq!(meta.author, Some("Claude".to_string()));
        assert_eq!(meta.tags, vec!["rust", "graph", "memory"]);
        assert_eq!(meta.wikilinks, vec!["other-page"]);
        assert!(meta.frontmatter.is_some());
    }

    #[test]
    fn test_database_roundtrip() {
        let conn = Connection::open_in_memory().unwrap();
        ensure_schema(&conn).unwrap();

        let mut doc = SourceDocument::new(
            "/home/test/wiki/page.md".to_string(),
            "wiki".to_string(),
            "abc123".to_string(),
        );
        doc.title = Some("Test Page".to_string());
        doc.author = Some("Author".to_string());
        doc.tags = vec!["rust".to_string(), "graph".to_string()];
        doc.wikilinks = vec!["other".to_string()];

        let inserted = insert_or_update(&conn, &doc).unwrap();
        assert!(inserted);

        // Idempotent re-insert
        let again = insert_or_update(&conn, &doc).unwrap();
        assert!(!again);

        // List
        let docs = list_by_kind(&conn, Some("wiki")).unwrap();
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].path_or_uri, "/home/test/wiki/page.md");
        assert_eq!(docs[0].tags, vec!["rust", "graph"]);
        assert_eq!(docs[0].wikilinks, vec!["other"]);

        // Update with different hash
        let mut doc2 = doc.clone();
        doc2.content_hash = "def456".to_string();
        doc2.title = Some("Updated Title".to_string());
        let updated = insert_or_update(&conn, &doc2).unwrap();
        assert!(updated);

        let docs = list_by_kind(&conn, Some("wiki")).unwrap();
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].title, Some("Updated Title".to_string()));
    }

    #[test]
    fn test_parse_frontmatter_f64_that_is_not_json_number() {
        // f64 values like NaN or Infinity cannot be represented as serde_json::Number
        // parse_frontmatter should not panic on these — it should fall through to string
        let yaml = "ratio: inf\n";
        let json = parse_frontmatter(yaml);
        // Should return Some with the value as a string, not panic
        assert!(
            json.is_some(),
            "parse_frontmatter should return Some for valid-looking frontmatter"
        );
        let json = json.unwrap();
        assert_eq!(json["ratio"], "inf");
    }

    #[test]
    fn test_parse_frontmatter_negative_number() {
        let yaml = "count: -5\n";
        let json = parse_frontmatter(yaml).unwrap();
        assert_eq!(json["count"], -5);
    }

    #[test]
    fn test_parse_frontmatter_float() {
        let yaml = "ratio: 2.5\n";
        let json = parse_frontmatter(yaml).unwrap();
        assert!((json["ratio"].as_f64().unwrap() - 2.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_database_list_all_kinds() {
        let conn = Connection::open_in_memory().unwrap();
        ensure_schema(&conn).unwrap();

        insert_or_update(
            &conn,
            &SourceDocument::new("/a.md".to_string(), "wiki".to_string(), "h1".to_string()),
        )
        .unwrap();
        insert_or_update(
            &conn,
            &SourceDocument::new(
                "/b.msg".to_string(),
                "message".to_string(),
                "h2".to_string(),
            ),
        )
        .unwrap();

        let all = list_by_kind(&conn, None).unwrap();
        assert_eq!(all.len(), 2);

        let wiki = list_by_kind(&conn, Some("wiki")).unwrap();
        assert_eq!(wiki.len(), 1);
    }
}
