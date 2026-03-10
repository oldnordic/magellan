//! Backend capability model
//!
//! Provides compile-time and runtime capability queries for each Magellan backend.
//! This enables:
//! - Backend-aware help/usage messaging
//! - Command validation based on backend capabilities
//! - Build feature detection for frontend UI
//! - Operational status reporting

use std::collections::HashSet;

/// Backend type identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BackendType {
    /// SQLite-based backend (CodeGraph with sqlitegraph)
    SQLite,
    /// Geometric backend (GeoGraphDB with 3D spatial indexing)
    Geometric,
    /// Native V3 backend (sqlitegraph native mode)
    NativeV3,
}

impl BackendType {
    /// Get the file extension for this backend type
    pub fn extension(&self) -> &'static str {
        match self {
            Self::SQLite => "db",
            Self::Geometric => "geo",
            Self::NativeV3 => "v3",
        }
    }

    /// Get display name for this backend
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::SQLite => "SQLite",
            Self::Geometric => "Geometric",
            Self::NativeV3 => "Native V3",
        }
    }

    /// Detect backend type from file extension
    pub fn from_extension(ext: Option<&str>) -> Option<Self> {
        match ext {
            #[cfg(feature = "geometric-backend")]
            Some("geo") => Some(Self::Geometric),
            #[cfg(not(feature = "geometric-backend"))]
            Some("geo") => None, // Geo not built
            Some("v3") => Some(Self::NativeV3),
            Some("db") | Some("sqlite") | Some(_) => Some(Self::SQLite),
            None => Some(Self::SQLite), // Default
        }
    }
}

/// Capability flags for a backend
#[derive(Debug, Clone)]
pub struct BackendCapabilities {
    /// Backend type
    pub backend_type: BackendType,

    /// Can query symbols by name/FQN
    pub supports_symbol_queries: bool,

    /// Can query call graph (callees/callers)
    pub supports_call_graph: bool,

    /// Can analyze control flow (CFG blocks, edges)
    pub supports_cfg_analysis: bool,

    /// Can store/retrieve code chunks
    pub supports_chunks: bool,

    /// Can detect cycles (SCCs) in call graph
    pub supports_cycles: bool,

    /// Can enumerate execution paths (CFG-based)
    pub supports_paths: bool,

    /// Can compute program slices (forward/backward reachability)
    pub supports_slice: bool,

    /// Supports historical snapshots (time travel queries)
    pub supports_historical_snapshot: bool,

    /// Supports vacuum/maintenance operations
    pub supports_vacuum_maintenance: bool,

    /// Can analyze dead code
    pub supports_dead_code: bool,

    /// Can compute reachability (transitive closure)
    pub supports_reachability: bool,

    /// Can export to external formats (LSIF, JSON)
    pub supports_export: bool,

    /// Has AST node storage
    pub supports_ast: bool,

    /// Has label/annotation support
    pub supports_labels: bool,

    /// Recommended file extension for new databases
    pub database_extension_hint: String,

    /// Human-readable format description
    pub format_hint: String,

    /// Whether this backend is enabled in current build
    pub build_enabled: bool,

    /// Cargo feature required to enable this backend
    pub required_feature: Option<String>,

    /// Compilation flag for conditional compilation
    pub cfg_feature: Option<String>,
}

impl BackendCapabilities {
    /// Get capabilities for SQLite backend
    #[cfg(feature = "sqlite-backend")]
    fn sqlite() -> Self {
        Self {
            backend_type: BackendType::SQLite,
            supports_symbol_queries: true,
            supports_call_graph: true,
            supports_cfg_analysis: true,
            supports_chunks: true,
            supports_cycles: true,
            supports_paths: false, // CFG-based paths not in SQLite
            supports_slice: true,
            supports_historical_snapshot: false,
            supports_vacuum_maintenance: true,
            supports_dead_code: true,
            supports_reachability: true,
            supports_export: true,
            supports_ast: true,
            supports_labels: true,
            database_extension_hint: "db".to_string(),
            format_hint: "SQLite3 database with sqlitegraph".to_string(),
            build_enabled: true,
            required_feature: Some("sqlite-backend".to_string()),
            cfg_feature: Some("sqlite-backend".to_string()),
        }
    }

    /// Get capabilities for Geometric backend
    #[cfg(feature = "geometric-backend")]
    fn geometric() -> Self {
        Self {
            backend_type: BackendType::Geometric,
            supports_symbol_queries: true,
            supports_call_graph: true,
            supports_cfg_analysis: true,
            supports_chunks: true,
            supports_cycles: true,
            supports_paths: true, // Geo supports path enumeration
            supports_slice: true,
            supports_historical_snapshot: false,
            supports_vacuum_maintenance: true, // CFG vacuum
            supports_dead_code: true,
            supports_reachability: true,
            supports_export: true,
            supports_ast: false,    // Not yet implemented in Geo
            supports_labels: false, // Not yet implemented in Geo
            database_extension_hint: "geo".to_string(),
            format_hint: "GeoGraphDB single-file bundle with spatial indexing".to_string(),
            build_enabled: true,
            required_feature: Some("geometric-backend".to_string()),
            cfg_feature: Some("geometric-backend".to_string()),
        }
    }

    /// Get capabilities for Native V3 backend
    #[cfg(feature = "native-v3")]
    fn native_v3() -> Self {
        Self {
            backend_type: BackendType::NativeV3,
            supports_symbol_queries: true,
            supports_call_graph: true,
            supports_cfg_analysis: true,
            supports_chunks: true,
            supports_cycles: true,
            supports_paths: false, // Not yet in V3
            supports_slice: true,
            supports_historical_snapshot: false,
            supports_vacuum_maintenance: true,
            supports_dead_code: true,
            supports_reachability: true,
            supports_export: true,
            supports_ast: true,
            supports_labels: true,
            database_extension_hint: "v3".to_string(),
            format_hint: "sqlitegraph native V3 format".to_string(),
            build_enabled: true,
            required_feature: Some("native-v3".to_string()),
            cfg_feature: Some("native-v3".to_string()),
        }
    }

    /// Get capabilities for a backend type (returns empty if not built)
    pub fn for_backend(backend_type: BackendType) -> Self {
        match backend_type {
            #[cfg(feature = "sqlite-backend")]
            BackendType::SQLite => Self::sqlite(),
            #[cfg(not(feature = "sqlite-backend"))]
            BackendType::SQLite => Self::not_built(BackendType::SQLite, "sqlite-backend"),

            #[cfg(feature = "geometric-backend")]
            BackendType::Geometric => Self::geometric(),
            #[cfg(not(feature = "geometric-backend"))]
            BackendType::Geometric => Self::not_built(BackendType::Geometric, "geometric-backend"),

            #[cfg(feature = "native-v3")]
            BackendType::NativeV3 => Self::native_v3(),
            #[cfg(not(feature = "native-v3"))]
            BackendType::NativeV3 => Self::not_built(BackendType::NativeV3, "native-v3"),
        }
    }

    /// Create a "not built" capability set
    fn not_built(backend_type: BackendType, feature: &str) -> Self {
        Self {
            backend_type,
            supports_symbol_queries: false,
            supports_call_graph: false,
            supports_cfg_analysis: false,
            supports_chunks: false,
            supports_cycles: false,
            supports_paths: false,
            supports_slice: false,
            supports_historical_snapshot: false,
            supports_vacuum_maintenance: false,
            supports_dead_code: false,
            supports_reachability: false,
            supports_export: false,
            supports_ast: false,
            supports_labels: false,
            database_extension_hint: backend_type.extension().to_string(),
            format_hint: format!("Not built (requires `--features {}`)", feature),
            build_enabled: false,
            required_feature: Some(feature.to_string()),
            cfg_feature: Some(feature.to_string()),
        }
    }

    /// Get all backends that are enabled in this build
    pub fn enabled_backends() -> Vec<BackendType> {
        let mut backends = Vec::new();

        #[cfg(feature = "sqlite-backend")]
        backends.push(BackendType::SQLite);

        #[cfg(feature = "geometric-backend")]
        backends.push(BackendType::Geometric);

        #[cfg(feature = "native-v3")]
        backends.push(BackendType::NativeV3);

        backends
    }

    /// Get the default backend for this build
    pub fn default_backend() -> BackendType {
        // Priority order: SQLite > Geometric > NativeV3
        #[cfg(feature = "sqlite-backend")]
        return BackendType::SQLite;

        #[cfg(all(not(feature = "sqlite-backend"), feature = "geometric-backend"))]
        return BackendType::Geometric;

        #[cfg(all(not(feature = "sqlite-backend"), not(feature = "geometric-backend")))]
        return BackendType::NativeV3;
    }

    /// Check if a specific command is supported by this backend
    pub fn supports_command(&self, command: &str) -> bool {
        match command {
            "find" | "query" | "refs" | "get" => self.supports_symbol_queries,
            "cycles" | "reachable" => self.supports_cycles && self.supports_call_graph,
            "slice" => self.supports_slice,
            "paths" => self.supports_paths,
            "dead-code" => self.supports_dead_code,
            "export" => self.supports_export,
            "label" => self.supports_labels,
            "ast" => self.supports_ast,
            "cfg" => self.supports_cfg_analysis,
            "doctor" | "status" | "watch" => true, // Operational commands always work
            _ => true,                             // Allow unknown commands through
        }
    }

    /// Get unsupported commands as a sorted list
    pub fn unsupported_commands(&self, all_commands: &[String]) -> Vec<String> {
        all_commands
            .iter()
            .filter(|cmd| !self.supports_command(cmd))
            .cloned()
            .collect()
    }

    /// Get a human-readable capability summary
    pub fn capability_summary(&self) -> String {
        let mut features = Vec::new();

        if self.supports_symbol_queries {
            features.push("symbol queries");
        }
        if self.supports_call_graph {
            features.push("call graph");
        }
        if self.supports_cfg_analysis {
            features.push("CFG analysis");
        }
        if self.supports_cycles {
            features.push("cycle detection");
        }
        if self.supports_paths {
            features.push("path enumeration");
        }
        if self.supports_slice {
            features.push("program slicing");
        }
        if self.supports_vacuum_maintenance {
            features.push("vacuum/maintenance");
        }
        if self.supports_export {
            features.push("export");
        }
        if self.supports_ast {
            features.push("AST queries");
        }
        if self.supports_labels {
            features.push("labels");
        }

        if features.is_empty() {
            "No features (backend not built)".to_string()
        } else {
            features.join(", ")
        }
    }

    /// Format as a table row for status output
    pub fn table_row(&self) -> Vec<String> {
        vec![
            self.backend_type.display_name().to_string(),
            self.database_extension_hint.clone(),
            if self.build_enabled {
                "Yes".to_string()
            } else {
                "No".to_string()
            },
            self.required_feature.clone().unwrap_or_default(),
            self.capability_summary(),
        ]
    }
}

/// Get all available backend capabilities (including disabled ones)
pub fn all_capabilities() -> Vec<BackendCapabilities> {
    vec![
        BackendCapabilities::for_backend(BackendType::SQLite),
        BackendCapabilities::for_backend(BackendType::Geometric),
        BackendCapabilities::for_backend(BackendType::NativeV3),
    ]
}

/// Get capabilities for a specific file path
pub fn capabilities_for_path(path: &std::path::Path) -> BackendCapabilities {
    let backend_type = BackendType::from_extension(path.extension().and_then(|e| e.to_str()))
        .unwrap_or(BackendType::SQLite);

    BackendCapabilities::for_backend(backend_type)
}

/// Command metadata for validation
#[derive(Debug, Clone)]
pub struct CommandMetadata {
    /// Command name (as used in CLI)
    pub name: &'static str,
    /// Human-readable description
    pub description: &'static str,
    /// Minimum capability required
    pub required_capability: Capability,
    /// Backend types that support this command (None = all)
    pub supported_backends: Option<Vec<BackendType>>,
}

/// Capability requirements for commands
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Capability {
    /// No special capability required (operational commands)
    None,
    /// Symbol query capability
    SymbolQuery,
    /// Call graph capability
    CallGraph,
    /// CFG analysis capability
    CfgAnalysis,
    /// Cycle detection capability
    Cycles,
    /// Path enumeration capability
    Paths,
    /// Program slicing capability
    Slice,
    /// Dead code analysis capability
    DeadCode,
    /// Export capability
    Export,
    /// AST query capability
    Ast,
    /// Label/annotation capability
    Labels,
    /// Watch/index capability
    Watch,
}

/// Get metadata for all Magellan commands
pub fn command_metadata() -> Vec<CommandMetadata> {
    vec![
        CommandMetadata {
            name: "find",
            description: "Find symbols by name or pattern",
            required_capability: Capability::SymbolQuery,
            supported_backends: None, // All backends
        },
        CommandMetadata {
            name: "query",
            description: "Query database with filters",
            required_capability: Capability::SymbolQuery,
            supported_backends: None,
        },
        CommandMetadata {
            name: "refs",
            description: "Find references to a symbol",
            required_capability: Capability::CallGraph,
            supported_backends: None,
        },
        CommandMetadata {
            name: "cycles",
            description: "Detect cycles in call graph",
            required_capability: Capability::Cycles,
            supported_backends: None,
        },
        CommandMetadata {
            name: "reachable",
            description: "Find reachable symbols from entry point",
            required_capability: Capability::Cycles,
            supported_backends: None,
        },
        CommandMetadata {
            name: "dead-code",
            description: "Find dead code from entry points",
            required_capability: Capability::DeadCode,
            supported_backends: None,
        },
        CommandMetadata {
            name: "slice",
            description: "Compute program slice (forward/backward)",
            required_capability: Capability::Slice,
            supported_backends: None,
        },
        CommandMetadata {
            name: "paths",
            description: "Enumerate execution paths (CFG-based)",
            required_capability: Capability::Paths,
            supported_backends: Some(vec![BackendType::Geometric]),
        },
        CommandMetadata {
            name: "export",
            description: "Export database to external format",
            required_capability: Capability::Export,
            supported_backends: None,
        },
        CommandMetadata {
            name: "ast",
            description: "Query AST nodes",
            required_capability: Capability::Ast,
            supported_backends: Some(vec![BackendType::SQLite, BackendType::NativeV3]),
        },
        CommandMetadata {
            name: "label",
            description: "Manage symbol labels",
            required_capability: Capability::Labels,
            supported_backends: Some(vec![BackendType::SQLite, BackendType::NativeV3]),
        },
        CommandMetadata {
            name: "get",
            description: "Get symbol details",
            required_capability: Capability::SymbolQuery,
            supported_backends: None,
        },
        CommandMetadata {
            name: "watch",
            description: "Watch directory for changes",
            required_capability: Capability::Watch,
            supported_backends: None,
        },
        CommandMetadata {
            name: "status",
            description: "Show database status",
            required_capability: Capability::None,
            supported_backends: None,
        },
        CommandMetadata {
            name: "doctor",
            description: "Check database health",
            required_capability: Capability::None,
            supported_backends: None,
        },
        CommandMetadata {
            name: "cfg",
            description: "Show control flow graph",
            required_capability: Capability::CfgAnalysis,
            supported_backends: None,
        },
        // Add more commands as needed
    ]
}

/// Validate a command against backend capabilities
///
/// Returns Ok(()) if the command is supported, or Err with explanation
pub fn validate_command(
    command: &str,
    backend_caps: &BackendCapabilities,
) -> Result<(), CommandValidationError> {
    let all_commands = command_metadata();
    let metadata = all_commands.iter().find(|m| m.name == command).ok_or(
        CommandValidationError::UnknownCommand {
            command: command.to_string(),
        },
    )?;

    // Check if backend supports this command
    if let Some(supported) = &metadata.supported_backends {
        if !supported.contains(&backend_caps.backend_type) {
            return Err(CommandValidationError::UnsupportedBackend {
                command: command.to_string(),
                backend: backend_caps.backend_type.display_name().to_string(),
                supported_backends: supported
                    .iter()
                    .map(|b| b.display_name().to_string())
                    .collect::<Vec<_>>()
                    .join(", "),
            });
        }
    }

    // Check capability requirements
    let capability_met = match metadata.required_capability {
        Capability::None => true,
        Capability::SymbolQuery => backend_caps.supports_symbol_queries,
        Capability::CallGraph => backend_caps.supports_call_graph,
        Capability::CfgAnalysis => backend_caps.supports_cfg_analysis,
        Capability::Cycles => backend_caps.supports_cycles,
        Capability::Paths => backend_caps.supports_paths,
        Capability::Slice => backend_caps.supports_slice,
        Capability::DeadCode => backend_caps.supports_dead_code,
        Capability::Export => backend_caps.supports_export,
        Capability::Ast => backend_caps.supports_ast,
        Capability::Labels => backend_caps.supports_labels,
        Capability::Watch => true, // All backends support watching
    };

    if !capability_met {
        return Err(CommandValidationError::MissingCapability {
            command: command.to_string(),
            backend: backend_caps.backend_type.display_name().to_string(),
            capability: format!("{:?}", metadata.required_capability),
        });
    }

    // Check if backend is built
    if !backend_caps.build_enabled {
        return Err(CommandValidationError::BackendNotBuilt {
            backend: backend_caps.backend_type.display_name().to_string(),
            feature: backend_caps.required_feature.clone().unwrap_or_default(),
        });
    }

    Ok(())
}

/// Validation errors for commands
#[derive(Debug, Clone, thiserror::Error)]
pub enum CommandValidationError {
    #[error("Unknown command: '{command}'")]
    UnknownCommand { command: String },

    #[error("Command '{command}' is not supported by {backend} backend (supported: {supported_backends})")]
    UnsupportedBackend {
        command: String,
        backend: String,
        supported_backends: String,
    },

    #[error(
        "Command '{command}' requires {:?} capability, not available in {backend} backend",
        capability
    )]
    MissingCapability {
        command: String,
        backend: String,
        capability: String,
    },

    #[error("Backend {backend} is not enabled in this build. Rebuild with --features {feature}")]
    BackendNotBuilt { backend: String, feature: String },
}

/// Get all available commands for a backend
pub fn available_commands(backend_caps: &BackendCapabilities) -> Vec<&'static str> {
    command_metadata()
        .iter()
        .filter_map(|cmd| {
            if validate_command(cmd.name, backend_caps).is_ok() {
                Some(cmd.name)
            } else {
                None
            }
        })
        .collect()
}

/// Get unsupported commands for a backend
pub fn unsupported_commands(backend_caps: &BackendCapabilities) -> Vec<&'static str> {
    command_metadata()
        .iter()
        .filter_map(|cmd| {
            if validate_command(cmd.name, backend_caps).is_err() {
                Some(cmd.name)
            } else {
                None
            }
        })
        .collect()
}

/// Format command availability for display
pub fn format_command_availability(backend_caps: &BackendCapabilities) -> String {
    let available = available_commands(backend_caps);
    let unsupported = unsupported_commands(backend_caps);

    let mut output = String::new();

    output.push_str(&format!(
        "Backend: {} ({})\n",
        backend_caps.backend_type.display_name(),
        backend_caps.database_extension_hint
    ));

    if !backend_caps.build_enabled {
        output.push_str(&format!(
            "Status: Not built (requires --features {})\n",
            backend_caps
                .required_feature
                .as_deref()
                .unwrap_or("unknown")
        ));
        return output;
    }

    output.push_str(&format!("Status: Enabled\n\n"));

    output.push_str(&format!("Available commands ({}):\n", available.len()));
    for cmd in &available {
        if let Some(meta) = command_metadata().iter().find(|m| m.name == *cmd) {
            output.push_str(&format!("  {} - {}\n", cmd, meta.description));
        }
    }

    if !unsupported.is_empty() {
        output.push_str(&format!(
            "\nUnsupported commands ({}):\n",
            unsupported.len()
        ));
        for cmd in &unsupported {
            if let Some(meta) = command_metadata().iter().find(|m| m.name == *cmd) {
                let reason = if let Some(supported) = &meta.supported_backends {
                    if !supported.contains(&backend_caps.backend_type) {
                        format!(
                            "(only: {})",
                            supported
                                .iter()
                                .map(|b| b.display_name().to_string())
                                .collect::<Vec<_>>()
                                .join(", ")
                        )
                    } else {
                        "(capability not available)".to_string()
                    }
                } else {
                    "(capability not available)".to_string()
                };
                output.push_str(&format!("  {} - {} {}\n", cmd, meta.description, reason));
            }
        }
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backend_type_extension() {
        assert_eq!(BackendType::SQLite.extension(), "db");
        assert_eq!(BackendType::Geometric.extension(), "geo");
        assert_eq!(BackendType::NativeV3.extension(), "v3");
    }

    #[test]
    fn test_from_extension() {
        assert_eq!(
            BackendType::from_extension(Some("db")),
            Some(BackendType::SQLite)
        );
        assert_eq!(
            BackendType::from_extension(Some("sqlite")),
            Some(BackendType::SQLite)
        );
        assert_eq!(
            BackendType::from_extension(Some("v3")),
            Some(BackendType::NativeV3)
        );
        // Geo depends on feature flag
    }

    #[test]
    fn test_default_backend_exists() {
        let default = BackendCapabilities::default_backend();
        let caps = BackendCapabilities::for_backend(default);
        // At least one backend should always be built
        let enabled = BackendCapabilities::enabled_backends();
        assert!(!enabled.is_empty(), "At least one backend must be enabled");
    }

    #[test]
    fn test_command_support() {
        let caps = BackendCapabilities::for_backend(BackendType::SQLite);
        assert!(caps.supports_command("status"));
        assert!(caps.supports_command("find"));
    }

    #[test]
    fn test_validate_command_basic() {
        let sqlite_caps = BackendCapabilities::for_backend(BackendType::SQLite);
        // Operational commands always work
        assert!(validate_command("status", &sqlite_caps).is_ok());
        assert!(validate_command("doctor", &sqlite_caps).is_ok());

        // SQLite should support find, cycles, etc.
        assert!(validate_command("find", &sqlite_caps).is_ok());
        assert!(validate_command("cycles", &sqlite_caps).is_ok());
        assert!(validate_command("slice", &sqlite_caps).is_ok());
    }

    #[test]
    fn test_validate_unknown_command() {
        let caps = BackendCapabilities::for_backend(BackendType::SQLite);
        let result = validate_command("not_a_real_command", &caps);
        assert!(result.is_err());
        match result.unwrap_err() {
            CommandValidationError::UnknownCommand { command } => {
                assert_eq!(command, "not_a_real_command");
            }
            _ => panic!("Expected UnknownCommand error"),
        }
    }

    #[test]
    fn test_available_commands() {
        let caps = BackendCapabilities::for_backend(BackendType::SQLite);
        let available = available_commands(&caps);
        assert!(!available.is_empty());
        assert!(available.contains(&"status"));
        assert!(available.contains(&"find"));
    }

    #[test]
    fn test_format_command_availability() {
        let caps = BackendCapabilities::for_backend(BackendType::SQLite);
        let output = format_command_availability(&caps);
        assert!(output.contains("SQLite"));
        assert!(output.contains("Available commands"));
    }

    #[cfg(feature = "geometric-backend")]
    #[test]
    fn test_paths_command_geo_only() {
        let geo_caps = BackendCapabilities::for_backend(BackendType::Geometric);
        assert!(validate_command("paths", &geo_caps).is_ok());

        let sqlite_caps = BackendCapabilities::for_backend(BackendType::SQLite);
        assert!(validate_command("paths", &sqlite_caps).is_err());
    }
}
