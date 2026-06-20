use anyhow::Result;
use rusqlite::{params, OptionalExtension};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::common::normalize_repo_relative_path;
use crate::graph::symbols::stable_symbol_id_for_fact;
use crate::ingest::pool;
use crate::ingest::{detect_language, Language, Parser, SymbolFact};
use crate::references::CallFact;
use crate::CodeGraph;

#[derive(Debug, Clone)]
pub struct SnapshotFileInput {
    pub path: PathBuf,
    pub source: Vec<u8>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SnapshotIngestStats {
    pub files_total: usize,
    pub files_indexed: usize,
    pub files_skipped: usize,
    pub symbol_versions: usize,
    pub edge_versions: usize,
}

#[derive(Debug, Clone)]
struct StoredSymbolVersion {
    stable_id: String,
    name: String,
    kind: String,
    file_path: String,
    start_line: i64,
    start_col: i64,
    end_line: i64,
    end_col: i64,
    body_hash: Option<String>,
}

#[derive(Debug, Clone)]
struct StoredEdgeVersion {
    source_stable_id: String,
    target_stable_id: String,
    kind: String,
}

#[derive(Debug, Default)]
struct ParentSnapshotData {
    file_hashes: HashMap<String, String>,
    symbols_by_file: HashMap<String, Vec<StoredSymbolVersion>>,
    edges: Vec<StoredEdgeVersion>,
}

struct PreparedSnapshotFile {
    repo_relative_path: String,
    source: Vec<u8>,
    tree: Option<tree_sitter::Tree>,
    language: Option<Language>,
    symbol_facts: Vec<SymbolFact>,
    stored_symbols: Vec<StoredSymbolVersion>,
}

fn hash_symbol_body(source: &[u8], byte_start: usize, byte_end: usize) -> Option<String> {
    let bytes = source.get(byte_start..byte_end)?;
    let hash = blake3::hash(bytes).to_hex().to_string();
    Some(hash[..32].to_string())
}

fn extract_symbols_for_source(
    file_path: &Path,
    source: &[u8],
) -> Result<(Option<Language>, Option<tree_sitter::Tree>, Vec<SymbolFact>)> {
    use crate::ingest::c::CParser;
    use crate::ingest::cpp::CppParser;
    use crate::ingest::cuda::CudaParser;
    use crate::ingest::go::GoParser;
    use crate::ingest::java::JavaParser;
    use crate::ingest::javascript::JavaScriptParser;
    use crate::ingest::python::PythonParser;
    use crate::ingest::typescript::TypeScriptParser;

    let path_buf = file_path.to_path_buf();
    let language = detect_language(file_path);
    let parsed_tree = match language {
        Some(lang) => pool::with_parser(lang, |parser| parser.parse(source, None))?,
        None => None,
    };

    let symbols = match (language, &parsed_tree) {
        (Some(Language::Rust), Some(tree)) => {
            Parser::extract_symbols_from_tree(tree, path_buf.clone(), source)
        }
        (Some(Language::C), Some(tree)) => {
            CParser::extract_symbols_from_tree(tree, path_buf.clone(), source)
        }
        (Some(Language::Cpp), Some(tree)) => {
            CppParser::extract_symbols_from_tree(tree, path_buf.clone(), source)
        }
        (Some(Language::Java), Some(tree)) => {
            JavaParser::extract_symbols_from_tree(tree, path_buf.clone(), source)
        }
        (Some(Language::Python), Some(tree)) => {
            PythonParser::extract_symbols_from_tree(tree, path_buf.clone(), source)
        }
        (Some(Language::JavaScript), Some(tree)) => {
            JavaScriptParser::extract_symbols_from_tree(tree, path_buf.clone(), source)
        }
        (Some(Language::TypeScript), Some(tree)) => {
            TypeScriptParser::extract_symbols_from_tree(tree, path_buf.clone(), source)
        }
        (Some(Language::Go), Some(tree)) => {
            GoParser::extract_symbols_from_tree(tree, path_buf.clone(), source)
        }
        (Some(Language::Cuda), Some(tree)) => {
            CudaParser::extract_symbols_from_tree(tree, path_buf.clone(), source)
        }
        _ => Vec::new(),
    };

    Ok((language, parsed_tree, symbols))
}

fn extract_calls_for_source(
    language: Language,
    tree: &tree_sitter::Tree,
    file_path: &Path,
    source: &[u8],
    symbols: &[SymbolFact],
) -> Vec<CallFact> {
    use crate::ingest::c::CParser;
    use crate::ingest::cpp::CppParser;
    use crate::ingest::cuda::CudaParser;
    use crate::ingest::go::GoParser;
    use crate::ingest::java::JavaParser;
    use crate::ingest::javascript::JavaScriptParser;
    use crate::ingest::python::PythonParser;
    use crate::ingest::typescript::TypeScriptParser;

    let path_buf = file_path.to_path_buf();
    match language {
        Language::Rust => Parser::extract_calls_from_tree(tree, path_buf, source, symbols),
        Language::Python => PythonParser::extract_calls_from_tree(tree, path_buf, source, symbols),
        Language::C => CParser::extract_calls_from_tree(tree, path_buf, source, symbols),
        Language::Cpp => CppParser::extract_calls_from_tree(tree, path_buf, source, symbols),
        Language::Java => JavaParser::extract_calls_from_tree(tree, path_buf, source, symbols),
        Language::JavaScript => {
            JavaScriptParser::extract_calls_from_tree(tree, path_buf, source, symbols)
        }
        Language::TypeScript => {
            TypeScriptParser::extract_calls_from_tree(tree, path_buf, source, symbols)
        }
        Language::Go => GoParser::extract_calls_from_tree(tree, path_buf, source, symbols),
        Language::Cuda => CudaParser::extract_calls_from_tree(tree, path_buf, source, symbols),
    }
}

fn load_parent_snapshot_data(
    conn: &rusqlite::Connection,
    snapshot_id: i64,
) -> Result<Option<ParentSnapshotData>> {
    let parent_id: Option<i64> = conn
        .query_row(
            "SELECT rs.id
             FROM repo_snapshot_parents rsp
             JOIN repo_snapshots rs ON rs.commit_oid = rsp.parent_oid
             WHERE rsp.snapshot_id = ?1
             ORDER BY rs.id DESC
             LIMIT 1",
            params![snapshot_id],
            |row| row.get(0),
        )
        .optional()?;

    let Some(parent_id) = parent_id else {
        return Ok(None);
    };

    let mut data = ParentSnapshotData::default();

    let mut file_stmt = conn.prepare(
        "SELECT file_path, content_hash FROM file_versions WHERE snapshot_id = ?1 AND is_deleted = 0",
    )?;
    let file_rows = file_stmt.query_map(params![parent_id], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    for row in file_rows {
        let (file_path, content_hash) = row?;
        data.file_hashes.insert(file_path, content_hash);
    }

    let mut symbol_stmt = conn.prepare(
        "SELECT stable_id, name, kind, file_path, start_line, start_col, end_line, end_col, body_hash
         FROM symbol_versions
         WHERE snapshot_id = ?1",
    )?;
    let symbol_rows = symbol_stmt.query_map(params![parent_id], |row| {
        Ok(StoredSymbolVersion {
            stable_id: row.get(0)?,
            name: row.get(1)?,
            kind: row.get(2)?,
            file_path: row.get(3)?,
            start_line: row.get(4)?,
            start_col: row.get(5)?,
            end_line: row.get(6)?,
            end_col: row.get(7)?,
            body_hash: row.get(8)?,
        })
    })?;
    for row in symbol_rows {
        let symbol = row?;
        data.symbols_by_file
            .entry(symbol.file_path.clone())
            .or_default()
            .push(symbol);
    }

    let mut edge_stmt = conn.prepare(
        "SELECT source_stable_id, target_stable_id, kind
         FROM edge_versions
         WHERE snapshot_id = ?1",
    )?;
    let edge_rows = edge_stmt.query_map(params![parent_id], |row| {
        Ok(StoredEdgeVersion {
            source_stable_id: row.get(0)?,
            target_stable_id: row.get(1)?,
            kind: row.get(2)?,
        })
    })?;
    for row in edge_rows {
        data.edges.push(row?);
    }

    Ok(Some(data))
}

fn insert_file_version(
    conn: &rusqlite::Connection,
    snapshot_id: i64,
    file_path: &str,
    content_hash: &str,
    size_bytes: i64,
) -> Result<()> {
    conn.execute(
        "INSERT INTO file_versions (snapshot_id, file_path, content_hash, size_bytes, is_deleted)
         VALUES (?1, ?2, ?3, ?4, 0)",
        params![snapshot_id, file_path, content_hash, size_bytes],
    )?;
    Ok(())
}

fn insert_symbol_version(
    conn: &rusqlite::Connection,
    snapshot_id: i64,
    symbol: &StoredSymbolVersion,
) -> Result<()> {
    conn.execute(
        "INSERT INTO symbol_versions
         (snapshot_id, stable_id, name, kind, file_path, start_line, start_col, end_line, end_col, body_hash)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            snapshot_id,
            symbol.stable_id,
            symbol.name,
            symbol.kind,
            symbol.file_path,
            symbol.start_line,
            symbol.start_col,
            symbol.end_line,
            symbol.end_col,
            symbol.body_hash
        ],
    )?;
    Ok(())
}

fn insert_edge_version(
    conn: &rusqlite::Connection,
    snapshot_id: i64,
    edge: &StoredEdgeVersion,
) -> Result<()> {
    conn.execute(
        "INSERT INTO edge_versions (snapshot_id, source_stable_id, target_stable_id, kind)
         VALUES (?1, ?2, ?3, ?4)",
        params![
            snapshot_id,
            edge.source_stable_id,
            edge.target_stable_id,
            edge.kind
        ],
    )?;
    Ok(())
}

pub fn ingest_snapshot_sources(
    graph: &CodeGraph,
    snapshot_id: i64,
    repo_root: &Path,
    files: &[SnapshotFileInput],
) -> Result<SnapshotIngestStats> {
    let conn = graph.side_connection().lock();
    conn.execute(
        "DELETE FROM edge_versions WHERE snapshot_id = ?1",
        params![snapshot_id],
    )?;
    conn.execute(
        "DELETE FROM symbol_versions WHERE snapshot_id = ?1",
        params![snapshot_id],
    )?;
    conn.execute(
        "DELETE FROM file_versions WHERE snapshot_id = ?1",
        params![snapshot_id],
    )?;

    let parent_snapshot = load_parent_snapshot_data(&conn, snapshot_id)?;
    let mut stats = SnapshotIngestStats {
        files_total: files.len(),
        ..SnapshotIngestStats::default()
    };

    let mut current_name_lookup: HashMap<(String, String), String> = HashMap::new();
    let mut global_name_lookup: HashMap<String, Vec<String>> = HashMap::new();
    let mut copied_edge_keys: HashSet<(String, String, String)> = HashSet::new();
    let mut changed_files = Vec::new();

    for file in files {
        let repo_relative_path = normalize_repo_relative_path(&file.path, Some(repo_root));
        let content_hash = graph.compute_content_hash(&file.source);
        insert_file_version(
            &conn,
            snapshot_id,
            &repo_relative_path,
            &content_hash,
            file.source.len() as i64,
        )?;

        let parent_hash = parent_snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.file_hashes.get(&repo_relative_path));
        if parent_hash == Some(&content_hash) {
            stats.files_skipped += 1;
            if let Some(parent_symbols) = parent_snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.symbols_by_file.get(&repo_relative_path))
            {
                for symbol in parent_symbols {
                    insert_symbol_version(&conn, snapshot_id, symbol)?;
                    stats.symbol_versions += 1;
                    current_name_lookup.insert(
                        (symbol.file_path.clone(), symbol.name.clone()),
                        symbol.stable_id.clone(),
                    );
                    global_name_lookup
                        .entry(symbol.name.clone())
                        .or_default()
                        .push(symbol.stable_id.clone());
                }
                if let Some(parent_edges) = parent_snapshot.as_ref().map(|snapshot| &snapshot.edges)
                {
                    let symbol_ids: HashSet<String> = parent_symbols
                        .iter()
                        .map(|symbol| symbol.stable_id.clone())
                        .collect();
                    for edge in parent_edges {
                        if !symbol_ids.contains(&edge.source_stable_id)
                            && !symbol_ids.contains(&edge.target_stable_id)
                        {
                            continue;
                        }
                        let edge_key = (
                            edge.source_stable_id.clone(),
                            edge.target_stable_id.clone(),
                            edge.kind.clone(),
                        );
                        if copied_edge_keys.insert(edge_key) {
                            insert_edge_version(&conn, snapshot_id, edge)?;
                            stats.edge_versions += 1;
                        }
                    }
                }
            }
            continue;
        }

        let (language, tree, symbol_facts) = extract_symbols_for_source(&file.path, &file.source)?;
        let stored_symbols: Vec<StoredSymbolVersion> = symbol_facts
            .iter()
            .map(|fact| StoredSymbolVersion {
                stable_id: stable_symbol_id_for_fact(fact, &file.source, Some(repo_root)),
                name: fact
                    .name
                    .clone()
                    .unwrap_or_else(|| format!("<{}:{}>", fact.kind_normalized, fact.byte_start)),
                kind: fact.kind_normalized.clone(),
                file_path: repo_relative_path.clone(),
                start_line: fact.start_line as i64,
                start_col: fact.start_col as i64,
                end_line: fact.end_line as i64,
                end_col: fact.end_col as i64,
                body_hash: hash_symbol_body(&file.source, fact.byte_start, fact.byte_end),
            })
            .collect();

        for symbol in &stored_symbols {
            insert_symbol_version(&conn, snapshot_id, symbol)?;
            stats.symbol_versions += 1;
            current_name_lookup.insert(
                (symbol.file_path.clone(), symbol.name.clone()),
                symbol.stable_id.clone(),
            );
            global_name_lookup
                .entry(symbol.name.clone())
                .or_default()
                .push(symbol.stable_id.clone());
        }

        stats.files_indexed += 1;
        changed_files.push(PreparedSnapshotFile {
            repo_relative_path,
            source: file.source.clone(),
            tree,
            language,
            symbol_facts,
            stored_symbols,
        });
    }

    for prepared in changed_files {
        let Some(language) = prepared.language else {
            continue;
        };
        let Some(tree) = prepared.tree.as_ref() else {
            continue;
        };
        let path_buf = repo_root.join(&prepared.repo_relative_path);
        let calls = extract_calls_for_source(
            language,
            tree,
            &path_buf,
            &prepared.source,
            &prepared.symbol_facts,
        );

        for call in calls {
            let caller_symbol = prepared
                .symbol_facts
                .iter()
                .zip(prepared.stored_symbols.iter())
                .find(|(fact, _)| {
                    fact.name.as_deref() == Some(call.caller.as_str())
                        && fact.byte_start <= call.byte_start
                        && fact.byte_end >= call.byte_end
                })
                .map(|(_, stored)| stored.stable_id.clone())
                .or_else(|| {
                    current_name_lookup
                        .get(&(prepared.repo_relative_path.clone(), call.caller.clone()))
                        .cloned()
                });

            let callee_symbol = current_name_lookup
                .get(&(prepared.repo_relative_path.clone(), call.callee.clone()))
                .cloned()
                .or_else(|| {
                    global_name_lookup.get(&call.callee).and_then(|matches| {
                        if matches.len() == 1 {
                            matches.first().cloned()
                        } else {
                            None
                        }
                    })
                });

            let (Some(source_stable_id), Some(target_stable_id)) = (caller_symbol, callee_symbol)
            else {
                continue;
            };

            let edge = StoredEdgeVersion {
                source_stable_id,
                target_stable_id,
                kind: "CALLS".to_string(),
            };
            let edge_key = (
                edge.source_stable_id.clone(),
                edge.target_stable_id.clone(),
                edge.kind.clone(),
            );
            if copied_edge_keys.insert(edge_key) {
                insert_edge_version(&conn, snapshot_id, &edge)?;
                stats.edge_versions += 1;
            }
        }
    }

    Ok(stats)
}
