use super::*;
use parking_lot::{Mutex, MutexGuard};
use rusqlite::{params, Connection, OptionalExtension};
use std::sync::Arc;

use crate::graph::side_tables_sqlite_schema::{
    ast_node_from_row, code_chunk_from_row, code_content_search_result_from_row,
    cross_file_ref_from_row, ensure_schema, execution_record_from_row, file_metrics_from_row,
    symbol_metrics_from_row,
};

/// SQLite-based side tables implementation
pub struct SqliteSideTables {
    conn: Arc<Mutex<Connection>>,
}

impl SqliteSideTables {
    /// Lock the shared connection.
    fn lock_conn(&self) -> MutexGuard<'_, Connection> {
        self.conn.lock()
    }

    /// Open or create side tables in SQLite database
    pub fn open(db_path: &Path) -> Result<Self> {
        let conn = Connection::open(db_path)?;
        Self::with_shared(Arc::new(Mutex::new(conn)))
    }

    /// Create from an existing connection reference-counted Mutex.
    ///
    /// This allows `CodeGraph` to share its `side_conn` with `SqliteSideTables`,
    /// eliminating redundant connection opens.
    pub fn with_shared(conn: Arc<Mutex<Connection>>) -> Result<Self> {
        let tables = Self { conn };
        tables.ensure_schema()?;
        Ok(tables)
    }

    fn ensure_schema(&self) -> Result<()> {
        let conn = self.lock_conn();
        ensure_schema(&conn)
    }
}

impl SideTables for SqliteSideTables {
    fn start_execution(
        &self,
        execution_id: &str,
        tool_version: &str,
        args: &[String],
        root: Option<&str>,
        db_path: &str,
    ) -> Result<i64> {
        let conn = self.lock_conn();
        let args_json = serde_json::to_string(args)?;
        let started_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        conn.execute(
            "INSERT INTO execution_log
                    (execution_id, tool_version, args, root, db_path, started_at, outcome)
                    VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'running')",
            params![
                execution_id,
                tool_version,
                args_json,
                root,
                db_path,
                started_at
            ],
        )?;

        Ok(conn.last_insert_rowid())
    }

    fn finish_execution(
        &self,
        execution_id: &str,
        outcome: &str,
        error_message: Option<&str>,
        files_indexed: usize,
        symbols_indexed: usize,
        references_indexed: usize,
    ) -> Result<()> {
        let conn = self.lock_conn();
        let finished_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        // Get started_at to compute duration
        let started_at: i64 = conn
            .query_row(
                "SELECT started_at FROM execution_log WHERE execution_id = ?1",
                params![execution_id],
                |row| row.get(0),
            )
            .optional()?
            .unwrap_or(finished_at);

        let duration_ms = (finished_at - started_at) * 1000;

        conn.execute(
            "UPDATE execution_log
                    SET finished_at = ?1, outcome = ?2, error_message = ?3,
                        duration_ms = ?4, files_indexed = ?5, symbols_indexed = ?6,
                        references_indexed = ?7
                    WHERE execution_id = ?8",
            params![
                finished_at,
                outcome,
                error_message,
                duration_ms,
                files_indexed as i64,
                symbols_indexed as i64,
                references_indexed as i64,
                execution_id,
            ],
        )?;

        Ok(())
    }

    fn get_execution(&self, execution_id: &str) -> Result<Option<ExecutionRecord>> {
        let conn = self.lock_conn();

        let result = conn
            .query_row(
                "SELECT id, execution_id, tool_version, args, root, db_path,
                            started_at, finished_at, duration_ms, outcome, error_message,
                            files_indexed, symbols_indexed, references_indexed
                     FROM execution_log
                     WHERE execution_id = ?1",
                params![execution_id],
                execution_record_from_row,
            )
            .optional()?;

        Ok(result)
    }

    fn list_executions(&self, limit: Option<usize>) -> Result<Vec<ExecutionRecord>> {
        let conn = self.lock_conn();

        let limit_clause = limit.map(|l| format!(" LIMIT {}", l)).unwrap_or_default();
        let sql = format!(
            "SELECT id, execution_id, tool_version, args, root, db_path,
                        started_at, finished_at, duration_ms, outcome, error_message,
                        files_indexed, symbols_indexed, references_indexed
                 FROM execution_log
                 ORDER BY started_at DESC{}",
            limit_clause
        );

        let mut stmt = conn.prepare(&sql)?;
        let records = stmt
            .query_map([], execution_record_from_row)?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(records)
    }

    fn store_file_metrics(&self, metrics: &FileMetrics) -> Result<()> {
        let conn = self.lock_conn();
        conn.execute(
            "INSERT OR REPLACE INTO file_metrics (
                    file_path, symbol_count, loc, estimated_loc,
                    fan_in, fan_out, complexity_score, last_updated
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                &metrics.file_path,
                metrics.symbol_count,
                metrics.loc,
                metrics.estimated_loc,
                metrics.fan_in,
                metrics.fan_out,
                metrics.complexity_score,
                metrics.last_updated,
            ],
        )?;
        Ok(())
    }

    fn get_file_metrics(&self, file_path: &str) -> Result<Option<FileMetrics>> {
        let conn = self.lock_conn();
        let result = conn
            .query_row(
                "SELECT file_path, symbol_count, loc, estimated_loc,
                            fan_in, fan_out, complexity_score, last_updated
                     FROM file_metrics
                     WHERE file_path = ?1",
                params![file_path],
                file_metrics_from_row,
            )
            .optional()?;

        Ok(result)
    }

    fn store_symbol_metrics(&self, metrics: &SymbolMetrics) -> Result<()> {
        let conn = self.lock_conn();
        conn.execute(
            "INSERT OR REPLACE INTO symbol_metrics (
                    symbol_id, symbol_name, kind, file_path,
                    loc, estimated_loc, fan_in, fan_out,
                    cyclomatic_complexity, last_updated
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                metrics.symbol_id,
                &metrics.symbol_name,
                &metrics.kind,
                &metrics.file_path,
                metrics.loc,
                metrics.estimated_loc,
                metrics.fan_in,
                metrics.fan_out,
                metrics.cyclomatic_complexity,
                metrics.last_updated,
            ],
        )?;
        Ok(())
    }

    fn get_symbol_metrics(&self, symbol_id: i64) -> Result<Option<SymbolMetrics>> {
        let conn = self.lock_conn();
        let result = conn
            .query_row(
                "SELECT symbol_id, symbol_name, kind, file_path,
                            loc, estimated_loc, fan_in, fan_out,
                            cyclomatic_complexity, last_updated
                     FROM symbol_metrics
                     WHERE symbol_id = ?1",
                params![symbol_id],
                symbol_metrics_from_row,
            )
            .optional()?;

        Ok(result)
    }

    fn delete_metrics_for_file(&self, file_path: &str) -> Result<usize> {
        let conn = self.lock_conn();

        // Delete symbol metrics for this file first
        let symbol_count = conn.execute(
            "DELETE FROM symbol_metrics WHERE file_path = ?1",
            params![file_path],
        )?;

        // Delete file metrics
        conn.execute(
            "DELETE FROM file_metrics WHERE file_path = ?1",
            params![file_path],
        )?;

        Ok(symbol_count)
    }

    fn get_hotspots(
        &self,
        limit: Option<u32>,
        min_loc: Option<i64>,
        min_fan_in: Option<i64>,
        min_fan_out: Option<i64>,
    ) -> Result<Vec<FileMetrics>> {
        let conn = self.lock_conn();

        // Build query with optional filters
        let mut query = String::from(
            "SELECT file_path, symbol_count, loc, estimated_loc,
                        fan_in, fan_out, complexity_score, last_updated
                 FROM file_metrics
                 WHERE 1=1",
        );
        let mut param_count = 0;

        if min_loc.is_some() {
            param_count += 1;
            query.push_str(&format!(" AND loc >= ?{param_count}"));
        }
        if min_fan_in.is_some() {
            param_count += 1;
            query.push_str(&format!(" AND fan_in >= ?{param_count}"));
        }
        if min_fan_out.is_some() {
            param_count += 1;
            query.push_str(&format!(" AND fan_out >= ?{param_count}"));
        }

        param_count += 1;
        query.push_str(&format!(
            " ORDER BY complexity_score DESC LIMIT ?{param_count}"
        ));

        let mut stmt = conn.prepare(&query)?;

        // Build params based on which filters are active
        let mut query_params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

        if let Some(min_loc) = min_loc {
            query_params.push(Box::new(min_loc));
        }
        if let Some(min_fi) = min_fan_in {
            query_params.push(Box::new(min_fi));
        }
        if let Some(min_fo) = min_fan_out {
            query_params.push(Box::new(min_fo));
        }
        query_params.push(Box::new(limit.unwrap_or(20) as i64));

        // Convert to references for query
        let param_refs: Vec<&dyn rusqlite::types::ToSql> =
            query_params.iter().map(|p| p.as_ref()).collect();

        let mut rows = stmt.query(&*param_refs)?;

        let mut results = Vec::new();
        while let Some(row) = rows.next()? {
            results.push(file_metrics_from_row(row)?);
        }

        Ok(results)
    }

    // ===== Code Chunk Methods =====

    fn store_chunk(&self, chunk: &CodeChunk) -> Result<i64> {
        let conn = self.lock_conn();
        let insert_chunk = |conn: &Connection| {
            conn.execute(
                    "INSERT OR REPLACE INTO code_chunks
                        (file_path, byte_start, byte_end, content, content_hash, symbol_name, symbol_kind, created_at)
                        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    params![
                        chunk.file_path,
                        chunk.byte_start as i64,
                        chunk.byte_end as i64,
                        chunk.content,
                        chunk.content_hash,
                        chunk.symbol_name,
                        chunk.symbol_kind,
                        chunk.created_at,
                    ],
                )
        };
        match insert_chunk(&conn) {
            Ok(_) => {}
            Err(err) if crate::generation::is_code_chunks_fts_corruption(&err) => {
                crate::generation::rebuild_code_chunks_fts(&conn)?;
                insert_chunk(&conn)?;
            }
            Err(err) => return Err(err.into()),
        }
        Ok(conn.last_insert_rowid())
    }

    fn get_chunk(&self, chunk_id: i64) -> Result<Option<CodeChunk>> {
        let conn = self.lock_conn();
        let result = conn
            .query_row(
                "SELECT id, file_path, byte_start, byte_end, content, content_hash,
                            symbol_name, symbol_kind, created_at
                     FROM code_chunks WHERE id = ?1",
                params![chunk_id],
                code_chunk_from_row,
            )
            .optional()?;
        Ok(result)
    }

    fn get_chunk_by_span(
        &self,
        file_path: &str,
        byte_start: usize,
        byte_end: usize,
    ) -> Result<Option<CodeChunk>> {
        let conn = self.lock_conn();
        let result = conn
            .query_row(
                "SELECT id, file_path, byte_start, byte_end, content, content_hash,
                            symbol_name, symbol_kind, created_at
                     FROM code_chunks WHERE file_path = ?1 AND byte_start = ?2 AND byte_end = ?3",
                params![file_path, byte_start as i64, byte_end as i64],
                code_chunk_from_row,
            )
            .optional()?;
        Ok(result)
    }

    fn get_chunks_for_file(&self, file_path: &str) -> Result<Vec<CodeChunk>> {
        let conn = self.lock_conn();
        let mut stmt = conn.prepare(
            "SELECT id, file_path, byte_start, byte_end, content, content_hash,
                        symbol_name, symbol_kind, created_at
                 FROM code_chunks WHERE file_path = ?1 ORDER BY byte_start",
        )?;
        let chunks = stmt
            .query_map(params![file_path], code_chunk_from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(chunks)
    }

    fn count_chunks_for_file(&self, file_path: &str) -> Result<usize> {
        let conn = self.lock_conn();
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM code_chunks WHERE file_path = ?1",
            params![file_path],
            |row| row.get(0),
        )?;
        Ok(count as usize)
    }

    fn delete_chunks_for_file(&self, file_path: &str) -> Result<usize> {
        let conn = self.lock_conn();
        let delete_chunks = |conn: &Connection| {
            conn.execute(
                "DELETE FROM code_chunks WHERE file_path = ?1",
                params![file_path],
            )
        };
        let affected = match delete_chunks(&conn) {
            Ok(affected) => affected,
            Err(err) if crate::generation::is_code_chunks_fts_corruption(&err) => {
                crate::generation::rebuild_code_chunks_fts(&conn)?;
                delete_chunks(&conn)?
            }
            Err(err) => return Err(err.into()),
        };
        Ok(affected)
    }

    fn get_chunks_by_symbol(&self, file_path: &str, symbol_name: &str) -> Result<Vec<CodeChunk>> {
        let conn = self.lock_conn();
        let mut stmt = conn.prepare(
            "SELECT id, file_path, byte_start, byte_end, content, content_hash,
                        symbol_name, symbol_kind, created_at
                 FROM code_chunks 
                 WHERE file_path = ?1 AND symbol_name = ?2
                 ORDER BY byte_start",
        )?;
        let chunks = stmt
            .query_map(params![file_path, symbol_name], code_chunk_from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(chunks)
    }

    fn get_all_chunks(&self) -> Result<Vec<CodeChunk>> {
        let conn = self.lock_conn();
        let mut stmt = conn.prepare(
            "SELECT id, file_path, byte_start, byte_end, content, content_hash,
                        symbol_name, symbol_kind, created_at
                 FROM code_chunks ORDER BY file_path, byte_start",
        )?;
        let chunks = stmt
            .query_map([], code_chunk_from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(chunks)
    }

    fn count_chunks(&self) -> Result<usize> {
        let conn = self.lock_conn();
        let count: i64 =
            conn.query_row("SELECT COUNT(*) FROM code_chunks", [], |row| row.get(0))?;
        Ok(count as usize)
    }

    fn search_code_content(
        &self,
        pattern: &str,
        limit: usize,
    ) -> Result<Vec<CodeContentSearchResult>> {
        let conn = self.lock_conn();
        let mut stmt = conn.prepare(
            "SELECT cc.symbol_name, cc.symbol_kind, cc.file_path,
                        cc.byte_start, cc.byte_end, cc.content, fts.rank
                 FROM code_chunks_fts fts
                 JOIN code_chunks cc ON fts.rowid = cc.id
                 WHERE code_chunks_fts MATCH ?1
                 ORDER BY fts.rank
                 LIMIT ?2",
        )?;
        let results = stmt
            .query_map(params![pattern, limit as i64], |row| {
                code_content_search_result_from_row(row, pattern)
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(results)
    }

    // ===== AST Node Methods =====

    fn store_ast_node(&self, node: &crate::graph::AstNode, file_id: i64) -> Result<i64> {
        let conn = self.lock_conn();
        conn.execute(
            "INSERT INTO ast_nodes (parent_id, kind, byte_start, byte_end, file_id)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                node.parent_id,
                node.kind,
                node.byte_start as i64,
                node.byte_end as i64,
                file_id,
            ],
        )?;
        Ok(conn.last_insert_rowid())
    }

    fn store_ast_nodes_batch(&self, nodes: &[(crate::graph::AstNode, i64)]) -> Result<Vec<i64>> {
        if nodes.is_empty() {
            return Ok(Vec::new());
        }

        let mut conn = self.lock_conn();

        // Use transaction for better performance
        let tx = conn.transaction()?;

        let mut ids = Vec::with_capacity(nodes.len());
        for (node, file_id) in nodes {
            tx.execute(
                "INSERT INTO ast_nodes (parent_id, kind, byte_start, byte_end, file_id)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    node.parent_id,
                    node.kind,
                    node.byte_start as i64,
                    node.byte_end as i64,
                    file_id,
                ],
            )?;
            ids.push(tx.last_insert_rowid());
        }

        tx.commit()?;
        Ok(ids)
    }

    fn get_ast_node(&self, node_id: i64) -> Result<Option<crate::graph::AstNode>> {
        let conn = self.lock_conn();
        let result = conn
            .query_row(
                "SELECT id, parent_id, kind, byte_start, byte_end
                     FROM ast_nodes WHERE id = ?1",
                params![node_id],
                ast_node_from_row,
            )
            .optional()?;
        Ok(result)
    }

    fn update_ast_node_parent(&self, node_id: i64, new_parent_id: i64) -> Result<()> {
        let conn = self.lock_conn();
        conn.execute(
            "UPDATE ast_nodes SET parent_id = ?1 WHERE id = ?2",
            params![new_parent_id, node_id],
        )?;
        Ok(())
    }

    fn get_ast_nodes_by_file(&self, file_id: i64) -> Result<Vec<crate::graph::AstNode>> {
        let conn = self.lock_conn();
        let mut stmt = conn.prepare(
            "SELECT id, parent_id, kind, byte_start, byte_end
                 FROM ast_nodes WHERE file_id = ?1 ORDER BY byte_start",
        )?;
        let nodes = stmt
            .query_map(params![file_id], ast_node_from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(nodes)
    }

    fn get_all_ast_nodes(&self) -> Result<Vec<crate::graph::AstNode>> {
        let conn = self.lock_conn();
        let mut stmt = conn.prepare(
            "SELECT id, parent_id, kind, byte_start, byte_end
                 FROM ast_nodes ORDER BY byte_start",
        )?;
        let nodes = stmt
            .query_map([], ast_node_from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(nodes)
    }

    fn get_ast_nodes_by_kind(&self, kind: &str) -> Result<Vec<crate::graph::AstNode>> {
        let conn = self.lock_conn();
        let mut stmt = conn.prepare(
            "SELECT id, parent_id, kind, byte_start, byte_end
                 FROM ast_nodes WHERE kind = ?1 ORDER BY byte_start",
        )?;
        let nodes = stmt
            .query_map(params![kind], ast_node_from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(nodes)
    }

    fn get_ast_children(&self, parent_id: i64) -> Result<Vec<crate::graph::AstNode>> {
        let conn = self.lock_conn();
        let mut stmt = conn.prepare(
            "SELECT id, parent_id, kind, byte_start, byte_end
                 FROM ast_nodes WHERE parent_id = ?1 ORDER BY byte_start",
        )?;
        let nodes = stmt
            .query_map(params![parent_id], ast_node_from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(nodes)
    }

    fn count_ast_nodes(&self) -> Result<usize> {
        let conn = self.lock_conn();
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM ast_nodes", [], |row| row.get(0))?;
        Ok(count as usize)
    }

    fn count_ast_nodes_for_file(&self, file_id: i64) -> Result<usize> {
        let conn = self.lock_conn();
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM ast_nodes WHERE file_id = ?1",
            params![file_id],
            |row| row.get(0),
        )?;
        Ok(count as usize)
    }

    fn delete_ast_nodes_for_file(&self, file_id: i64) -> Result<usize> {
        let conn = self.lock_conn();
        let affected =
            conn.execute("DELETE FROM ast_nodes WHERE file_id = ?1", params![file_id])?;
        Ok(affected)
    }

    // ===== Cross-File Reference Methods =====

    fn store_cross_file_ref(&self, cref: &crate::graph::schema::CrossFileRef) -> Result<()> {
        let conn = self.lock_conn();
        conn.execute(
            "INSERT INTO cross_file_refs
                    (from_symbol_id, to_symbol_id, file_path, line_number, byte_start, byte_end)
                    VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                cref.from_symbol_id,
                cref.to_symbol_id,
                cref.file_path,
                cref.line_number as i64,
                cref.byte_start as i64,
                cref.byte_end as i64,
            ],
        )?;
        Ok(())
    }

    fn get_references_to(
        &self,
        to_symbol_id: &str,
    ) -> Result<Vec<crate::graph::schema::CrossFileRef>> {
        let conn = self.lock_conn();
        let mut stmt = conn.prepare(
            "SELECT from_symbol_id, to_symbol_id, file_path, line_number, byte_start, byte_end
                 FROM cross_file_refs WHERE to_symbol_id = ?1",
        )?;
        let rows = stmt.query_map(params![to_symbol_id], cross_file_ref_from_row)?;
        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    fn get_references_from(
        &self,
        from_symbol_id: &str,
    ) -> Result<Vec<crate::graph::schema::CrossFileRef>> {
        let conn = self.lock_conn();
        let mut stmt = conn.prepare(
            "SELECT from_symbol_id, to_symbol_id, file_path, line_number, byte_start, byte_end
                 FROM cross_file_refs WHERE from_symbol_id = ?1",
        )?;
        let rows = stmt.query_map(params![from_symbol_id], cross_file_ref_from_row)?;
        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    fn delete_cross_file_refs_for_file(&self, file_path: &str) -> Result<usize> {
        let conn = self.lock_conn();
        let affected = conn.execute(
            "DELETE FROM cross_file_refs WHERE file_path = ?1",
            params![file_path],
        )?;
        Ok(affected)
    }

    fn count_cross_file_refs(&self) -> Result<usize> {
        let conn = self.lock_conn();
        let count: i64 =
            conn.query_row("SELECT COUNT(*) FROM cross_file_refs", [], |row| row.get(0))?;
        Ok(count as usize)
    }

    // ===== Label Methods =====

    fn add_label(&self, entity_id: i64, label: &str) -> Result<()> {
        let conn = self.lock_conn();
        conn.execute(
            "INSERT OR IGNORE INTO graph_labels(entity_id, label) VALUES(?1, ?2)",
            params![entity_id, label],
        )?;
        Ok(())
    }

    fn get_labels_for_entity(&self, entity_id: i64) -> Result<Vec<String>> {
        let conn = self.lock_conn();
        let mut stmt =
            conn.prepare("SELECT label FROM graph_labels WHERE entity_id = ?1 ORDER BY label")?;
        let labels = stmt
            .query_map(params![entity_id], |row| row.get(0))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(labels)
    }

    fn get_entities_by_label(&self, label: &str) -> Result<Vec<i64>> {
        let conn = self.lock_conn();
        let mut stmt =
            conn.prepare("SELECT entity_id FROM graph_labels WHERE label = ?1 ORDER BY entity_id")?;
        let entities = stmt
            .query_map(params![label], |row| row.get(0))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(entities)
    }

    fn get_all_labels(&self) -> Result<Vec<String>> {
        let conn = self.lock_conn();
        let mut stmt = conn.prepare("SELECT DISTINCT label FROM graph_labels ORDER BY label")?;
        let labels = stmt
            .query_map([], |row| row.get(0))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(labels)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
