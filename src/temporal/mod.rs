mod ingest;
pub mod persistence;
pub mod query;
pub mod scc;
mod snapshots;
pub mod worktrees;

pub use ingest::{ingest_snapshot_sources, SnapshotFileInput, SnapshotIngestStats};
pub use snapshots::{register_snapshot, SnapshotSpec};
