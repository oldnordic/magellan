use anyhow::Result;
use magellan::output::{output_json, JsonResponse, OutputFormat};
use magellan::temporal::query::{
    load_edge_barcode, load_scc_barcodes, load_symbol_barcode, load_temporal_status,
    lookup_symbol_as_of,
};
use std::path::PathBuf;

pub fn run_temporal_status(db_path: PathBuf, output_format: OutputFormat) -> Result<()> {
    let response = load_temporal_status(&db_path)?;
    let exec_id = magellan::output::generate_execution_id();

    if output_format == OutputFormat::Json || output_format == OutputFormat::Pretty {
        let json_response = JsonResponse::new(response, &exec_id);
        return output_json(&json_response, output_format);
    }

    println!("Temporal status for {}", db_path.display());
    println!("  Snapshots: {}", response.snapshot_count);
    println!("  File versions: {}", response.file_version_count);
    println!("  Symbol versions: {}", response.symbol_version_count);
    println!("  Edge versions: {}", response.edge_version_count);
    if let Some(commit_oid) = response.latest_commit_oid {
        println!("  Latest commit: {}", commit_oid);
    }
    Ok(())
}

pub fn run_temporal_barcode(
    db_path: PathBuf,
    stable_id: Option<String>,
    edge_source: Option<String>,
    edge_target: Option<String>,
    edge_kind: Option<String>,
    scc: bool,
    output_format: OutputFormat,
) -> Result<()> {
    let exec_id = magellan::output::generate_execution_id();

    if scc {
        let response = load_scc_barcodes(&db_path)?;
        if output_format == OutputFormat::Json || output_format == OutputFormat::Pretty {
            let json_response = JsonResponse::new(response, &exec_id);
            return output_json(&json_response, output_format);
        }

        println!("Temporal SCC barcode report");
        println!("  SCCs: {}", response.count);
        for scc in response.sccs {
            println!(
                "  {} members={} snapshots={} churn={}",
                scc.lineage_key, scc.member_count, scc.snapshot_count, scc.churn_count
            );
        }
        return Ok(());
    }

    if let (Some(edge_source), Some(edge_target)) = (edge_source.as_deref(), edge_target.as_deref())
    {
        let edge_kind = edge_kind.as_deref().unwrap_or("CALLS");
        let response = load_edge_barcode(&db_path, edge_source, edge_target, edge_kind)?;
        if output_format == OutputFormat::Json || output_format == OutputFormat::Pretty {
            let json_response = JsonResponse::new(response, &exec_id);
            return output_json(&json_response, output_format);
        }

        println!(
            "Temporal edge barcode for {} -> {} ({})",
            edge_source, edge_target, edge_kind
        );
        println!("  Snapshots: {}", response.snapshot_count);
        if let Some(first_commit) = response.first_commit_oid.as_deref() {
            println!("  First commit: {}", first_commit);
        }
        if let Some(last_commit) = response.last_commit_oid.as_deref() {
            println!("  Last commit: {}", last_commit);
        }
        for point in response.points {
            println!("  {}", point.commit_oid);
        }
        return Ok(());
    }

    let stable_id =
        stable_id.ok_or_else(|| anyhow::anyhow!("--symbol is required unless --scc is used"))?;
    let response = load_symbol_barcode(&db_path, &stable_id)?;
    if output_format == OutputFormat::Json || output_format == OutputFormat::Pretty {
        let json_response = JsonResponse::new(response, &exec_id);
        return output_json(&json_response, output_format);
    }

    println!("Temporal barcode for {}", response.stable_id);
    println!("  Snapshots: {}", response.snapshot_count);
    if let Some(first_commit) = response.first_commit_oid.as_deref() {
        println!("  First commit: {}", first_commit);
    }
    if let Some(last_commit) = response.last_commit_oid.as_deref() {
        println!("  Last commit: {}", last_commit);
    }
    for point in response.points {
        println!("  {} {} {}", point.commit_oid, point.file_path, point.name);
    }
    Ok(())
}

pub fn run_as_of(
    db_path: PathBuf,
    commit_oid: String,
    symbol_name: String,
    output_format: OutputFormat,
) -> Result<()> {
    let response = lookup_symbol_as_of(&db_path, &commit_oid, &symbol_name)?;
    let exec_id = magellan::output::generate_execution_id();

    if output_format == OutputFormat::Json || output_format == OutputFormat::Pretty {
        let json_response = JsonResponse::new(response, &exec_id);
        return output_json(&json_response, output_format);
    }

    println!(
        "Snapshot {} at commit {}",
        response.snapshot_id, response.commit_oid
    );
    println!("  Matches: {}", response.count);
    for symbol in response.matches {
        println!("  {} {} {}", symbol.file_path, symbol.kind, symbol.name);
    }
    Ok(())
}
