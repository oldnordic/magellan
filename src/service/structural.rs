//! Structural Analogy Engine — Phase 4
//!
//! Computes structural fingerprints (hash + bag-of-kinds vector) for symbols
//! using their AST node sequences. Drives the cross-project similarity index.

use magellan::{is_structural_kind, AstNode};
use sha2::{Digest, Sha256};

/// Vocabulary of structural AST kinds used for the bag-of-kinds vector.
/// Order is stable — index position maps to the vector dimension.
pub const KIND_VOCAB: &[&str] = &[
    "if_expression",
    "if_statement",
    "match_expression",
    "match_statement",
    "while_expression",
    "while_statement",
    "for_expression",
    "for_statement",
    "loop_expression",
    "return_expression",
    "return_statement",
    "call_expression",
    "let_statement",
    "let_declaration",
    "block",
    "block_expression",
    "function_item",
    "struct_item",
    "enum_item",
    "assignment_expression",
];

/// SHA-256 hex fingerprint of the structural AST kind sequence within a byte range.
///
/// Filters `nodes` to those whose `byte_start` falls within `[start, end)` and
/// whose `kind` passes `is_structural_kind`. Sorts by `byte_start`, joins kinds
/// with `"|"`, and returns the SHA-256 hex digest.
///
/// Returns an empty-input hash when no structural nodes are in range.
pub fn structural_hash(nodes: &[AstNode], start: usize, end: usize) -> String {
    let sequence = kind_sequence(nodes, start, end);
    let joined = sequence.join("|");
    let mut hasher = Sha256::new();
    hasher.update(joined.as_bytes());
    hex::encode(hasher.finalize())
}

/// Bag-of-structural-kinds histogram, L2-normalized to a unit vector.
///
/// Returns a `Vec<f32>` of length `KIND_VOCAB.len()`. Each element is the
/// normalized frequency of that kind within the symbol's byte range.
/// Returns a zero vector (not unit) when no structural nodes are found.
pub fn kind_vector(nodes: &[AstNode], start: usize, end: usize) -> Vec<f32> {
    let sequence = kind_sequence(nodes, start, end);
    let mut counts = vec![0.0f32; KIND_VOCAB.len()];
    for kind in &sequence {
        if let Some(idx) = KIND_VOCAB.iter().position(|&v| v == kind.as_str()) {
            counts[idx] += 1.0;
        }
    }
    let norm: f32 = counts.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        counts.iter_mut().for_each(|x| *x /= norm);
    }
    counts
}

/// Cosine similarity between two unit vectors. Returns 0.0 if lengths differ.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return 0.0;
    }
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

fn kind_sequence(nodes: &[AstNode], start: usize, end: usize) -> Vec<String> {
    let mut filtered: Vec<&AstNode> = nodes
        .iter()
        .filter(|n| n.byte_start >= start && n.byte_start < end && is_structural_kind(&n.kind))
        .collect();
    filtered.sort_by_key(|n| n.byte_start);
    filtered.iter().map(|n| n.kind.clone()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use magellan::AstNode;

    fn node(kind: &str, start: usize, end: usize) -> AstNode {
        AstNode::new(None, kind, start, end)
    }

    // ── structural_hash ──

    #[test]
    fn test_structural_hash_deterministic() {
        let nodes = vec![
            node("function_item", 0, 100),
            node("block", 10, 90),
            node("if_expression", 20, 60),
            node("call_expression", 30, 50),
        ];
        let h1 = structural_hash(&nodes, 0, 100);
        let h2 = structural_hash(&nodes, 0, 100);
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 64, "SHA-256 hex is 64 chars");
    }

    #[test]
    fn test_structural_hash_different_sequences_differ() {
        let nodes_a = vec![node("function_item", 0, 100), node("if_expression", 10, 50)];
        let nodes_b = vec![
            node("function_item", 0, 100),
            node("match_expression", 10, 50),
        ];
        assert_ne!(
            structural_hash(&nodes_a, 0, 100),
            structural_hash(&nodes_b, 0, 100)
        );
    }

    #[test]
    fn test_structural_hash_filters_non_structural() {
        let nodes_clean = vec![node("function_item", 0, 100)];
        let nodes_noisy = vec![
            node("function_item", 0, 100),
            node("identifier", 10, 20),     // NOT structural
            node("string_literal", 25, 35), // NOT structural
        ];
        // hash should be identical — non-structural nodes are ignored
        assert_eq!(
            structural_hash(&nodes_clean, 0, 100),
            structural_hash(&nodes_noisy, 0, 100)
        );
    }

    #[test]
    fn test_structural_hash_filters_by_byte_range() {
        let nodes = vec![
            node("function_item", 0, 200),
            node("if_expression", 50, 100), // inside range [50, 150)
            node("match_expression", 160, 200), // outside range [50, 150)
        ];
        let h_inner = structural_hash(&nodes, 50, 150);
        let h_outer = structural_hash(&nodes, 0, 200);
        assert_ne!(h_inner, h_outer);
    }

    // ── kind_vector ──

    #[test]
    fn test_kind_vector_is_unit_length() {
        let nodes = vec![
            node("function_item", 0, 100),
            node("block", 10, 90),
            node("if_expression", 20, 60),
            node("call_expression", 30, 50),
        ];
        let v = kind_vector(&nodes, 0, 100);
        assert_eq!(v.len(), KIND_VOCAB.len());
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!(
            (norm - 1.0).abs() < 1e-5,
            "expected unit vector, norm={}",
            norm
        );
    }

    #[test]
    fn test_kind_vector_zero_for_empty() {
        let nodes: Vec<AstNode> = vec![];
        let v = kind_vector(&nodes, 0, 100);
        assert!(v.iter().all(|&x| x == 0.0), "expected all-zero vector");
    }

    #[test]
    fn test_kind_vector_different_sequences_differ() {
        let nodes_a = vec![node("if_expression", 0, 50)];
        let nodes_b = vec![node("match_expression", 0, 50)];
        let va = kind_vector(&nodes_a, 0, 100);
        let vb = kind_vector(&nodes_b, 0, 100);
        // Different kinds → different vectors
        assert_ne!(va, vb);
    }

    // ── cosine_similarity ──

    #[test]
    fn test_cosine_similarity_identical() {
        let v = vec![0.6, 0.8, 0.0];
        assert!((cosine_similarity(&v, &v) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        assert!((cosine_similarity(&a, &b)).abs() < 1e-5);
    }

    #[test]
    fn test_cosine_similarity_length_mismatch_returns_zero() {
        let a = vec![1.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert_eq!(cosine_similarity(&a, &b), 0.0);
    }

    #[test]
    fn test_kind_vector_cosine_same_structure_is_one() {
        let nodes = vec![node("function_item", 0, 100), node("if_expression", 10, 50)];
        let va = kind_vector(&nodes, 0, 100);
        let vb = kind_vector(&nodes, 0, 100);
        assert!((cosine_similarity(&va, &vb) - 1.0).abs() < 1e-5);
    }
}
