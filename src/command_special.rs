use crate::{refresh_cmd, service, service_cmd};
use magellan::output::{output_json, JsonResponse, MigrateResponse, OutputFormat};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

pub fn handle_migrate(
    db_path: PathBuf,
    dry_run: bool,
    no_backup: bool,
    output_format: OutputFormat,
) -> ExitCode {
    match crate::migrate_cmd::run_migrate(db_path, dry_run, no_backup) {
        Ok(result) => match output_format {
            OutputFormat::Json | OutputFormat::Pretty => {
                let response = MigrateResponse {
                    success: result.success,
                    backup_path: result.backup_path.map(|p| p.to_string_lossy().to_string()),
                    old_version: result.old_version,
                    new_version: result.new_version,
                    message: result.message,
                };
                let exec_id = crate::generate_execution_id();
                let json_response = JsonResponse::new(response, &exec_id);
                if let Err(e) = output_json(&json_response, output_format) {
                    eprintln!("Error: {}", e);
                    return ExitCode::from(1);
                }
            }
            OutputFormat::Human => {
                if result.success {
                    println!("{}", result.message);
                    if result.old_version != result.new_version {
                        println!("Version: {} -> {}", result.old_version, result.new_version);
                    }
                    if let Some(ref backup) = result.backup_path {
                        println!("Backup: {}", backup.display());
                    }
                } else {
                    eprintln!("Migration failed: {}", result.message);
                    return ExitCode::from(1);
                }
            }
        },
        Err(e) => {
            eprintln!("Error: {}", e);
            return ExitCode::from(1);
        }
    }
    ExitCode::SUCCESS
}

pub fn handle_migrate_backend(
    input_db: PathBuf,
    output_db: PathBuf,
    export_dir: Option<PathBuf>,
    dry_run: bool,
    output_format: OutputFormat,
) -> ExitCode {
    match magellan::migrate_backend_cmd::run_migrate_backend(
        input_db, output_db, export_dir, dry_run,
    ) {
        Ok(result) => match output_format {
            OutputFormat::Json | OutputFormat::Pretty => {
                let exec_id = crate::generate_execution_id();
                let json_data = serde_json::json!({
                    "success": result.success,
                    "source_format": format!("{:?}", result.source_format),
                    "target_format": format!("{:?}", result.target_format),
                    "entities_migrated": result.entities_migrated,
                    "edges_migrated": result.edges_migrated,
                    "side_tables_migrated": result.side_tables_migrated,
                    "message": result.message,
                    "execution_id": exec_id,
                });
                if let Err(e) = output_json(&JsonResponse::new(json_data, &exec_id), output_format)
                {
                    eprintln!("Error: {}", e);
                    return ExitCode::from(1);
                }
            }
            OutputFormat::Human => {
                if result.success {
                    println!("{}", result.message);
                    println!(
                        "Format: {:?} -> {:?}",
                        result.source_format, result.target_format
                    );
                    println!("Entities: {}", result.entities_migrated);
                    println!("Edges: {}", result.edges_migrated);
                    if result.side_tables_migrated {
                        println!("Side tables: migrated");
                    }
                } else {
                    eprintln!("Migration failed: {}", result.message);
                    return ExitCode::from(1);
                }
            }
        },
        Err(e) => {
            eprintln!("Error: {}", e);
            return ExitCode::from(1);
        }
    }
    ExitCode::SUCCESS
}

pub fn handle_refresh(
    raw_db_path: PathBuf,
    dry_run: bool,
    include_untracked: bool,
    staged: bool,
    unstaged: bool,
    force: bool,
    output_format: OutputFormat,
) -> ExitCode {
    let db_path = if raw_db_path.as_path() == Path::new(".magellan/magellan.db") {
        match refresh_cmd::resolve_db_path(None) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("Warning: registry lookup failed ({}), using default", e);
                raw_db_path
            }
        }
    } else {
        raw_db_path
    };
    let args = refresh_cmd::RefreshArgs {
        db_path,
        dry_run,
        include_untracked,
        staged,
        unstaged,
        force,
        output_format,
    };
    match refresh_cmd::run_refresh(&args) {
        Ok(report) => {
            match output_format {
                OutputFormat::Json | OutputFormat::Pretty => {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&report).unwrap_or_default()
                    );
                }
                OutputFormat::Human => {
                    println!("Refresh complete:");
                    println!("  Updated: {}", report.updated.len());
                    println!("  Deleted: {}", report.deleted.len());
                    println!("  Added: {}", report.added.len());
                    println!("  Unchanged: {}", report.unchanged);
                    if report.dry_run {
                        println!("  (dry run - no changes applied)");
                    }
                }
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            ExitCode::from(1)
        }
    }
}

pub fn handle_service(action: service_cmd::ServiceAction, output_format: OutputFormat) -> ExitCode {
    let runtime = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("Error: failed to create async runtime: {}", e);
            return ExitCode::from(1);
        }
    };
    if let Err(e) = runtime.block_on(async { service_cmd::run(action, output_format).await }) {
        eprintln!("Error: {}", e);
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}

pub fn handle_service_daemon() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();
    let runtime = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("Error: failed to create async runtime: {}", e);
            return ExitCode::from(1);
        }
    };
    if let Err(e) = runtime.block_on(async {
        let (svc, _shutdown_rx) = service::Service::new().await?;
        svc.run().await
    }) {
        eprintln!("Error: {}", e);
        return ExitCode::from(1);
    }
    ExitCode::SUCCESS
}
