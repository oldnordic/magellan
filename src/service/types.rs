//! Service types shared between daemon and CLI client

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// A registered project entry in the registry
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectEntry {
    pub name: String,
    pub root: PathBuf,
    pub db: PathBuf,
    pub source: String,
    pub enabled: bool,
    #[serde(default)]
    pub registered_at: String,
}

impl ProjectEntry {
    pub fn new(name: String, root: PathBuf, db: PathBuf, source: String) -> Self {
        let registered_at = chrono::Utc::now().to_rfc3339();
        Self {
            name,
            root,
            db,
            source,
            enabled: true,
            registered_at,
        }
    }
}

/// JSON-RPC-like request from CLI to daemon
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceRequest {
    pub id: String,
    pub method: String,
    #[serde(flatten)]
    pub params: serde_json::Value,
}

/// JSON-RPC-like response from daemon to CLI
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceResponse {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ServiceError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl ServiceResponse {
    pub fn ok(id: String, result: serde_json::Value) -> Self {
        Self {
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn err(id: String, code: i32, message: String) -> Self {
        Self {
            id,
            result: None,
            error: Some(ServiceError {
                code,
                message,
                data: None,
            }),
        }
    }

    pub fn not_implemented(id: String, method: String) -> Self {
        Self::err(
            id,
            -32001,
            format!("Method '{}' not implemented in Phase 0", method),
        )
    }

    /// Convert to a plain serde_json::Value (the wire format)
    pub fn into_val(self) -> serde_json::Value {
        match serde_json::to_value(&self) {
            Ok(v) => v,
            Err(_) => serde_json::json!({
                "id": self.id,
                "error": { "code": -32603, "message": "Failed to serialize response" }
            }),
        }
    }
}

/// Tagged filesystem event batch for multi-root dispatcher
#[derive(Debug, Clone)]
pub struct TaggedBatch {
    pub project_name: String,
    pub paths: Vec<PathBuf>,
}
