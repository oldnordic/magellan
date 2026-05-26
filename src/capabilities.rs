//! Backend capability model
//!
//! Provides compile-time and runtime capability queries for Magellan backend.

use std::collections::HashSet;

/// Backend type identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BackendType {
    /// SQLite-based backend (CodeGraph with sqlitegraph)
    SQLite,
}

impl BackendType {
    /// Get the file extension for this backend type
    pub fn extension(&self) -> &'static str {
        match self {
            Self::SQLite => "db",
        }
    }

    /// Get display name for this backend
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::SQLite => "SQLite",
        }
    }

    /// Detect backend type from file extension
    pub fn from_extension(ext: Option<&str>) -> Option<Self> {
        match ext {
            Some("db") | Some("sqlite") | Some(_) => Some(Self::SQLite),
            None => Some(Self::SQLite),
        }
    }
}

/// Capability flags for the SQLite backend
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
    fn sqlite() -> Self {
        Self {
            backend_type: BackendType::SQLite,
            supports_symbol_queries: true,
            supports_call_graph: true,
            supports_cfg_analysis: true,
            supports_chunks: true,
            supports_cycles: true,
            supports_paths: false,
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

    /// Get capabilities for a backend type
    pub fn for_backend(backend_type: BackendType) -> Self {
        match backend_type {
            BackendType::SQLite => Self::sqlite(),
        }
    }

    /// Get all backends that are enabled in this build
    pub fn enabled_backends() -> Vec<BackendType> {
        vec![BackendType::SQLite]
    }

    /// Get the default backend for this build
    pub fn default_backend() -> BackendType {
        BackendType::SQLite
    }

    /// Check if a specific command is supported
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
            "doctor" | "status" | "watch" => true,
            _ => true,
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

/// Get all available backend capabilities
pub fn all_capabilities() -> Vec<BackendCapabilities> {
    vec![BackendCapabilities::for_backend(BackendType::SQLite)]
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
            supported_backends: None,
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
            supported_backends: None,
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
            supported_backends: None,
        },
        CommandMetadata {
            name: "label",
            description: "Manage symbol labels",
            required_capability: Capability::Labels,
            supported_backends: None,
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
    ]
}

/// Validate a command against backend capabilities
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

    if let Some(supported) = &metadata.supported_backends {
        if !supported.contains(&backend_caps.backend_type) {
            return Err(CommandValidationError::UnsupportedBackend {
                command: command.to_string(),
                backend: backend_caps.backend_type.display_name().to_string(),
            });
        }
    }

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
        Capability::Watch => true,
    };

    if !capability_met {
        return Err(CommandValidationError::MissingCapability {
            command: command.to_string(),
            capability: format!("{:?}", metadata.required_capability),
        });
    }

    Ok(())
}

/// Error type for command validation
#[derive(Debug, Clone)]
pub enum CommandValidationError {
    UnknownCommand { command: String },
    UnsupportedBackend { command: String, backend: String },
    MissingCapability { command: String, capability: String },
}

impl std::fmt::Display for CommandValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownCommand { command } => write!(f, "Unknown command: {}", command),
            Self::UnsupportedBackend { command, backend } => {
                write!(
                    f,
                    "Command '{}' not supported on {} backend",
                    command, backend
                )
            }
            Self::MissingCapability {
                command,
                capability,
            } => {
                write!(
                    f,
                    "Command '{}' requires {} capability (not available on this backend)",
                    command, capability
                )
            }
        }
    }
}

impl std::error::Error for CommandValidationError {}
