use anyhow::Result;

pub trait TextEmbedder: Send + Sync {
    fn embed(&self, text: &str) -> Result<Vec<f32>>;
    fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        texts.iter().map(|t| self.embed(t)).collect()
    }
    fn dimension(&self) -> usize;
    fn name(&self) -> &str;
}

pub struct HashEmbedder {
    dim: usize,
}

impl HashEmbedder {
    pub fn new(dim: usize) -> Self {
        Self { dim }
    }
}

impl TextEmbedder for HashEmbedder {
    fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let mut vec = vec![0.0f32; self.dim];
        let lower = text.to_ascii_lowercase();
        for (i, token) in lower
            .split(|c: char| !c.is_alphanumeric())
            .filter(|t| !t.is_empty())
            .enumerate()
        {
            let hash = xxhash_rust::xxh3::xxh3_64(token.as_bytes());
            let slot = (hash as usize) % self.dim;
            let idx = (i + slot) % self.dim;
            vec[idx] += 1.0;
        }
        let norm: f32 = vec.iter().map(|v| v * v).sum::<f32>().sqrt();
        if norm > 0.0 {
            for v in &mut vec {
                *v /= norm;
            }
        }
        Ok(vec)
    }

    fn dimension(&self) -> usize {
        self.dim
    }

    fn name(&self) -> &str {
        "hash"
    }
}

pub struct OllamaEmbedder {
    base_url: String,
    model: String,
    dim: usize,
}

impl OllamaEmbedder {
    pub fn new(base_url: &str, model: &str) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            model: model.to_string(),
            dim: 768,
        }
    }
}

impl TextEmbedder for OllamaEmbedder {
    fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let results = self.embed_batch(&[text])?;
        results
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("ollama embed: no result returned"))
    }

    fn dimension(&self) -> usize {
        self.dim
    }

    fn name(&self) -> &str {
        "ollama"
    }
}

impl OllamaEmbedder {
    pub fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        let url = format!("{}/api/embed", self.base_url);
        let body = serde_json::json!({
            "model": self.model,
            "input": texts,
        });
        let mut response = ureq::post(&url)
            .config()
            .timeout_global(Some(std::time::Duration::from_secs(120)))
            .build()
            .header("Content-Type", "application/json")
            .send_json(&body)
            .map_err(|e| anyhow::anyhow!("ollama embed request failed: {}", e))?;
        let body_text = response
            .body_mut()
            .read_to_string()
            .map_err(|e| anyhow::anyhow!("ollama embed response read failed: {}", e))?;
        let response_body: serde_json::Value = serde_json::from_str(&body_text)
            .map_err(|e| anyhow::anyhow!("ollama embed response parse failed: {}", e))?;
        let embedding_arrays = response_body
            .get("embeddings")
            .and_then(|v| v.as_array())
            .ok_or_else(|| anyhow::anyhow!("ollama embed: no embeddings array in response"))?;
        let mut results = Vec::with_capacity(embedding_arrays.len());
        for arr in embedding_arrays {
            let vec: Vec<f32> = arr
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_f64().map(|f| f as f32))
                        .collect()
                })
                .unwrap_or_default();
            if vec.is_empty() {
                return Err(anyhow::anyhow!(
                    "ollama embed: empty embedding vector in batch"
                ));
            }
            results.push(vec);
        }
        Ok(results)
    }
}

pub fn create_embedder(enabled: bool, base_url: &str, model: &str) -> Box<dyn TextEmbedder> {
    if enabled {
        Box::new(OllamaEmbedder::new(base_url, model))
    } else {
        Box::new(HashEmbedder::new(128))
    }
}

pub fn symbol_embed_text(entity: &sqlitegraph::GraphEntity) -> String {
    let mut parts = vec![entity.kind.clone(), entity.name.clone()];
    for key in &[
        "fqn",
        "canonical_fqn",
        "display_fqn",
        "file_path",
        "kind_normalized",
    ] {
        if let Some(value) = entity.data.get(key).and_then(|v| v.as_str()) {
            parts.push(value.to_string());
        }
    }
    if let Some(lang) = entity.data.get("language").and_then(|v| v.as_str()) {
        parts.push(lang.to_string());
    }
    parts.join(" ")
}

pub fn symbol_fact_embed_text(
    name: &Option<String>,
    file_path: &str,
    kind_normalized: &str,
) -> String {
    let mut parts = vec!["Symbol".to_string()];
    if let Some(name) = name {
        parts.push(name.clone());
    }
    parts.push(file_path.to_string());
    parts.push(kind_normalized.to_string());
    parts.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_embedder_dimension() {
        let embedder = HashEmbedder::new(128);
        assert_eq!(embedder.dimension(), 128);
    }

    #[test]
    fn test_hash_embedder_basic() {
        let embedder = HashEmbedder::new(128);
        let vec = embedder.embed("fn parse_rust").unwrap();
        assert_eq!(vec.len(), 128);
        let norm: f32 = vec.iter().map(|v| v * v).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 0.01, "should be unit vector");
    }

    #[test]
    fn test_hash_embedder_shared_tokens() {
        let embedder = HashEmbedder::new(128);
        let a = embedder.embed("fn parse_rust").unwrap();
        let b = embedder.embed("fn parse_python").unwrap();
        let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        assert!(
            dot > 0.3,
            "shared 'fn' 'parse' tokens should give positive cosine, got {}",
            dot
        );
    }

    #[test]
    fn test_hash_embedder_no_shared_tokens() {
        let embedder = HashEmbedder::new(128);
        let a = embedder.embed("sync_claude_transcript").unwrap();
        let b = embedder.embed("process_file_operations").unwrap();
        let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        assert!(
            dot < 0.1,
            "no shared tokens should give near-zero cosine, got {}",
            dot
        );
    }

    #[test]
    fn test_create_embedder_hash() {
        let embedder = create_embedder(false, "", "");
        assert_eq!(embedder.name(), "hash");
        assert_eq!(embedder.dimension(), 128);
    }

    #[test]
    fn test_create_embedder_ollama() {
        let embedder = create_embedder(true, "http://localhost:11434", "nomic-embed-text");
        assert_eq!(embedder.name(), "ollama");
        assert_eq!(embedder.dimension(), 768);
    }

    #[test]
    fn test_symbol_embed_text() {
        let entity = sqlitegraph::GraphEntity {
            id: 1,
            kind: "Symbol".to_string(),
            name: "parse_rust".to_string(),
            file_path: Some("src/lib.rs".to_string()),
            data: serde_json::json!({
                "fqn": "magellan::parse_rust",
                "kind_normalized": "function",
                "language": "rust",
            }),
        };
        let text = symbol_embed_text(&entity);
        assert!(text.contains("Symbol"));
        assert!(text.contains("parse_rust"));
        assert!(text.contains("magellan::parse_rust"));
        assert!(text.contains("function"));
        assert!(text.contains("rust"));
    }
}
