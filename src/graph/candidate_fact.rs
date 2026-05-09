//! Candidate fact staging for graph memory — Phase 2
//!
//! Facts extracted from source documents are staged as CandidateFacts
//! before entering trusted graph memory. The validator checks each
//! candidate against the ontology (v0) and routes accepted facts to
//! the graph and rejected/ambiguous facts to a review queue.

use crate::graph::ontology::{EntityType, OntologyV0, RelationType};
use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::time::{SystemTime, UNIX_EPOCH};

/// Status of a candidate fact in the staging pipeline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CandidateStatus {
    /// Just submitted, awaiting validation.
    Pending,
    /// Passed all validation checks, ready for graph insertion.
    Accepted,
    /// Failed one or more validation checks, in review queue.
    Rejected,
    /// Ambiguous — needs human or higher-confidence review.
    Ambiguous,
    /// Part of a conflict set with contradictory candidates.
    InConflict,
}

impl CandidateStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            CandidateStatus::Pending => "pending",
            CandidateStatus::Accepted => "accepted",
            CandidateStatus::Rejected => "rejected",
            CandidateStatus::Ambiguous => "ambiguous",
            CandidateStatus::InConflict => "in_conflict",
        }
    }

    pub fn parse(s: &str) -> Option<CandidateStatus> {
        match s {
            "pending" => Some(CandidateStatus::Pending),
            "accepted" => Some(CandidateStatus::Accepted),
            "rejected" => Some(CandidateStatus::Rejected),
            "ambiguous" => Some(CandidateStatus::Ambiguous),
            "in_conflict" => Some(CandidateStatus::InConflict),
            _ => None,
        }
    }
}

/// Properties attached to a candidate fact (provenance, confidence, etc.)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CandidateProperties {
    /// When the source fact was observed
    pub observed_at: i64,
    /// Source of the fact (file path, event id, etc.)
    pub source: String,
    /// Confidence score 0.0-1.0
    pub confidence: f64,
    /// How the fact was extracted
    pub extraction_method: String,
    /// Agent/tool that produced the candidate
    pub extractor: String,
    /// Optional: mechanism for causal relations
    pub mechanism: Option<String>,
    /// Optional: evidence span in source document
    pub evidence_span: Option<String>,
    /// Optional: severity level
    pub severity: Option<String>,
}

impl Default for CandidateProperties {
    fn default() -> Self {
        Self {
            observed_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64,
            source: String::new(),
            confidence: 1.0,
            extraction_method: "unknown".to_string(),
            extractor: "unknown".to_string(),
            mechanism: None,
            evidence_span: None,
            severity: None,
        }
    }
}

/// A candidate fact awaiting validation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CandidateFact {
    pub id: i64,
    pub candidate_id: String,
    pub source_document_id: i64,
    pub subject_type: String,
    pub subject_key: String,
    pub predicate: String,
    pub object_type: Option<String>,
    pub object_key: Option<String>,
    pub properties: CandidateProperties,
    pub status: CandidateStatus,
    pub rejection_reason: Option<String>,
    pub created_at: i64,
    pub reviewed_at: Option<i64>,
}

impl CandidateFact {
    /// Create a new candidate fact with default id (0, auto-assigned by DB).
    pub fn new(
        candidate_id: String,
        source_document_id: i64,
        subject_type: String,
        subject_key: String,
        predicate: String,
        properties: CandidateProperties,
    ) -> Self {
        let created_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        Self {
            id: 0,
            candidate_id,
            source_document_id,
            subject_type,
            subject_key,
            predicate,
            object_type: None,
            object_key: None,
            properties,
            status: CandidateStatus::Pending,
            rejection_reason: None,
            created_at,
            reviewed_at: None,
        }
    }

    /// Set the object side of the relation.
    pub fn with_object(mut self, object_type: String, object_key: String) -> Self {
        self.object_type = Some(object_type);
        self.object_key = Some(object_key);
        self
    }
}

/// Result of validating a candidate fact.
#[derive(Debug, Clone, PartialEq)]
pub struct ValidationResult {
    pub accepted: bool,
    pub errors: Vec<ValidationError>,
    pub warnings: Vec<String>,
}

impl ValidationResult {
    pub fn accepted() -> Self {
        Self {
            accepted: true,
            errors: vec![],
            warnings: vec![],
        }
    }

    pub fn rejected(errors: Vec<ValidationError>) -> Self {
        Self {
            accepted: false,
            errors,
            warnings: vec![],
        }
    }

    pub fn with_warnings(mut self, warnings: Vec<String>) -> Self {
        self.warnings = warnings;
        self
    }
}

/// Specific validation failure.
#[derive(Debug, Clone, PartialEq)]
pub enum ValidationError {
    /// Entity type not in ontology
    UnknownEntityType { entity_type: String },
    /// Relation type not in ontology
    UnknownRelationType { relation_type: String },
    /// Relation not allowed between subject/object types
    InvalidRelationForTypes {
        subject_type: String,
        relation_type: String,
        object_type: Option<String>,
    },
    /// Required property missing
    MissingRequiredProperty { property: String },
    /// Confidence out of valid range
    InvalidConfidence { confidence: f64 },
    /// Causal relation missing mechanism
    MissingMechanism { relation_type: String },
    /// Source document does not exist
    SourceDocumentNotFound { source_document_id: i64 },
    /// Source document hash mismatch
    SourceHashMismatch {
        source_document_id: i64,
        expected_hash: String,
        actual_hash: String,
    },
    /// Duplicate of existing active fact
    DuplicateFact { existing_candidate_id: String },
    /// Conflicts with existing fact without explicit override
    ConflictWithoutOverride { existing_candidate_id: String },
    /// Predicate is not canonical
    NonCanonicalPredicate { predicate: String },
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ValidationError::UnknownEntityType { entity_type } => {
                write!(f, "Unknown entity type: {}", entity_type)
            }
            ValidationError::UnknownRelationType { relation_type } => {
                write!(f, "Unknown relation type: {}", relation_type)
            }
            ValidationError::InvalidRelationForTypes {
                subject_type,
                relation_type,
                object_type,
            } => {
                write!(
                    f,
                    "Relation '{}' not allowed from '{}' to '{}'",
                    relation_type,
                    subject_type,
                    object_type.as_deref().unwrap_or("(none)")
                )
            }
            ValidationError::MissingRequiredProperty { property } => {
                write!(f, "Missing required property: {}", property)
            }
            ValidationError::InvalidConfidence { confidence } => {
                write!(f, "Confidence {} out of range [0.0, 1.0]", confidence)
            }
            ValidationError::MissingMechanism { relation_type } => {
                write!(
                    f,
                    "Causal relation '{}' requires mechanism and evidence",
                    relation_type
                )
            }
            ValidationError::SourceDocumentNotFound { source_document_id } => {
                write!(f, "Source document {} not found", source_document_id)
            }
            ValidationError::SourceHashMismatch {
                source_document_id,
                expected_hash,
                actual_hash,
            } => {
                write!(
                    f,
                    "Source document {} hash mismatch: expected {}, got {}",
                    source_document_id, expected_hash, actual_hash
                )
            }
            ValidationError::DuplicateFact {
                existing_candidate_id,
            } => {
                write!(f, "Duplicate of existing fact: {}", existing_candidate_id)
            }
            ValidationError::ConflictWithoutOverride {
                existing_candidate_id,
            } => {
                write!(
                    f,
                    "Conflicts with {} without supersedes/invalidated_by",
                    existing_candidate_id
                )
            }
            ValidationError::NonCanonicalPredicate { predicate } => {
                write!(f, "Predicate '{}' is not canonical", predicate)
            }
        }
    }
}

// ============================================================================
// Database Schema
// ============================================================================

/// Ensure candidate_facts table and indexes exist.
pub fn ensure_schema(conn: &Connection) -> Result<()> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS candidate_facts (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            candidate_id TEXT NOT NULL UNIQUE,
            source_document_id INTEGER NOT NULL,
            subject_type TEXT NOT NULL,
            subject_key TEXT NOT NULL,
            predicate TEXT NOT NULL,
            object_type TEXT,
            object_key TEXT,
            properties_json TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'pending',
            rejection_reason TEXT,
            created_at INTEGER NOT NULL,
            reviewed_at INTEGER,
            FOREIGN KEY (source_document_id) REFERENCES source_documents(id)
        )",
        [],
    )
    .context("create candidate_facts table")?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_candidate_facts_status ON candidate_facts(status)",
        [],
    )
    .context("create status index")?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_candidate_facts_source ON candidate_facts(source_document_id)",
        [],
    )
    .context("create source index")?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_candidate_facts_predicate ON candidate_facts(predicate)",
        [],
    )
    .context("create predicate index")?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_candidate_facts_status_created ON candidate_facts(status, created_at)",
        [],
    )
    .context("create status+created index")?;

    Ok(())
}

// ============================================================================
// Validation
// ============================================================================

/// Validate a candidate fact against the ontology.
///
/// Checks:
/// 1. Entity types are allowed
/// 2. Relation type is allowed for subject/object types
/// 3. Required properties are present
/// 4. Confidence is in valid range
/// 5. Causal relations have mechanism
/// 6. Predicate is canonical
///
/// Note: Rules 5 (source hash), 8 (duplicates), 9 (conflicts), and 11 (simultaneous
/// contradictions) require DB state and are checked by validate_with_context().
pub fn validate_ontology(candidate: &CandidateFact) -> ValidationResult {
    let ontology = OntologyV0::new();
    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    // Rule 1: Entity type is allowed
    if !ontology.is_entity_type_allowed(&candidate.subject_type) {
        errors.push(ValidationError::UnknownEntityType {
            entity_type: candidate.subject_type.clone(),
        });
    }

    if let Some(ref obj_type) = candidate.object_type {
        if !ontology.is_entity_type_allowed(obj_type) {
            errors.push(ValidationError::UnknownEntityType {
                entity_type: obj_type.clone(),
            });
        }
    }

    // Rule 2: Relation type is allowed
    if !ontology.is_relation_type_allowed(&candidate.predicate) {
        errors.push(ValidationError::UnknownRelationType {
            relation_type: candidate.predicate.clone(),
        });
    } else {
        // Check subject/object type compatibility
        let obj_type_ref = candidate.object_type.as_deref();
        if !ontology.is_relation_valid_for_types(
            &candidate.subject_type,
            &candidate.predicate,
            obj_type_ref,
        ) {
            errors.push(ValidationError::InvalidRelationForTypes {
                subject_type: candidate.subject_type.clone(),
                relation_type: candidate.predicate.clone(),
                object_type: candidate.object_type.clone(),
            });
        }
    }

    // Rule 3: Required properties are present
    if candidate.properties.source.is_empty() {
        errors.push(ValidationError::MissingRequiredProperty {
            property: "source".to_string(),
        });
    }
    if candidate.properties.extraction_method.is_empty() {
        errors.push(ValidationError::MissingRequiredProperty {
            property: "extraction_method".to_string(),
        });
    }

    // Rule 4: Confidence in valid range
    let conf = candidate.properties.confidence;
    if !(0.0..=1.0).contains(&conf) || conf.is_nan() {
        errors.push(ValidationError::InvalidConfidence { confidence: conf });
    }

    // Rule 6: Confidence matches extraction method limits
    match candidate.properties.extraction_method.as_str() {
        "frontmatter" | "wikilink" | "event_envelope" if conf < 1.0 => {
            warnings.push(format!(
                "Deterministic method '{}' should have confidence 1.0, got {}",
                candidate.properties.extraction_method, conf
            ));
        }
        "regex" | "tree_sitter" if conf > 0.95 => {
            warnings.push(format!(
                "Parser method '{}' confidence {} may be too high",
                candidate.properties.extraction_method, conf
            ));
        }
        "llm_candidate" if conf > 0.8 => {
            warnings.push(format!(
                "LLM candidate confidence {} should typically be ≤ 0.8",
                conf
            ));
        }
        _ => {}
    }

    // Rule 7: Predicate is canonical
    // (already checked by is_relation_type_allowed above)

    // Rule 10: Causal relations include mechanism and evidence
    if RelationType::parse(&candidate.predicate)
        .map(|r| r.requires_mechanism())
        .unwrap_or(false)
        && candidate.properties.mechanism.is_none()
    {
        errors.push(ValidationError::MissingMechanism {
            relation_type: candidate.predicate.clone(),
        });
    }

    if errors.is_empty() {
        ValidationResult::accepted().with_warnings(warnings)
    } else {
        ValidationResult::rejected(errors).with_warnings(warnings)
    }
}

// ============================================================================
// Database Operations
// ============================================================================

/// Insert a new candidate fact. Returns the auto-generated id.
///
/// If a candidate with the same candidate_id already exists, returns
/// an error — callers should use update_status() for status changes.
pub fn insert(conn: &Connection, candidate: &CandidateFact) -> Result<i64> {
    let properties_json =
        serde_json::to_string(&candidate.properties).context("serialize candidate properties")?;

    conn.execute(
        "INSERT INTO candidate_facts (
            candidate_id, source_document_id, subject_type, subject_key,
            predicate, object_type, object_key, properties_json, status,
            rejection_reason, created_at, reviewed_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        params![
            candidate.candidate_id,
            candidate.source_document_id,
            candidate.subject_type,
            candidate.subject_key,
            candidate.predicate,
            candidate.object_type,
            candidate.object_key,
            properties_json,
            candidate.status.as_str(),
            candidate.rejection_reason,
            candidate.created_at,
            candidate.reviewed_at,
        ],
    )
    .context("insert candidate fact")?;

    Ok(conn.last_insert_rowid())
}

/// Update the status of a candidate fact.
pub fn update_status(
    conn: &Connection,
    candidate_id: &str,
    new_status: CandidateStatus,
    rejection_reason: Option<&str>,
) -> Result<usize> {
    let reviewed_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    let rows = conn
        .execute(
            "UPDATE candidate_facts SET status = ?1, rejection_reason = ?2, reviewed_at = ?3
             WHERE candidate_id = ?4",
            params![
                new_status.as_str(),
                rejection_reason,
                reviewed_at,
                candidate_id,
            ],
        )
        .context("update candidate status")?;

    Ok(rows)
}

/// Find a candidate fact by its stable candidate_id.
pub fn find_by_id(conn: &Connection, candidate_id: &str) -> Result<Option<CandidateFact>> {
    let row = conn
        .query_row(
            "SELECT id, candidate_id, source_document_id, subject_type, subject_key,
                    predicate, object_type, object_key, properties_json, status,
                    rejection_reason, created_at, reviewed_at
             FROM candidate_facts WHERE candidate_id = ?1",
            params![candidate_id],
            row_to_candidate,
        )
        .optional()
        .context("find candidate by id")?;

    Ok(row)
}

/// List candidate facts by status, ordered by created_at descending.
pub fn list_by_status(
    conn: &Connection,
    status: Option<CandidateStatus>,
    limit: Option<usize>,
) -> Result<Vec<CandidateFact>> {
    let sql = match status {
        Some(_) => {
            "SELECT id, candidate_id, source_document_id, subject_type, subject_key,
                    predicate, object_type, object_key, properties_json, status,
                    rejection_reason, created_at, reviewed_at
             FROM candidate_facts WHERE status = ?1
             ORDER BY created_at DESC
             LIMIT ?2"
        }
        None => {
            "SELECT id, candidate_id, source_document_id, subject_type, subject_key,
                    predicate, object_type, object_key, properties_json, status,
                    rejection_reason, created_at, reviewed_at
             FROM candidate_facts
             ORDER BY created_at DESC
             LIMIT ?2"
        }
    };

    let limit_val = limit.unwrap_or(1000) as i64;

    let mut stmt = conn.prepare(sql).context("prepare list query")?;

    let rows = match status {
        Some(s) => stmt
            .query_map(params![s.as_str(), limit_val], row_to_candidate)
            .context("execute list query with status")?,
        None => stmt
            .query_map(params![limit_val], row_to_candidate)
            .context("execute list query")?,
    };

    let mut results = Vec::new();
    for row in rows {
        results.push(row.context("map row to candidate")?);
    }

    Ok(results)
}

/// Get the review queue: all rejected and ambiguous candidates.
pub fn review_queue(conn: &Connection, limit: Option<usize>) -> Result<Vec<CandidateFact>> {
    let limit_val = limit.unwrap_or(1000) as i64;
    let mut stmt = conn
        .prepare(
            "SELECT id, candidate_id, source_document_id, subject_type, subject_key,
                    predicate, object_type, object_key, properties_json, status,
                    rejection_reason, created_at, reviewed_at
             FROM candidate_facts
             WHERE status IN ('rejected', 'ambiguous', 'in_conflict')
             ORDER BY created_at DESC
             LIMIT ?1",
        )
        .context("prepare review queue query")?;

    let rows = stmt
        .query_map(params![limit_val], row_to_candidate)
        .context("execute review queue query")?;

    let mut results = Vec::new();
    for row in rows {
        results.push(row.context("map row to candidate")?);
    }

    Ok(results)
}

/// Check if an active (pending or accepted) candidate with the same
/// subject-predicate-object triple already exists.
pub fn find_duplicate(
    conn: &Connection,
    subject_type: &str,
    subject_key: &str,
    predicate: &str,
    object_type: Option<&str>,
    object_key: Option<&str>,
) -> Result<Option<String>> {
    let sql = match (object_type, object_key) {
        (Some(_), Some(_)) => {
            "SELECT candidate_id FROM candidate_facts
             WHERE subject_type = ?1 AND subject_key = ?2 AND predicate = ?3
               AND object_type = ?4 AND object_key = ?5
               AND status IN ('pending', 'accepted')
             LIMIT 1"
        }
        _ => {
            "SELECT candidate_id FROM candidate_facts
             WHERE subject_type = ?1 AND subject_key = ?2 AND predicate = ?3
               AND object_type IS NULL AND object_key IS NULL
               AND status IN ('pending', 'accepted')
             LIMIT 1"
        }
    };

    let result = match (object_type, object_key) {
        (Some(ot), Some(ok)) => conn
            .query_row(
                sql,
                params![subject_type, subject_key, predicate, ot, ok],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .context("find duplicate with object")?,
        _ => conn
            .query_row(sql, params![subject_type, subject_key, predicate], |row| {
                row.get::<_, String>(0)
            })
            .optional()
            .context("find duplicate without object")?,
    };

    Ok(result)
}

/// Row mapper for candidate facts.
fn row_to_candidate(row: &rusqlite::Row) -> rusqlite::Result<CandidateFact> {
    let properties_json: String = row.get(8)?;
    let properties: CandidateProperties =
        serde_json::from_str(&properties_json).unwrap_or_default();

    let status_str: String = row.get(9)?;
    let status = CandidateStatus::parse(&status_str).unwrap_or(CandidateStatus::Pending);

    Ok(CandidateFact {
        id: row.get(0)?,
        candidate_id: row.get(1)?,
        source_document_id: row.get(2)?,
        subject_type: row.get(3)?,
        subject_key: row.get(4)?,
        predicate: row.get(5)?,
        object_type: row.get(6)?,
        object_key: row.get(7)?,
        properties,
        status,
        rejection_reason: row.get(10)?,
        created_at: row.get(11)?,
        reviewed_at: row.get(12)?,
    })
}

// ============================================================================
// Conflict Detection
// ============================================================================

/// Detect conflicting candidates among a batch.
///
/// Two candidates conflict when they have the same subject and predicate
/// but different objects (for relations with objects) or different properties
/// that semantically contradict (e.g., same task with different assignees).
///
/// Returns groups of conflicting candidate_ids.
pub fn detect_conflicts(conn: &Connection, candidate_ids: &[&str]) -> Result<Vec<ConflictSet>> {
    if candidate_ids.len() < 2 {
        return Ok(vec![]);
    }

    let placeholders = candidate_ids
        .iter()
        .map(|_| "?")
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        "SELECT candidate_id, subject_type, subject_key, predicate, object_type, object_key
         FROM candidate_facts
         WHERE candidate_id IN ({})",
        placeholders
    );

    let mut stmt = conn.prepare(&sql).context("prepare conflict detection")?;

    // Build params dynamically
    let params: Vec<&dyn rusqlite::ToSql> = candidate_ids
        .iter()
        .map(|id| id as &dyn rusqlite::ToSql)
        .collect();

    type EntryVec = Vec<(String, Option<String>, Option<String>)>;

    let rows = stmt
        .query_map(params.as_slice(), |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Option<String>>(5)?,
            ))
        })
        .context("execute conflict detection")?;

    let mut by_key: std::collections::HashMap<String, EntryVec> = std::collections::HashMap::new();

    for row in rows {
        let (cid, stype, skey, pred, otype, okey) = row?;
        let key = format!("{}:{}:{}", stype, skey, pred);
        by_key.entry(key).or_default().push((cid, otype, okey));
    }

    let mut conflicts = Vec::new();
    for (_key, entries) in by_key {
        if entries.len() > 1 {
            // Check if they have different objects (conflict) or are duplicates
            let unique_objects: HashSet<String> = entries
                .iter()
                .map(|(_, ot, ok)| match (ot, ok) {
                    (Some(t), Some(k)) => format!("{}:{}", t, k),
                    _ => "none".to_string(),
                })
                .collect();

            if unique_objects.len() > 1 {
                let candidate_ids: Vec<String> =
                    entries.into_iter().map(|(cid, _, _)| cid).collect();
                conflicts.push(ConflictSet {
                    conflict_set_id: format!("conflict_{}", uuid::Uuid::new_v4()),
                    conflict_type: ConflictType::StatusConflict,
                    candidate_ids,
                    detected_at: SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs() as i64,
                    resolution_status: ResolutionStatus::Unresolved,
                    resolver: None,
                    resolution_reason: None,
                });
            }
        }
    }

    Ok(conflicts)
}

/// A set of conflicting candidates.
#[derive(Debug, Clone)]
pub struct ConflictSet {
    pub conflict_set_id: String,
    pub conflict_type: ConflictType,
    pub candidate_ids: Vec<String>,
    pub detected_at: i64,
    pub resolution_status: ResolutionStatus,
    pub resolver: Option<String>,
    pub resolution_reason: Option<String>,
}

/// Type of conflict detected.
#[derive(Debug, Clone, PartialEq)]
pub enum ConflictType {
    StatusConflict,
    OwnershipConflict,
    TimestampConflict,
    SemanticConflict,
    CausalConflict,
}

impl ConflictType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ConflictType::StatusConflict => "status_conflict",
            ConflictType::OwnershipConflict => "ownership_conflict",
            ConflictType::TimestampConflict => "timestamp_conflict",
            ConflictType::SemanticConflict => "semantic_conflict",
            ConflictType::CausalConflict => "causal_conflict",
        }
    }
}

/// Resolution status of a conflict set.
#[derive(Debug, Clone, PartialEq)]
pub enum ResolutionStatus {
    Unresolved,
    AcceptedOne,
    Merged,
    RejectedAll,
    NeedsUser,
}

impl ResolutionStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            ResolutionStatus::Unresolved => "unresolved",
            ResolutionStatus::AcceptedOne => "accepted_one",
            ResolutionStatus::Merged => "merged",
            ResolutionStatus::RejectedAll => "rejected_all",
            ResolutionStatus::NeedsUser => "needs_user",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn in_memory_db() -> Connection {
        Connection::open_in_memory().unwrap()
    }

    fn setup_schema(conn: &Connection) {
        crate::graph::source_inventory::ensure_schema(conn).unwrap();
        ensure_schema(conn).unwrap();
    }

    fn sample_doc(conn: &Connection) -> i64 {
        use crate::graph::source_inventory::SourceDocument;
        let doc = SourceDocument::new(
            "/wiki/test.md".to_string(),
            "wiki".to_string(),
            "abc123".to_string(),
        );
        crate::graph::source_inventory::insert_or_update(conn, &doc).unwrap();
        // Get the id
        conn.query_row(
            "SELECT id FROM source_documents WHERE path_or_uri = ?1",
            params!["/wiki/test.md"],
            |row| row.get::<_, i64>(0),
        )
        .unwrap()
    }

    fn sample_candidate(source_doc_id: i64) -> CandidateFact {
        let props = CandidateProperties {
            observed_at: 1234567890,
            source: "/wiki/test.md".to_string(),
            confidence: 1.0,
            extraction_method: "event_envelope".to_string(),
            extractor: "test".to_string(),
            mechanism: None,
            evidence_span: None,
            severity: None,
        };
        CandidateFact::new(
            "cf_test_001".to_string(),
            source_doc_id,
            "Task".to_string(),
            "graph-memory-impl".to_string(),
            "assigned_to".to_string(),
            props,
        )
        .with_object("Agent".to_string(), "Codex".to_string())
    }

    #[test]
    fn test_candidate_status_roundtrip() {
        for status in [
            CandidateStatus::Pending,
            CandidateStatus::Accepted,
            CandidateStatus::Rejected,
            CandidateStatus::Ambiguous,
            CandidateStatus::InConflict,
        ] {
            let s = status.as_str();
            let parsed = CandidateStatus::parse(s).unwrap();
            assert_eq!(parsed, status);
        }
    }

    #[test]
    fn test_ensure_schema_creates_table() {
        let conn = in_memory_db();
        ensure_schema(&conn).unwrap();

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='candidate_facts'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_insert_and_find() {
        let conn = in_memory_db();
        setup_schema(&conn);
        let doc_id = sample_doc(&conn);

        let candidate = sample_candidate(doc_id);
        let id = insert(&conn, &candidate).unwrap();
        assert!(id > 0);

        let found = find_by_id(&conn, "cf_test_001").unwrap();
        assert!(found.is_some());
        let found = found.unwrap();
        assert_eq!(found.candidate_id, "cf_test_001");
        assert_eq!(found.subject_type, "Task");
        assert_eq!(found.subject_key, "graph-memory-impl");
        assert_eq!(found.predicate, "assigned_to");
        assert_eq!(found.object_type, Some("Agent".to_string()));
        assert_eq!(found.object_key, Some("Codex".to_string()));
        assert_eq!(found.status, CandidateStatus::Pending);
    }

    #[test]
    fn test_update_status() {
        let conn = in_memory_db();
        setup_schema(&conn);
        let doc_id = sample_doc(&conn);

        let candidate = sample_candidate(doc_id);
        insert(&conn, &candidate).unwrap();

        let rows = update_status(&conn, "cf_test_001", CandidateStatus::Accepted, None).unwrap();
        assert_eq!(rows, 1);

        let found = find_by_id(&conn, "cf_test_001").unwrap().unwrap();
        assert_eq!(found.status, CandidateStatus::Accepted);
        assert!(found.reviewed_at.is_some());
    }

    #[test]
    fn test_list_by_status() {
        let conn = in_memory_db();
        setup_schema(&conn);
        let doc_id = sample_doc(&conn);

        let c1 = sample_candidate(doc_id);
        insert(&conn, &c1).unwrap();

        let mut c2 = sample_candidate(doc_id);
        c2.candidate_id = "cf_test_002".to_string();
        c2.subject_key = "other-task".to_string();
        insert(&conn, &c2).unwrap();
        update_status(&conn, "cf_test_002", CandidateStatus::Accepted, None).unwrap();

        let pending = list_by_status(&conn, Some(CandidateStatus::Pending), None).unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].candidate_id, "cf_test_001");

        let accepted = list_by_status(&conn, Some(CandidateStatus::Accepted), None).unwrap();
        assert_eq!(accepted.len(), 1);
        assert_eq!(accepted[0].candidate_id, "cf_test_002");
    }

    #[test]
    fn test_review_queue() {
        let conn = in_memory_db();
        setup_schema(&conn);
        let doc_id = sample_doc(&conn);

        let c1 = sample_candidate(doc_id);
        insert(&conn, &c1).unwrap();
        update_status(
            &conn,
            "cf_test_001",
            CandidateStatus::Rejected,
            Some("missing mechanism"),
        )
        .unwrap();

        let queue = review_queue(&conn, None).unwrap();
        assert_eq!(queue.len(), 1);
        assert_eq!(queue[0].candidate_id, "cf_test_001");
        assert_eq!(queue[0].status, CandidateStatus::Rejected);
    }

    #[test]
    fn test_validate_ontology_valid_fact() {
        let doc_id = 1i64;
        let props = CandidateProperties {
            observed_at: 1234567890,
            source: "/wiki/test.md".to_string(),
            confidence: 1.0,
            extraction_method: "event_envelope".to_string(),
            extractor: "test".to_string(),
            mechanism: None,
            evidence_span: None,
            severity: None,
        };
        let candidate = CandidateFact::new(
            "cf_001".to_string(),
            doc_id,
            "Task".to_string(),
            "task-1".to_string(),
            "assigned_to".to_string(),
            props,
        )
        .with_object("Agent".to_string(), "Codex".to_string());

        let result = validate_ontology(&candidate);
        assert!(
            result.accepted,
            "Valid fact should be accepted: {:?}",
            result.errors
        );
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_validate_ontology_unknown_entity_type() {
        let props = CandidateProperties {
            observed_at: 1234567890,
            source: "/wiki/test.md".to_string(),
            confidence: 1.0,
            extraction_method: "event_envelope".to_string(),
            extractor: "test".to_string(),
            mechanism: None,
            evidence_span: None,
            severity: None,
        };
        let candidate = CandidateFact::new(
            "cf_002".to_string(),
            1,
            "UnknownType".to_string(),
            "key".to_string(),
            "assigned_to".to_string(),
            props,
        );

        let result = validate_ontology(&candidate);
        assert!(!result.accepted);
        assert!(result
            .errors
            .iter()
            .any(|e| matches!(e, ValidationError::UnknownEntityType { .. })));
    }

    #[test]
    fn test_validate_ontology_unknown_relation_type() {
        let props = CandidateProperties {
            observed_at: 1234567890,
            source: "/wiki/test.md".to_string(),
            confidence: 1.0,
            extraction_method: "event_envelope".to_string(),
            extractor: "test".to_string(),
            mechanism: None,
            evidence_span: None,
            severity: None,
        };
        let candidate = CandidateFact::new(
            "cf_003".to_string(),
            1,
            "Task".to_string(),
            "key".to_string(),
            "unknown_relation".to_string(),
            props,
        );

        let result = validate_ontology(&candidate);
        assert!(!result.accepted);
        assert!(result
            .errors
            .iter()
            .any(|e| matches!(e, ValidationError::UnknownRelationType { .. })));
    }

    #[test]
    fn test_validate_ontology_invalid_relation_for_types() {
        let props = CandidateProperties {
            observed_at: 1234567890,
            source: "/wiki/test.md".to_string(),
            confidence: 1.0,
            extraction_method: "event_envelope".to_string(),
            extractor: "test".to_string(),
            mechanism: None,
            evidence_span: None,
            severity: None,
        };
        // assigned_to only allows Task → Agent, not Agent → Task
        let candidate = CandidateFact::new(
            "cf_004".to_string(),
            1,
            "Agent".to_string(),
            "Codex".to_string(),
            "assigned_to".to_string(),
            props,
        )
        .with_object("Task".to_string(), "task-1".to_string());

        let result = validate_ontology(&candidate);
        assert!(!result.accepted);
        assert!(result
            .errors
            .iter()
            .any(|e| matches!(e, ValidationError::InvalidRelationForTypes { .. })));
    }

    #[test]
    fn test_validate_ontology_missing_required_property() {
        let mut props = CandidateProperties {
            observed_at: 1234567890,
            source: "".to_string(), // empty source
            confidence: 1.0,
            extraction_method: "event_envelope".to_string(),
            extractor: "test".to_string(),
            mechanism: None,
            evidence_span: None,
            severity: None,
        };
        let candidate = CandidateFact::new(
            "cf_005".to_string(),
            1,
            "Task".to_string(),
            "key".to_string(),
            "assigned_to".to_string(),
            props.clone(),
        );

        let result = validate_ontology(&candidate);
        assert!(!result.accepted);
        assert!(result.errors.iter().any(|e| matches!(e, ValidationError::MissingRequiredProperty { property } if property == "source")));

        // Also test missing extraction_method
        props.source = "/wiki/test.md".to_string();
        props.extraction_method = "".to_string();
        let candidate2 = CandidateFact::new(
            "cf_006".to_string(),
            1,
            "Task".to_string(),
            "key".to_string(),
            "assigned_to".to_string(),
            props,
        );
        let result2 = validate_ontology(&candidate2);
        assert!(!result.accepted);
        assert!(result2.errors.iter().any(|e| matches!(e, ValidationError::MissingRequiredProperty { property } if property == "extraction_method")));
    }

    #[test]
    fn test_validate_ontology_invalid_confidence() {
        let props = CandidateProperties {
            observed_at: 1234567890,
            source: "/wiki/test.md".to_string(),
            confidence: 1.5, // out of range
            extraction_method: "event_envelope".to_string(),
            extractor: "test".to_string(),
            mechanism: None,
            evidence_span: None,
            severity: None,
        };
        let candidate = CandidateFact::new(
            "cf_007".to_string(),
            1,
            "Task".to_string(),
            "key".to_string(),
            "assigned_to".to_string(),
            props,
        );

        let result = validate_ontology(&candidate);
        assert!(!result.accepted);
        assert!(result
            .errors
            .iter()
            .any(|e| matches!(e, ValidationError::InvalidConfidence { .. })));
    }

    #[test]
    fn test_validate_ontology_causal_requires_mechanism() {
        let props = CandidateProperties {
            observed_at: 1234567890,
            source: "/wiki/test.md".to_string(),
            confidence: 0.9,
            extraction_method: "llm_candidate".to_string(),
            extractor: "test".to_string(),
            mechanism: None, // missing
            evidence_span: None,
            severity: None,
        };
        let candidate = CandidateFact::new(
            "cf_008".to_string(),
            1,
            "Failure".to_string(),
            "bug-1".to_string(),
            "caused_by".to_string(),
            props,
        )
        .with_object("Event".to_string(), "deploy-failure".to_string());

        let result = validate_ontology(&candidate);
        assert!(!result.accepted);
        assert!(result
            .errors
            .iter()
            .any(|e| matches!(e, ValidationError::MissingMechanism { .. })));
    }

    #[test]
    fn test_validate_ontology_causal_with_mechanism_passes() {
        let props = CandidateProperties {
            observed_at: 1234567890,
            source: "/wiki/test.md".to_string(),
            confidence: 0.9,
            extraction_method: "llm_candidate".to_string(),
            extractor: "test".to_string(),
            mechanism: Some("race condition in concurrent write".to_string()),
            evidence_span: Some("lines 45-52".to_string()),
            severity: None,
        };
        let candidate = CandidateFact::new(
            "cf_009".to_string(),
            1,
            "Failure".to_string(),
            "bug-1".to_string(),
            "caused_by".to_string(),
            props,
        )
        .with_object("Event".to_string(), "deploy-failure".to_string());

        let result = validate_ontology(&candidate);
        assert!(
            result.accepted,
            "Causal with mechanism should pass: {:?}",
            result.errors
        );
    }

    #[test]
    fn test_validate_ontology_confidence_method_mismatch_warning() {
        let props = CandidateProperties {
            observed_at: 1234567890,
            source: "/wiki/test.md".to_string(),
            confidence: 0.5, // too low for deterministic method
            extraction_method: "frontmatter".to_string(),
            extractor: "test".to_string(),
            mechanism: None,
            evidence_span: None,
            severity: None,
        };
        let candidate = CandidateFact::new(
            "cf_010".to_string(),
            1,
            "Task".to_string(),
            "key".to_string(),
            "assigned_to".to_string(),
            props,
        )
        .with_object("Agent".to_string(), "Codex".to_string());

        let result = validate_ontology(&candidate);
        assert!(result.accepted, "Should accept with warning, not reject");
        assert!(result
            .warnings
            .iter()
            .any(|w| w.contains("Deterministic method")));
    }

    #[test]
    fn test_find_duplicate_detects_same_fact() {
        let conn = in_memory_db();
        setup_schema(&conn);
        let doc_id = sample_doc(&conn);

        let c1 = sample_candidate(doc_id);
        insert(&conn, &c1).unwrap();

        let dup = find_duplicate(
            &conn,
            "Task",
            "graph-memory-impl",
            "assigned_to",
            Some("Agent"),
            Some("Codex"),
        )
        .unwrap();
        assert_eq!(dup, Some("cf_test_001".to_string()));
    }

    #[test]
    fn test_find_duplicate_no_match_for_different() {
        let conn = in_memory_db();
        setup_schema(&conn);
        let doc_id = sample_doc(&conn);

        let c1 = sample_candidate(doc_id);
        insert(&conn, &c1).unwrap();

        let dup = find_duplicate(
            &conn,
            "Task",
            "other-task",
            "assigned_to",
            Some("Agent"),
            Some("Codex"),
        )
        .unwrap();
        assert!(dup.is_none());
    }

    #[test]
    fn test_duplicate_ignores_rejected_status() {
        let conn = in_memory_db();
        setup_schema(&conn);
        let doc_id = sample_doc(&conn);

        let c1 = sample_candidate(doc_id);
        insert(&conn, &c1).unwrap();
        update_status(
            &conn,
            "cf_test_001",
            CandidateStatus::Rejected,
            Some("test"),
        )
        .unwrap();

        // Rejected facts should not be found as duplicates
        let dup = find_duplicate(
            &conn,
            "Task",
            "graph-memory-impl",
            "assigned_to",
            Some("Agent"),
            Some("Codex"),
        )
        .unwrap();
        assert!(dup.is_none(), "Rejected fact should not count as duplicate");
    }
}
