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
    dim: std::sync::atomic::AtomicUsize,
    num_ctx: usize,
}

impl OllamaEmbedder {
    pub fn new(base_url: &str, model: &str, num_ctx: usize) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            model: model.to_string(),
            dim: std::sync::atomic::AtomicUsize::new(0),
            num_ctx,
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

    fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        let url = format!("{}/api/embed", self.base_url);
        let mut body = serde_json::json!({
            "model": self.model,
            "input": texts,
        });
        if self.num_ctx > 0 {
            body["options"] = serde_json::json!({ "num_ctx": self.num_ctx });
        }
        let response_body = http_post_json(&url, &body, "")?;
        // Ollama format: {"embeddings": [[...], ...]}
        let embedding_arrays = response_body
            .get("embeddings")
            .and_then(|v| v.as_array())
            .ok_or_else(|| anyhow::anyhow!("ollama embed: no embeddings array in response"))?;
        let refs: Vec<&serde_json::Value> = embedding_arrays.iter().collect();
        parse_vector_arrays(&refs, "ollama")
    }

    fn dimension(&self) -> usize {
        auto_detect_dim(&self.dim, || self.embed_batch(&["x"]).ok())
    }

    fn name(&self) -> &str {
        "ollama"
    }
}

/// OpenAI-compatible embedding endpoint.
///
/// Works with: llama.cpp server (`/v1/embeddings`), vLLM, any OpenAI-compatible API.
/// Request: `POST /v1/embeddings` with `{"model": "...", "input": [...]}`
/// Response: `{"data": [{"embedding": [...]}, ...]}`
pub struct OpenAICompatEmbedder {
    base_url: String,
    model: String,
    api_key: String,
    dim: std::sync::atomic::AtomicUsize,
}

impl OpenAICompatEmbedder {
    pub fn new(base_url: &str, model: &str, api_key: &str) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            model: model.to_string(),
            api_key: api_key.to_string(),
            dim: std::sync::atomic::AtomicUsize::new(0),
        }
    }
}

impl TextEmbedder for OpenAICompatEmbedder {
    fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let results = self.embed_batch(&[text])?;
        results
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("openai-compat embed: no result returned"))
    }

    fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        let url = format!("{}/v1/embeddings", self.base_url);
        let body = serde_json::json!({
            "model": self.model,
            "input": texts,
        });
        let response_body = http_post_json(&url, &body, &self.api_key)?;
        // OpenAI format: {"data": [{"embedding": [...]}, ...]}
        let data_array = response_body
            .get("data")
            .and_then(|v| v.as_array())
            .ok_or_else(|| anyhow::anyhow!("openai-compat embed: no data array in response"))?;
        let embedding_arrays: Vec<&serde_json::Value> = data_array
            .iter()
            .filter_map(|item| item.get("embedding"))
            .collect();
        if embedding_arrays.len() != texts.len() {
            return Err(anyhow::anyhow!(
                "openai-compat embed: expected {} embeddings, got {}",
                texts.len(),
                embedding_arrays.len()
            ));
        }
        parse_vector_arrays(&embedding_arrays, "openai-compat")
    }

    fn dimension(&self) -> usize {
        auto_detect_dim(&self.dim, || self.embed_batch(&["x"]).ok())
    }

    fn name(&self) -> &str {
        "openai-compat"
    }
}

// ── Shared helpers ──

fn http_post_json(url: &str, body: &serde_json::Value, api_key: &str) -> Result<serde_json::Value> {
    let mut req = ureq::post(url)
        .config()
        .timeout_global(Some(std::time::Duration::from_secs(120)))
        .build()
        .header("Content-Type", "application/json");
    if !api_key.is_empty() {
        req = req.header("Authorization", &format!("Bearer {}", api_key));
    }
    let mut response = req
        .send_json(body)
        .map_err(|e| anyhow::anyhow!("embed request failed: {}", e))?;
    let body_text = response
        .body_mut()
        .read_to_string()
        .map_err(|e| anyhow::anyhow!("embed response read failed: {}", e))?;
    serde_json::from_str(&body_text)
        .map_err(|e| anyhow::anyhow!("embed response parse failed: {}", e))
}

fn parse_vector_arrays(
    arrays: &[&serde_json::Value],
    provider_name: &str,
) -> Result<Vec<Vec<f32>>> {
    let mut results = Vec::with_capacity(arrays.len());
    for arr in arrays {
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
                "{}: empty embedding vector in batch",
                provider_name
            ));
        }
        results.push(vec);
    }
    Ok(results)
}

fn auto_detect_dim<F>(dim: &std::sync::atomic::AtomicUsize, probe: F) -> usize
where
    F: FnOnce() -> Option<Vec<Vec<f32>>>,
{
    use std::sync::atomic::Ordering;

    let d = dim.load(Ordering::Relaxed);
    if d > 0 {
        return d;
    }
    if let Some(vectors) = probe() {
        if let Some(first) = vectors.first() {
            let detected = first.len();
            dim.store(detected, Ordering::Relaxed);
            return detected;
        }
    }
    // Fallback: nomic-embed-text default
    let fallback = 768;
    dim.store(fallback, Ordering::Relaxed);
    fallback
}

pub fn create_embedder(
    provider: &crate::config::EmbedProvider,
    enabled: bool,
    base_url: &str,
    model: &str,
    api_key: &str,
    num_ctx: usize,
) -> Box<dyn TextEmbedder> {
    if !enabled {
        return Box::new(HashEmbedder::new(128));
    }
    match provider {
        crate::config::EmbedProvider::Ollama => {
            Box::new(OllamaEmbedder::new(base_url, model, num_ctx))
        }
        crate::config::EmbedProvider::OpenAi => {
            Box::new(OpenAICompatEmbedder::new(base_url, model, api_key))
        }
        crate::config::EmbedProvider::Hash => Box::new(HashEmbedder::new(128)),
    }
}

pub fn symbol_embed_text(entity: &sqlitegraph::GraphEntity, body: Option<&str>) -> String {
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
    if let Some(body) = body {
        let truncated = if body.len() > EMBED_BODY_MAX_CHARS {
            let mut end = EMBED_BODY_MAX_CHARS;
            while !body.is_char_boundary(end) && end > 0 {
                end -= 1;
            }
            &body[..end]
        } else {
            body
        };
        parts.push(truncated.to_string());
    }
    parts.join(" ")
}

/// Maximum source body length included in embedding text.
/// Prevents extremely large function bodies from dominating the embedding.
const EMBED_BODY_MAX_CHARS: usize = 1024;

pub fn symbol_fact_embed_text(
    name: &Option<String>,
    file_path: &str,
    kind_normalized: &str,
    body: Option<&str>,
) -> String {
    let mut parts = vec!["Symbol".to_string()];
    if let Some(name) = name {
        parts.push(name.clone());
    }
    parts.push(file_path.to_string());
    parts.push(kind_normalized.to_string());
    if let Some(body) = body {
        let truncated = if body.len() > EMBED_BODY_MAX_CHARS {
            // Find the last char boundary at or before the limit
            let mut end = EMBED_BODY_MAX_CHARS;
            while !body.is_char_boundary(end) && end > 0 {
                end -= 1;
            }
            &body[..end]
        } else {
            body
        };
        parts.push(truncated.to_string());
    }
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
        let embedder = create_embedder(&crate::config::EmbedProvider::Hash, false, "", "", "", 0);
        assert_eq!(embedder.name(), "hash");
        assert_eq!(embedder.dimension(), 128);
    }

    #[test]
    fn test_create_embedder_ollama() {
        let embedder = create_embedder(
            &crate::config::EmbedProvider::Ollama,
            true,
            "http://localhost:11434",
            "nomic-embed-text",
            "",
            0,
        );
        assert_eq!(embedder.name(), "ollama");
        // dimension is auto-detected on first call; without ollama running it
        // falls back to 768. Just verify it returns a positive value.
        assert!(embedder.dimension() > 0);
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
        let text = symbol_embed_text(&entity, None);
        assert!(text.contains("Symbol"));
        assert!(text.contains("parse_rust"));
        assert!(text.contains("magellan::parse_rust"));
        assert!(text.contains("function"));
        assert!(text.contains("rust"));
    }

    #[test]
    fn test_symbol_embed_text_with_body() {
        let entity = sqlitegraph::GraphEntity {
            id: 2,
            kind: "Symbol".to_string(),
            name: "my_function".to_string(),
            file_path: Some("src/lib.rs".to_string()),
            data: serde_json::json!({
                "fqn": "my_crate::my_function",
                "kind_normalized": "function",
                "language": "rust",
            }),
        };
        let text_no_body = symbol_embed_text(&entity, None);
        let text_with_body = symbol_embed_text(&entity, Some("fn my_function() -> i32 { 42 }"));

        assert!(
            !text_no_body.contains("fn my_function()"),
            "no-body should not contain source: {:?}",
            text_no_body
        );
        assert!(
            text_with_body.contains("fn my_function()"),
            "with-body should contain source: {:?}",
            text_with_body
        );
        assert!(
            text_with_body.contains("42"),
            "with-body should contain function body"
        );
        assert!(
            text_with_body.contains("my_crate::my_function"),
            "with-body should still have fqn"
        );
    }

    #[test]
    fn test_symbol_fact_embed_text_with_body() {
        let name = Some("compute_value".to_string());
        let text_no_body = symbol_fact_embed_text(&name, "src/calc.rs", "fn", None);
        let text_with_body = symbol_fact_embed_text(
            &name,
            "src/calc.rs",
            "fn",
            Some("fn compute_value(x: i32) -> i32 { x * 2 + 1 }"),
        );

        assert!(
            text_no_body.contains("Symbol"),
            "should start with Symbol marker"
        );
        assert!(
            text_no_body.contains("compute_value"),
            "should contain name"
        );
        assert!(
            !text_no_body.contains("x * 2"),
            "no-body should not contain source body"
        );

        assert!(
            text_with_body.contains("compute_value"),
            "with-body should contain name"
        );
        assert!(
            text_with_body.contains("fn compute_value"),
            "with-body should contain function signature"
        );
        assert!(
            text_with_body.contains("x * 2 + 1"),
            "with-body should contain function body expression"
        );
    }

    #[test]
    fn test_symbol_fact_embed_text_body_truncation() {
        let long_body = "fn big_fn() { ".to_string() + &"let x = 1; ".repeat(200) + "}";
        let name = Some("big_fn".to_string());
        let text = symbol_fact_embed_text(&name, "src/big.rs", "fn", Some(&long_body));

        assert!(
            text.contains("big_fn"),
            "should contain name even with truncated body"
        );
        // Body should be truncated to ~1024 chars, not the full ~1800 chars
        assert!(
            text.len() < 1200,
            "text should be under ~1200 chars with truncation, got {}",
            text.len()
        );
    }
}
