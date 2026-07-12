use anyhow::{Context, Result};

use super::MetaDb;

/// Symbol-level hotspot candidate from metrics tables.
#[derive(Debug, Clone, PartialEq)]
pub struct HotspotCandidate {
    pub project: String,
    pub symbol: String,
    pub file: String,
    pub rank_score: f64,
    pub loc: i64,
    pub fan_in: i64,
    pub cyclomatic_complexity: i64,
}

impl MetaDb {
    /// Analyze hotspot candidates across enabled project shards.
    ///
    /// For each enabled project, opens its shard DB and queries `symbol_metrics`.
    /// Ranks symbols by `fan_in * cyclomatic_complexity` DESC.
    pub fn analyze_hotspots(
        &self,
        project_filter: Option<&str>,
        limit: Option<usize>,
    ) -> Result<Vec<HotspotCandidate>> {
        let mut candidates = Vec::new();
        for project in self.list_projects()? {
            if !project.enabled {
                continue;
            }
            if let Some(filter) = project_filter {
                if project.name != filter {
                    continue;
                }
            }

            let shard = std::path::Path::new(&project.db_path);
            if !shard.exists() {
                continue;
            }

            let conn = rusqlite::Connection::open(shard)
                .with_context(|| format!("open shard {}", project.db_path))?;
            let mut stmt = conn.prepare(
                "SELECT symbol_name, file_path, loc, fan_in, cyclomatic_complexity
                 FROM symbol_metrics
                 ORDER BY (fan_in * cyclomatic_complexity) DESC",
            )?;
            let rows = stmt.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                ))
            })?;

            for row in rows {
                let (symbol, file, loc, fan_in, cyclomatic_complexity) = row?;
                let rank_score = (fan_in as f64) * (cyclomatic_complexity as f64);
                candidates.push(HotspotCandidate {
                    project: project.name.clone(),
                    symbol,
                    file,
                    rank_score,
                    loc,
                    fan_in,
                    cyclomatic_complexity,
                });
            }
        }

        candidates.sort_by(|a, b| {
            b.rank_score
                .partial_cmp(&a.rank_score)
                .expect("invariant: rank_score is non-negative finite product of positive integers")
        });
        if let Some(limit) = limit {
            candidates.truncate(limit);
        }
        Ok(candidates)
    }
}
