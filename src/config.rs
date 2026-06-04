//! Configuration management for Magellan
//!
//! Loads settings from ~/.config/magellan/config.toml
//!
//! # Config File Format
//!
//! ```toml
//! [llm]
//! provider = "ollama"  # ollama, openai, anthropic, custom
//! base_url = "http://localhost:11434"
//! model = "codellama"
//! api_key = ""  # For cloud providers
//!
//! [registry]
//! auto_scan = true
//! scan_roots = ["/home/feanor/Projects"]
//! ```

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// LLM provider type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum LlmProvider {
    #[default]
    Ollama,
    OpenAi,
    Anthropic,
    Custom,
}

/// LLM configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    pub provider: LlmProvider,
    pub base_url: String,
    pub model: String,
    #[serde(default)]
    pub api_key: String,
}

impl Default for LlmConfig {
    fn default() -> Self {
        LlmConfig {
            provider: LlmProvider::Ollama,
            base_url: "http://localhost:11434".to_string(),
            model: "codellama".to_string(),
            api_key: String::new(),
        }
    }
}

/// Registry configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryConfig {
    #[serde(default = "default_auto_scan")]
    pub auto_scan: bool,
    #[serde(default)]
    pub scan_roots: Vec<String>,
}

fn default_auto_scan() -> bool {
    true
}

impl Default for RegistryConfig {
    fn default() -> Self {
        RegistryConfig {
            auto_scan: true,
            scan_roots: vec!["/home/feanor/Projects".to_string()],
        }
    }
}

/// Root Magellan configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub llm: LlmConfig,
    #[serde(default)]
    pub registry: RegistryConfig,
    #[serde(default)]
    pub embeddings: EmbeddingsConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingsConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_embeddings_base_url")]
    pub base_url: String,
    #[serde(default = "default_embeddings_model")]
    pub model: String,
}

fn default_embeddings_base_url() -> String {
    "http://localhost:11434".to_string()
}

fn default_embeddings_model() -> String {
    "nomic-embed-text".to_string()
}

impl Default for EmbeddingsConfig {
    fn default() -> Self {
        EmbeddingsConfig {
            enabled: false,
            base_url: default_embeddings_base_url(),
            model: default_embeddings_model(),
        }
    }
}

/// Get the default config path: ~/.config/magellan/config.toml
pub fn default_config_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home)
        .join(".config")
        .join("magellan")
        .join("config.toml")
}

/// Load configuration from default location
///
/// Returns Default config if file doesn't exist or is invalid.
pub fn load() -> Result<Config> {
    let path = default_config_path();
    load_from(&path)
}

/// Load configuration from a specific path
pub fn load_from(path: &PathBuf) -> Result<Config> {
    if !path.exists() {
        return Ok(Config::default());
    }

    let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read config from {}", path.display()))?;

    toml::from_str(&content)
        .with_context(|| format!("Failed to parse config from {}", path.display()))
}

/// Save configuration to default location
pub fn save(config: &Config) -> Result<()> {
    let path = default_config_path();
    save_to(config, &path)
}

/// Save configuration to a specific path
pub fn save_to(config: &Config, path: &PathBuf) -> Result<()> {
    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create config directory: {}", parent.display()))?;
    }

    let content = toml::to_string_pretty(config).context("Failed to serialize config to TOML")?;

    std::fs::write(path, content)
        .with_context(|| format!("Failed to write config to {}", path.display()))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.llm.provider, LlmProvider::Ollama);
        assert_eq!(config.llm.base_url, "http://localhost:11434");
        assert!(config.registry.auto_scan);
    }

    #[test]
    fn test_config_path() {
        let path = default_config_path();
        assert!(path
            .to_string_lossy()
            .contains(".config/magellan/config.toml"));
    }

    #[test]
    fn test_load_from_invalid_path() {
        let result = load_from(&PathBuf::from("/nonexistent/path/config.toml"));
        assert!(result.is_ok());
    }

    #[test]
    fn test_default_embeddings_disabled() {
        let config = Config::default();
        assert!(!config.embeddings.enabled);
        assert_eq!(config.embeddings.model, "nomic-embed-text");
    }

    #[test]
    fn test_parse_embeddings_enabled() {
        let toml_str = r#"
[embeddings]
enabled = true
model = "custom-model"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert!(config.embeddings.enabled);
        assert_eq!(config.embeddings.model, "custom-model");
        assert_eq!(config.embeddings.base_url, "http://localhost:11434");
    }
}
