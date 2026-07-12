
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
fn test_insert_duplicate_candidate_id_returns_error() {
    let conn = in_memory_db();
    setup_schema(&conn);
    let doc_id = sample_doc(&conn);

    let c1 = sample_candidate(doc_id);
    insert(&conn, &c1).unwrap();

    // Second insert with same candidate_id should fail with descriptive error
    let c2 = sample_candidate(doc_id);
    let err = insert(&conn, &c2).unwrap_err();
    let msg = format!("{:#}", err);
    assert!(
        msg.contains("candidate_id") || msg.contains("UNIQUE"),
        "Error should mention candidate_id or UNIQUE constraint: {}",
        msg
    );
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
