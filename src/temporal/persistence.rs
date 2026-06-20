use crate::temporal::scc::SnapshotScc;
use std::collections::BTreeMap;

#[derive(Debug, Clone, serde::Serialize)]
pub struct TemporalSccPoint {
    pub snapshot_id: i64,
    pub commit_oid: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct TemporalSccBarcode {
    pub lineage_key: String,
    pub member_count: usize,
    pub members: Vec<String>,
    pub snapshot_count: usize,
    pub born_commit_oid: Option<String>,
    pub died_commit_oid: Option<String>,
    pub lifetime_length: usize,
    pub churn_count: usize,
    pub snapshots: Vec<TemporalSccPoint>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct TemporalSccBarcodeReport {
    pub count: usize,
    pub sccs: Vec<TemporalSccBarcode>,
}

#[derive(Debug, Default)]
struct LineageAccumulator {
    members: Vec<String>,
    snapshots: Vec<TemporalSccPoint>,
    segment_count: usize,
    last_snapshot_id: Option<i64>,
}

pub fn lineage_key(members: &[String]) -> String {
    members.join("|")
}

pub fn build_scc_lineages(snapshot_sccs: &[SnapshotScc]) -> TemporalSccBarcodeReport {
    let mut accumulators: BTreeMap<String, LineageAccumulator> = BTreeMap::new();

    for scc in snapshot_sccs {
        let key = lineage_key(&scc.members);
        let entry = accumulators
            .entry(key)
            .or_insert_with(|| LineageAccumulator {
                members: scc.members.clone(),
                ..LineageAccumulator::default()
            });

        let starts_new_segment = entry
            .last_snapshot_id
            .map(|last_snapshot_id| last_snapshot_id + 1 != scc.snapshot_id)
            .unwrap_or(true);
        if starts_new_segment {
            entry.segment_count += 1;
        }

        entry.snapshots.push(TemporalSccPoint {
            snapshot_id: scc.snapshot_id,
            commit_oid: scc.commit_oid.clone(),
        });
        entry.last_snapshot_id = Some(scc.snapshot_id);
    }

    let mut sccs: Vec<TemporalSccBarcode> = accumulators
        .into_iter()
        .map(|(key, entry)| TemporalSccBarcode {
            lineage_key: key,
            member_count: entry.members.len(),
            members: entry.members,
            snapshot_count: entry.snapshots.len(),
            born_commit_oid: entry
                .snapshots
                .first()
                .map(|point| point.commit_oid.clone()),
            died_commit_oid: entry.snapshots.last().map(|point| point.commit_oid.clone()),
            lifetime_length: entry.snapshots.len(),
            churn_count: entry.segment_count.saturating_sub(1),
            snapshots: entry.snapshots,
        })
        .collect();
    sccs.sort_by(|a, b| a.lineage_key.cmp(&b.lineage_key));

    TemporalSccBarcodeReport {
        count: sccs.len(),
        sccs,
    }
}
