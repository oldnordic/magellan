use anyhow::Result;
use rusqlite::params;
use std::collections::{BTreeSet, HashMap};

#[derive(Debug, Clone)]
pub struct SnapshotScc {
    pub snapshot_id: i64,
    pub commit_oid: String,
    pub members: Vec<String>,
}

#[allow(
    clippy::too_many_arguments,
    reason = "Tarjan SCC helper carries recursive traversal state explicitly"
)]
fn strong_connect(
    node: &str,
    adjacency: &HashMap<String, Vec<String>>,
    index: &mut usize,
    stack: &mut Vec<String>,
    on_stack: &mut BTreeSet<String>,
    indices: &mut HashMap<String, usize>,
    lowlink: &mut HashMap<String, usize>,
    components: &mut Vec<Vec<String>>,
) {
    indices.insert(node.to_string(), *index);
    lowlink.insert(node.to_string(), *index);
    *index += 1;
    stack.push(node.to_string());
    on_stack.insert(node.to_string());

    if let Some(neighbors) = adjacency.get(node) {
        for neighbor in neighbors {
            if !indices.contains_key(neighbor) {
                strong_connect(
                    neighbor, adjacency, index, stack, on_stack, indices, lowlink, components,
                );
                if let (Some(node_lowlink), Some(neighbor_lowlink)) =
                    (lowlink.get(node).copied(), lowlink.get(neighbor).copied())
                {
                    lowlink.insert(node.to_string(), node_lowlink.min(neighbor_lowlink));
                }
            } else if on_stack.contains(neighbor) {
                let node_lowlink = lowlink.get(node).copied().unwrap_or(usize::MAX);
                let neighbor_index = indices.get(neighbor).copied().unwrap_or(usize::MAX);
                lowlink.insert(node.to_string(), node_lowlink.min(neighbor_index));
            }
        }
    }

    if indices.get(node) == lowlink.get(node) {
        let mut component = Vec::new();
        while let Some(member) = stack.pop() {
            on_stack.remove(&member);
            component.push(member.clone());
            if member == node {
                break;
            }
        }
        component.sort();
        components.push(component);
    }
}

pub fn compute_snapshot_sccs(
    conn: &rusqlite::Connection,
    snapshot_id: i64,
    commit_oid: &str,
) -> Result<Vec<SnapshotScc>> {
    let mut adjacency: HashMap<String, Vec<String>> = HashMap::new();

    let mut stmt = conn.prepare(
        "SELECT source_stable_id, target_stable_id
         FROM edge_versions
         WHERE snapshot_id = ?1 AND kind = 'CALLS'
         ORDER BY source_stable_id, target_stable_id",
    )?;
    let rows = stmt.query_map(params![snapshot_id], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;

    for row in rows {
        let (source, target) = row?;
        adjacency
            .entry(source.clone())
            .or_default()
            .push(target.clone());
        adjacency.entry(target).or_default();
    }

    let mut nodes: Vec<String> = adjacency.keys().cloned().collect();
    nodes.sort();

    let mut index = 0usize;
    let mut stack = Vec::new();
    let mut on_stack = BTreeSet::new();
    let mut indices = HashMap::new();
    let mut lowlink = HashMap::new();
    let mut components = Vec::new();

    for node in nodes {
        if !indices.contains_key(&node) {
            strong_connect(
                &node,
                &adjacency,
                &mut index,
                &mut stack,
                &mut on_stack,
                &mut indices,
                &mut lowlink,
                &mut components,
            );
        }
    }

    let mut sccs: Vec<SnapshotScc> = components
        .into_iter()
        .filter(|component| component.len() > 1)
        .map(|members| SnapshotScc {
            snapshot_id,
            commit_oid: commit_oid.to_string(),
            members,
        })
        .collect();
    sccs.sort_by(|a, b| a.members.cmp(&b.members));
    Ok(sccs)
}
