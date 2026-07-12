use anyhow::Result;
use rusqlite::{Connection, Row};

use crate::generation::CodeChunk;
use crate::graph::schema::CrossFileRef;
use crate::graph::side_tables::{
    CodeContentSearchResult, ExecutionRecord, FileMetrics, SymbolMetrics,
};

use super::AstNode;

pub(super) fn ensure_schema(conn: &Connection) -> Result<()> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS file_metrics (
                file_path TEXT PRIMARY KEY,
                symbol_count INTEGER DEFAULT 0,
                loc INTEGER DEFAULT 0,
                estimated_loc REAL DEFAULT 0,
                fan_in INTEGER DEFAULT 0,
                fan_out INTEGER DEFAULT 0,
                complexity_score REAL DEFAULT 0,
                last_updated INTEGER NOT NULL
            )",
        [],
    )?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS symbol_metrics (
                symbol_id INTEGER PRIMARY KEY,
                symbol_name TEXT NOT NULL,
                kind TEXT NOT NULL,
                file_path TEXT NOT NULL,
                loc INTEGER DEFAULT 0,
                estimated_loc REAL DEFAULT 0,
                fan_in INTEGER DEFAULT 0,
                fan_out INTEGER DEFAULT 0,
                cyclomatic_complexity INTEGER DEFAULT 0,
                last_updated INTEGER NOT NULL
            )",
        [],
    )?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS execution_log (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                execution_id TEXT NOT NULL UNIQUE,
                tool_version TEXT NOT NULL,
                args TEXT NOT NULL,
                root TEXT,
                db_path TEXT NOT NULL,
                started_at INTEGER NOT NULL,
                finished_at INTEGER,
                duration_ms INTEGER,
                outcome TEXT NOT NULL,
                error_message TEXT,
                files_indexed INTEGER DEFAULT 0,
                symbols_indexed INTEGER DEFAULT 0,
                references_indexed INTEGER DEFAULT 0
            )",
        [],
    )?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_execution_log_started_at ON execution_log(started_at DESC)",
        [],
    )?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_execution_log_execution_id ON execution_log(execution_id)",
        [],
    )?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_execution_log_outcome ON execution_log(outcome)",
        [],
    )?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_symbol_metrics_file_path ON symbol_metrics(file_path)",
        [],
    )?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS cross_file_refs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                from_symbol_id TEXT NOT NULL,
                to_symbol_id TEXT NOT NULL,
                file_path TEXT NOT NULL,
                line_number INTEGER NOT NULL,
                byte_start INTEGER NOT NULL,
                byte_end INTEGER NOT NULL
            )",
        [],
    )?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_cross_file_refs_to ON cross_file_refs(to_symbol_id)",
        [],
    )?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_cross_file_refs_from ON cross_file_refs(from_symbol_id)",
        [],
    )?;
    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_cross_file_refs_file ON cross_file_refs(file_path)",
        [],
    )?;

    Ok(())
}

pub(super) fn execution_record_from_row(row: &Row<'_>) -> rusqlite::Result<ExecutionRecord> {
    Ok(ExecutionRecord {
        id: row.get(0)?,
        execution_id: row.get(1)?,
        tool_version: row.get(2)?,
        args: row.get(3)?,
        root: row.get(4)?,
        db_path: row.get(5)?,
        started_at: row.get(6)?,
        finished_at: row.get(7)?,
        duration_ms: row.get(8)?,
        outcome: row.get(9)?,
        error_message: row.get(10)?,
        files_indexed: row.get(11)?,
        symbols_indexed: row.get(12)?,
        references_indexed: row.get(13)?,
    })
}

pub(super) fn file_metrics_from_row(row: &Row<'_>) -> rusqlite::Result<FileMetrics> {
    Ok(FileMetrics {
        file_path: row.get(0)?,
        symbol_count: row.get(1)?,
        loc: row.get(2)?,
        estimated_loc: row.get(3)?,
        fan_in: row.get(4)?,
        fan_out: row.get(5)?,
        complexity_score: row.get(6)?,
        last_updated: row.get(7)?,
    })
}

pub(super) fn symbol_metrics_from_row(row: &Row<'_>) -> rusqlite::Result<SymbolMetrics> {
    Ok(SymbolMetrics {
        symbol_id: row.get(0)?,
        symbol_name: row.get(1)?,
        kind: row.get(2)?,
        file_path: row.get(3)?,
        loc: row.get(4)?,
        estimated_loc: row.get(5)?,
        fan_in: row.get(6)?,
        fan_out: row.get(7)?,
        cyclomatic_complexity: row.get(8)?,
        last_updated: row.get(9)?,
    })
}

pub(super) fn code_chunk_from_row(row: &Row<'_>) -> rusqlite::Result<CodeChunk> {
    Ok(CodeChunk {
        id: Some(row.get(0)?),
        file_path: row.get(1)?,
        byte_start: row.get::<_, i64>(2)? as usize,
        byte_end: row.get::<_, i64>(3)? as usize,
        content: row.get(4)?,
        content_hash: row.get(5)?,
        symbol_name: row.get(6)?,
        symbol_kind: row.get(7)?,
        created_at: row.get(8)?,
    })
}

pub(super) fn ast_node_from_row(row: &Row<'_>) -> rusqlite::Result<AstNode> {
    Ok(AstNode {
        id: Some(row.get(0)?),
        parent_id: row.get(1)?,
        kind: row.get(2)?,
        byte_start: row.get::<_, i64>(3)? as usize,
        byte_end: row.get::<_, i64>(4)? as usize,
    })
}

pub(super) fn cross_file_ref_from_row(row: &Row<'_>) -> rusqlite::Result<CrossFileRef> {
    Ok(CrossFileRef {
        from_symbol_id: row.get(0)?,
        to_symbol_id: row.get(1)?,
        file_path: row.get(2)?,
        line_number: row.get::<_, i64>(3)? as usize,
        byte_start: row.get::<_, i64>(4)? as usize,
        byte_end: row.get::<_, i64>(5)? as usize,
    })
}

pub(super) fn code_content_search_result_from_row(
    row: &Row<'_>,
    pattern: &str,
) -> rusqlite::Result<CodeContentSearchResult> {
    let symbol_name: Option<String> = row.get(0)?;
    let symbol_kind: Option<String> = row.get(1)?;
    let file_path: String = row.get(2)?;
    let byte_start: i64 = row.get(3)?;
    let byte_end: i64 = row.get(4)?;
    let content: String = row.get(5)?;
    let rank: f64 = row.get(6)?;

    let start_line = content
        .char_indices()
        .take_while(|(i, _)| *i < byte_start as usize)
        .filter(|(_, c)| *c == '\n')
        .count()
        + 1;
    let end_line = content
        .char_indices()
        .take_while(|(i, _)| *i < byte_end as usize)
        .filter(|(_, c)| *c == '\n')
        .count()
        + 1;

    Ok(CodeContentSearchResult {
        symbol_name,
        symbol_kind,
        file_path,
        byte_start: byte_start as usize,
        byte_end: byte_end as usize,
        start_line,
        end_line,
        excerpt: build_excerpt(&content, pattern),
        rank,
    })
}

fn build_excerpt(content: &str, pattern: &str) -> String {
    let mut excerpt = content.chars().take(200).collect::<String>();
    for token in pattern.split_whitespace() {
        if let Some(pos) = content.find(token) {
            let start = pos.saturating_sub(80);
            let end = (pos + token.len() + 120).min(content.len());
            excerpt = content[start..end].to_string();
            break;
        }
    }
    excerpt
}
