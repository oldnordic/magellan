//! Structural Analogy Engine — Phase 4
//!
//! Computes structural fingerprints (hash + bag-of-kinds vector) for symbols
//! using their AST node sequences. Drives the cross-project similarity index.

use anyhow::Result;
use magellan::{extract_ast_nodes, is_structural_kind, AstNode};
use sha2::{Digest, Sha256};
use std::path::PathBuf;

use super::meta_db::MetaDb;

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

/// Build cross-project structural similarity pairs.
///
/// For each `(project_name, db_path)` pair: opens the project DB, iterates every
/// symbol across all indexed files, extracts AST nodes from source, computes
/// `structural_hash` + `kind_vector`, and upserts into `concept_embeddings`.
///
/// After all embeddings are stored, loads them all back, decodes the packed f32
/// vectors, and performs pairwise cosine similarity across **different** projects.
/// Pairs with similarity ≥ `threshold` are inserted into `pattern_cross_refs`.
///
/// Returns the number of cross-ref pairs inserted.
pub fn build_cross_refs(
    meta_db: &mut MetaDb,
    db_paths: &[(String, PathBuf)],
    threshold: f32,
) -> Result<usize> {
    // Phase 1: compute and upsert embeddings for every symbol in every project
    for (project, db_path) in db_paths {
        let mut graph = magellan::CodeGraph::open(db_path)?;
        let file_map = graph.all_file_nodes_readonly()?;
        let file_paths: Vec<String> = file_map.into_keys().collect();

        for file_path in file_paths {
            let source = match std::fs::read(&file_path) {
                Ok(s) => s,
                Err(_) => continue, // file deleted or unreadable — skip
            };

            let lang = match magellan::detect_language(std::path::Path::new(&file_path)) {
                Some(l) => l,
                None => continue,
            };
            let ast_nodes =
                magellan::parse_with_language(lang, |parser| -> Option<Vec<AstNode>> {
                    let tree = parser.parse(&source, None)?;
                    Some(extract_ast_nodes(&tree, &source))
                })?
                .unwrap_or_default();

            let symbols = match graph.symbols_in_file(&file_path) {
                Ok(s) => s,
                Err(_) => continue,
            };
            for sym in symbols {
                let name = match &sym.name {
                    Some(n) => n.clone(),
                    None => continue,
                };
                let hash = structural_hash(&ast_nodes, sym.byte_start, sym.byte_end);
                let vec = kind_vector(&ast_nodes, sym.byte_start, sym.byte_end);
                meta_db.upsert_embedding(project, &name, &file_path, &hash, &vec)?;
            }
        }
    }

    // Phase 2: pairwise similarity across different projects
    let embeddings = meta_db.list_embeddings()?;
    let decoded: Vec<(String, String, String, Vec<f32>)> = embeddings
        .into_iter()
        .map(|e| {
            let floats: Vec<f32> = e
                .vec
                .chunks_exact(4)
                .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                .collect();
            (e.project, e.symbol, e.file, floats)
        })
        .collect();

    let mut inserted = 0usize;
    for i in 0..decoded.len() {
        for j in (i + 1)..decoded.len() {
            let (proj_a, sym_a, file_a, vec_a) = &decoded[i];
            let (proj_b, sym_b, file_b, vec_b) = &decoded[j];
            if proj_a == proj_b {
                continue;
            }
            let score = cosine_similarity(vec_a, vec_b);
            if score >= threshold {
                meta_db.insert_cross_ref(
                    proj_a,
                    sym_a,
                    file_a,
                    proj_b,
                    sym_b,
                    file_b,
                    score as f64,
                )?;
                inserted += 1;
            }
        }
    }
    Ok(inserted)
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
    use magellan::{AstNode, CodeGraph};

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

    // ── build_cross_refs ──

    fn make_project_db(dir: &std::path::Path, name: &str, src: &str) -> PathBuf {
        let db = dir.join(format!("{name}.db"));
        let mut g = CodeGraph::open(&db).unwrap();
        // Write a temp source file alongside the db so the file path is readable
        let src_path = dir.join(format!("{name}.rs"));
        std::fs::write(&src_path, src).unwrap();
        g.index_file(src_path.to_str().unwrap(), src.as_bytes())
            .unwrap();
        db
    }

    #[test]
    fn test_build_cross_refs_identical_structure_creates_pair() {
        // Both projects have a function with the same structural shape →
        // cosine similarity should be 1.0 ≥ 0.70, producing 1 cross-ref pair.
        let dir = tempfile::tempdir().unwrap();
        let src = r#"fn greet() { if true { println!("hi"); } }"#;

        let db_a = make_project_db(dir.path(), "proj_a", src);
        let db_b = make_project_db(dir.path(), "proj_b", src);

        let mut meta =
            crate::service::meta_db::MetaDb::open_at(dir.path().join("meta.db")).unwrap();

        let pairs = build_cross_refs(
            &mut meta,
            &[("proj_a".to_string(), db_a), ("proj_b".to_string(), db_b)],
            0.70,
        )
        .unwrap();

        // At minimum one cross-ref pair should have been created
        assert!(pairs > 0, "expected ≥1 cross-ref pair, got {pairs}");

        // Verify it was persisted in meta.db
        let refs = meta.query_cross_refs_for_symbol("proj_a", "greet").unwrap();
        assert!(
            !refs.is_empty(),
            "expected stored cross-ref for proj_a::greet"
        );
        assert_eq!(refs[0].project_b, "proj_b");
    }

    #[test]
    fn test_build_cross_refs_same_project_skipped() {
        // Only one project → no cross-project pairs possible.
        let dir = tempfile::tempdir().unwrap();
        let src = r#"fn hello() { if true {} }"#;
        let db_a = make_project_db(dir.path(), "only", src);

        let mut meta =
            crate::service::meta_db::MetaDb::open_at(dir.path().join("meta.db")).unwrap();

        let pairs = build_cross_refs(&mut meta, &[("only".to_string(), db_a)], 0.70).unwrap();
        assert_eq!(pairs, 0, "single project should produce no cross-refs");
    }

    #[test]
    fn test_build_cross_refs_low_similarity_skipped() {
        // Two projects with completely different structural shapes → score below threshold.
        let dir = tempfile::tempdir().unwrap();
        // proj_a: function with many if/match/loop nodes → rich vector
        let src_a = r#"fn complex() { if true { match 1 { 1 => {}, _ => {} } } while false {} for _ in 0..1 {} }"#;
        // proj_b: minimal function with no structural nodes
        let src_b = r#"fn tiny() {}"#;

        let db_a = make_project_db(dir.path(), "rich", src_a);
        let db_b = make_project_db(dir.path(), "sparse", src_b);

        let mut meta =
            crate::service::meta_db::MetaDb::open_at(dir.path().join("meta.db")).unwrap();

        // Use a very high threshold so nothing passes
        let pairs = build_cross_refs(
            &mut meta,
            &[("rich".to_string(), db_a), ("sparse".to_string(), db_b)],
            0.99,
        )
        .unwrap();
        assert_eq!(
            pairs, 0,
            "dissimilar vectors should not cross threshold 0.99"
        );
    }
}
