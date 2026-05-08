//! Ontology for graph memory — v0
//!
//! Defines allowed entity types, relation types, and validation rules
//! for CandidateFact staging. Light schema: node types, edge types,
//! cardinality. No full OWL.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Entity type in the knowledge memory ontology.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EntityType {
    Agent,
    User,
    Project,
    Task,
    Decision,
    Preference,
    Artifact,
    SourceDocument,
    Claim,
    Failure,
    Verification,
    Event,
}

impl EntityType {
    /// All allowed entity types.
    pub fn all() -> &'static [EntityType] {
        &[
            EntityType::Agent,
            EntityType::User,
            EntityType::Project,
            EntityType::Task,
            EntityType::Decision,
            EntityType::Preference,
            EntityType::Artifact,
            EntityType::SourceDocument,
            EntityType::Claim,
            EntityType::Failure,
            EntityType::Verification,
            EntityType::Event,
        ]
    }

    /// String representation used in storage and validation.
    pub fn as_str(&self) -> &'static str {
        match self {
            EntityType::Agent => "Agent",
            EntityType::User => "User",
            EntityType::Project => "Project",
            EntityType::Task => "Task",
            EntityType::Decision => "Decision",
            EntityType::Preference => "Preference",
            EntityType::Artifact => "Artifact",
            EntityType::SourceDocument => "SourceDocument",
            EntityType::Claim => "Claim",
            EntityType::Failure => "Failure",
            EntityType::Verification => "Verification",
            EntityType::Event => "Event",
        }
    }

    /// Parse from string. Returns None for unknown types.
    pub fn parse(s: &str) -> Option<EntityType> {
        match s {
            "Agent" => Some(EntityType::Agent),
            "User" => Some(EntityType::User),
            "Project" => Some(EntityType::Project),
            "Task" => Some(EntityType::Task),
            "Decision" => Some(EntityType::Decision),
            "Preference" => Some(EntityType::Preference),
            "Artifact" => Some(EntityType::Artifact),
            "SourceDocument" => Some(EntityType::SourceDocument),
            "Claim" => Some(EntityType::Claim),
            "Failure" => Some(EntityType::Failure),
            "Verification" => Some(EntityType::Verification),
            "Event" => Some(EntityType::Event),
            _ => None,
        }
    }
}

/// Relation type in the knowledge memory ontology.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RelationType {
    AssignedTo,
    AuthoredBy,
    Mentions,
    LinksTo,
    DerivedFrom,
    DecidedBy,
    DependsOn,
    Supersedes,
    InvalidatedBy,
    VerifiedBy,
    CausedBy,
    RelatesTo,
    ObservedIn,
}

impl RelationType {
    /// String representation used in storage and validation.
    pub fn as_str(&self) -> &'static str {
        match self {
            RelationType::AssignedTo => "assigned_to",
            RelationType::AuthoredBy => "authored_by",
            RelationType::Mentions => "mentions",
            RelationType::LinksTo => "links_to",
            RelationType::DerivedFrom => "derived_from",
            RelationType::DecidedBy => "decided_by",
            RelationType::DependsOn => "depends_on",
            RelationType::Supersedes => "supersedes",
            RelationType::InvalidatedBy => "invalidated_by",
            RelationType::VerifiedBy => "verified_by",
            RelationType::CausedBy => "caused_by",
            RelationType::RelatesTo => "relates_to",
            RelationType::ObservedIn => "observed_in",
        }
    }

    /// Parse from string. Returns None for unknown types.
    pub fn parse(s: &str) -> Option<RelationType> {
        match s {
            "assigned_to" => Some(RelationType::AssignedTo),
            "authored_by" => Some(RelationType::AuthoredBy),
            "mentions" => Some(RelationType::Mentions),
            "links_to" => Some(RelationType::LinksTo),
            "derived_from" => Some(RelationType::DerivedFrom),
            "decided_by" => Some(RelationType::DecidedBy),
            "depends_on" => Some(RelationType::DependsOn),
            "supersedes" => Some(RelationType::Supersedes),
            "invalidated_by" => Some(RelationType::InvalidatedBy),
            "verified_by" => Some(RelationType::VerifiedBy),
            "caused_by" => Some(RelationType::CausedBy),
            "relates_to" => Some(RelationType::RelatesTo),
            "observed_in" => Some(RelationType::ObservedIn),
            _ => None,
        }
    }

    /// Whether this relation requires a mechanism and evidence (causal).
    pub fn requires_mechanism(&self) -> bool {
        matches!(self, RelationType::CausedBy)
    }

    /// Get allowed (subject, object) type pairs for this relation.
    /// Returns empty vec for RelatesTo (Any → Any) which is always allowed.
    pub fn allowed_pairs(&self) -> Vec<(EntityType, EntityType)> {
        match self {
            RelationType::AssignedTo => {
                vec![(EntityType::Task, EntityType::Agent)]
            }
            RelationType::AuthoredBy => {
                vec![
                    (EntityType::Artifact, EntityType::Agent),
                    (EntityType::Artifact, EntityType::User),
                    (EntityType::SourceDocument, EntityType::Agent),
                    (EntityType::SourceDocument, EntityType::User),
                ]
            }
            RelationType::Mentions => {
                // SourceDocument → Any entity type
                EntityType::all()
                    .iter()
                    .map(|et| (EntityType::SourceDocument, et.clone()))
                    .collect()
            }
            RelationType::LinksTo => {
                let mut pairs = vec![];
                for et in EntityType::all() {
                    pairs.push((EntityType::SourceDocument, et.clone()));
                }
                pairs
            }
            RelationType::DerivedFrom => {
                vec![
                    (EntityType::Claim, EntityType::SourceDocument),
                    (EntityType::Decision, EntityType::SourceDocument),
                ]
            }
            RelationType::DecidedBy => {
                vec![
                    (EntityType::Decision, EntityType::Agent),
                    (EntityType::Decision, EntityType::User),
                ]
            }
            RelationType::DependsOn => {
                vec![
                    (EntityType::Task, EntityType::Task),
                    (EntityType::Task, EntityType::Decision),
                    (EntityType::Task, EntityType::Artifact),
                    (EntityType::Decision, EntityType::Task),
                    (EntityType::Decision, EntityType::Decision),
                    (EntityType::Decision, EntityType::Artifact),
                    (EntityType::Artifact, EntityType::Task),
                    (EntityType::Artifact, EntityType::Decision),
                    (EntityType::Artifact, EntityType::Artifact),
                ]
            }
            RelationType::Supersedes => {
                vec![
                    (EntityType::Decision, EntityType::Decision),
                    (EntityType::Claim, EntityType::Claim),
                    (EntityType::Preference, EntityType::Preference),
                ]
            }
            RelationType::InvalidatedBy => {
                vec![
                    (EntityType::Claim, EntityType::Failure),
                    (EntityType::Claim, EntityType::Verification),
                    (EntityType::Claim, EntityType::Decision),
                    (EntityType::Decision, EntityType::Failure),
                    (EntityType::Decision, EntityType::Verification),
                    (EntityType::Decision, EntityType::Decision),
                    (EntityType::Verification, EntityType::Failure),
                    (EntityType::Verification, EntityType::Verification),
                    (EntityType::Verification, EntityType::Decision),
                ]
            }
            RelationType::VerifiedBy => {
                vec![
                    (EntityType::Claim, EntityType::Verification),
                    (EntityType::Artifact, EntityType::Verification),
                    (EntityType::Task, EntityType::Verification),
                ]
            }
            RelationType::CausedBy => {
                vec![
                    (EntityType::Failure, EntityType::Event),
                    (EntityType::Failure, EntityType::Decision),
                    (EntityType::Failure, EntityType::Failure),
                    (EntityType::Event, EntityType::Event),
                    (EntityType::Event, EntityType::Decision),
                    (EntityType::Event, EntityType::Failure),
                    (EntityType::Decision, EntityType::Event),
                    (EntityType::Decision, EntityType::Decision),
                    (EntityType::Decision, EntityType::Failure),
                ]
            }
            RelationType::RelatesTo => {
                // Any → Any: empty vec signals "always allowed"
                vec![]
            }
            RelationType::ObservedIn => {
                // Any → SourceDocument or Event
                let mut pairs = vec![];
                for et in EntityType::all() {
                    pairs.push((et.clone(), EntityType::SourceDocument));
                    pairs.push((et.clone(), EntityType::Event));
                }
                pairs
            }
        }
    }
}

/// Ontology v0 — the complete schema definition.
#[derive(Debug, Clone)]
pub struct OntologyV0 {
    entity_types: HashSet<String>,
    relation_types: HashSet<String>,
    // Maps relation_type -> allowed (subject, object) pairs
    relation_constraints: HashMap<String, Vec<(String, String)>>,
}

impl Default for OntologyV0 {
    fn default() -> Self {
        Self::new()
    }
}

impl OntologyV0 {
    /// Create the v0 ontology with all entity and relation types.
    pub fn new() -> Self {
        let entity_types: HashSet<String> = EntityType::all()
            .iter()
            .map(|e| e.as_str().to_string())
            .collect();

        let relation_types: HashSet<String> = vec![
            RelationType::AssignedTo,
            RelationType::AuthoredBy,
            RelationType::Mentions,
            RelationType::LinksTo,
            RelationType::DerivedFrom,
            RelationType::DecidedBy,
            RelationType::DependsOn,
            RelationType::Supersedes,
            RelationType::InvalidatedBy,
            RelationType::VerifiedBy,
            RelationType::CausedBy,
            RelationType::RelatesTo,
            RelationType::ObservedIn,
        ]
        .into_iter()
        .map(|r| r.as_str().to_string())
        .collect();

        let mut relation_constraints = HashMap::new();
        for rel in [
            RelationType::AssignedTo,
            RelationType::AuthoredBy,
            RelationType::Mentions,
            RelationType::LinksTo,
            RelationType::DerivedFrom,
            RelationType::DecidedBy,
            RelationType::DependsOn,
            RelationType::Supersedes,
            RelationType::InvalidatedBy,
            RelationType::VerifiedBy,
            RelationType::CausedBy,
            RelationType::RelatesTo,
            RelationType::ObservedIn,
        ] {
            let pairs: Vec<(String, String)> = rel
                .allowed_pairs()
                .into_iter()
                .map(|(s, o)| (s.as_str().to_string(), o.as_str().to_string()))
                .collect();
            relation_constraints.insert(rel.as_str().to_string(), pairs);
        }

        OntologyV0 {
            entity_types,
            relation_types,
            relation_constraints,
        }
    }

    /// Check if an entity type is allowed.
    pub fn is_entity_type_allowed(&self, entity_type: &str) -> bool {
        self.entity_types.contains(entity_type)
    }

    /// Check if a relation type is allowed.
    pub fn is_relation_type_allowed(&self, relation_type: &str) -> bool {
        self.relation_types.contains(relation_type)
    }

    /// Check if a (subject_type, relation_type, object_type) triple is valid.
    /// Returns true for RelatesTo (Any → Any) regardless of types.
    pub fn is_relation_valid_for_types(
        &self,
        subject_type: &str,
        relation_type: &str,
        object_type: Option<&str>,
    ) -> bool {
        if !self.is_relation_type_allowed(relation_type) {
            return false;
        }

        // relates_to is always allowed (Any → Any)
        if relation_type == "relates_to" {
            return true;
        }

        let allowed_pairs = match self.relation_constraints.get(relation_type) {
            Some(pairs) => pairs,
            None => return false,
        };

        // Empty allowed_pairs with non-relates_to means no valid pairs defined
        if allowed_pairs.is_empty() {
            return false;
        }

        let obj = object_type.unwrap_or("");
        allowed_pairs
            .iter()
            .any(|(s, o)| s == subject_type && o == obj)
    }

    /// Get list of all allowed entity type strings.
    pub fn entity_types(&self) -> Vec<String> {
        self.entity_types.iter().cloned().collect()
    }

    /// Get list of all allowed relation type strings.
    pub fn relation_types(&self) -> Vec<String> {
        self.relation_types.iter().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_entity_type_roundtrip() {
        for et in EntityType::all() {
            let s = et.as_str();
            let parsed = EntityType::parse(s);
            assert_eq!(
                parsed.as_ref(),
                Some(et),
                "EntityType {} should roundtrip",
                s
            );
        }
    }

    #[test]
    fn test_entity_type_unknown_returns_none() {
        assert!(EntityType::parse("UnknownType").is_none());
        assert!(EntityType::parse("AgentX").is_none());
    }

    #[test]
    fn test_relation_type_roundtrip() {
        let all = vec![
            RelationType::AssignedTo,
            RelationType::AuthoredBy,
            RelationType::Mentions,
            RelationType::LinksTo,
            RelationType::DerivedFrom,
            RelationType::DecidedBy,
            RelationType::DependsOn,
            RelationType::Supersedes,
            RelationType::InvalidatedBy,
            RelationType::VerifiedBy,
            RelationType::CausedBy,
            RelationType::RelatesTo,
            RelationType::ObservedIn,
        ];
        for rel in &all {
            let s = rel.as_str();
            let parsed = RelationType::parse(s);
            assert_eq!(
                parsed.as_ref(),
                Some(rel),
                "RelationType {} should roundtrip",
                s
            );
        }
    }

    #[test]
    fn test_ontology_v0_has_all_entity_types() {
        let ont = OntologyV0::new();
        assert_eq!(ont.entity_types().len(), 12);
        assert!(ont.is_entity_type_allowed("Agent"));
        assert!(ont.is_entity_type_allowed("Task"));
        assert!(ont.is_entity_type_allowed("Event"));
        assert!(!ont.is_entity_type_allowed("Unknown"));
    }

    #[test]
    fn test_ontology_v0_has_all_relation_types() {
        let ont = OntologyV0::new();
        assert_eq!(ont.relation_types().len(), 13);
        assert!(ont.is_relation_type_allowed("assigned_to"));
        assert!(ont.is_relation_type_allowed("caused_by"));
        assert!(!ont.is_relation_type_allowed("unknown_rel"));
    }

    #[test]
    fn test_relation_validation_assigned_to() {
        let ont = OntologyV0::new();
        // Valid: Task → Agent
        assert!(ont.is_relation_valid_for_types("Task", "assigned_to", Some("Agent")));
        // Invalid: Agent → Task
        assert!(!ont.is_relation_valid_for_types("Agent", "assigned_to", Some("Task")));
        // Invalid: Task → Project
        assert!(!ont.is_relation_valid_for_types("Task", "assigned_to", Some("Project")));
    }

    #[test]
    fn test_relation_validation_relates_to_always_allowed() {
        let ont = OntologyV0::new();
        // Any → Any is allowed for relates_to
        assert!(ont.is_relation_valid_for_types("Agent", "relates_to", Some("Project")));
        assert!(ont.is_relation_valid_for_types("Task", "relates_to", Some("Task")));
        assert!(ont.is_relation_valid_for_types("Event", "relates_to", None));
    }

    #[test]
    fn test_relation_validation_caused_by_requires_specific_pairs() {
        let ont = OntologyV0::new();
        // Valid: Failure → Event
        assert!(ont.is_relation_valid_for_types("Failure", "caused_by", Some("Event")));
        // Valid: Decision → Decision
        assert!(ont.is_relation_valid_for_types("Decision", "caused_by", Some("Decision")));
        // Invalid: Agent → Task
        assert!(!ont.is_relation_valid_for_types("Agent", "caused_by", Some("Task")));
    }

    #[test]
    fn test_relation_validation_derived_from() {
        let ont = OntologyV0::new();
        // Valid: Claim → SourceDocument
        assert!(ont.is_relation_valid_for_types("Claim", "derived_from", Some("SourceDocument")));
        // Valid: Decision → SourceDocument
        assert!(ont.is_relation_valid_for_types(
            "Decision",
            "derived_from",
            Some("SourceDocument")
        ));
        // Invalid: Task → SourceDocument
        assert!(!ont.is_relation_valid_for_types("Task", "derived_from", Some("SourceDocument")));
    }

    #[test]
    fn test_requires_mechanism() {
        assert!(RelationType::CausedBy.requires_mechanism());
        assert!(!RelationType::AssignedTo.requires_mechanism());
        assert!(!RelationType::RelatesTo.requires_mechanism());
        assert!(!RelationType::Mentions.requires_mechanism());
    }

    #[test]
    fn test_relation_validation_unknown_relation() {
        let ont = OntologyV0::new();
        assert!(!ont.is_relation_valid_for_types("Task", "unknown_rel", Some("Agent")));
    }

    #[test]
    fn test_relation_validation_unknown_entity_type() {
        let ont = OntologyV0::new();
        // Unknown entity types should fail even for valid relations
        assert!(!ont.is_relation_valid_for_types("Unknown", "assigned_to", Some("Agent")));
        assert!(!ont.is_relation_valid_for_types("Task", "assigned_to", Some("Unknown")));
    }
}
