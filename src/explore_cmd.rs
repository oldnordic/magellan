use anyhow::Result;
use magellan::graph::navigator::{DepthSymbol, SymbolInfo, TypedEdgeHop};
use magellan::graph::CodeGraph;
use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub enum OutputFormat {
    Human,
    Json,
}

pub struct ExploreConfig {
    pub db_path: PathBuf,
    pub symbol: Option<String>,
    pub id: Option<i64>,
    pub edges: bool,
    pub callers: bool,
    pub callees: bool,
    pub chain: Option<String>,
    pub depth: u32,
    pub format: OutputFormat,
}

#[derive(Serialize)]
struct ExploreResponse {
    node: Option<NodeResponse>,
    resolve: Option<Vec<NodeResponse>>,
    edges: Option<Vec<EdgeResponse>>,
    callers: Option<Vec<DepthResponse>>,
    callees: Option<Vec<DepthResponse>>,
    chain: Option<Vec<NodeResponse>>,
}

#[derive(Serialize)]
struct NodeResponse {
    id: i64,
    name: String,
    kind: String,
    file: Option<String>,
    line: usize,
}

#[derive(Serialize)]
struct EdgeResponse {
    edge_type: String,
    direction: String,
    target: NodeResponse,
}

#[derive(Serialize)]
struct DepthResponse {
    depth: u32,
    node: NodeResponse,
}

fn to_node(info: &SymbolInfo) -> NodeResponse {
    NodeResponse {
        id: info.id,
        name: info.name.clone(),
        kind: info
            .kind_normalized
            .clone()
            .unwrap_or_else(|| info.kind.clone()),
        file: info.file_path.clone(),
        line: info.start_line,
    }
}

fn to_depth(ds: &DepthSymbol) -> DepthResponse {
    DepthResponse {
        depth: ds.depth,
        node: to_node(&ds.info),
    }
}

fn to_edge(hop: &TypedEdgeHop) -> EdgeResponse {
    EdgeResponse {
        edge_type: hop.edge_type.clone(),
        direction: match hop.direction {
            sqlitegraph::backend::BackendDirection::Outgoing => "out".to_string(),
            sqlitegraph::backend::BackendDirection::Incoming => "in".to_string(),
        },
        target: to_node(&hop.target),
    }
}

pub fn run_explore(cfg: ExploreConfig) -> Result<()> {
    let graph = CodeGraph::open(&cfg.db_path)?;
    let nav = graph.navigator();
    let is_json = matches!(cfg.format, OutputFormat::Json);

    let mut resp = ExploreResponse {
        node: None,
        resolve: None,
        edges: None,
        callers: None,
        callees: None,
        chain: None,
    };

    let target_id = match (cfg.symbol, cfg.id) {
        (Some(name), _) => {
            let results = nav.resolve(&name)?;
            if results.is_empty() {
                if is_json {
                    println!("{{\"error\": \"no symbols found for '{}'\"}}", name);
                } else {
                    println!("No symbols found for '{}'", name);
                }
                return Ok(());
            }
            if is_json {
                resp.resolve = Some(results.iter().map(to_node).collect());
            } else {
                for info in &results {
                    print_symbol(info);
                }
            }
            results[0].id
        }
        (None, Some(id)) => id,
        (None, None) => {
            anyhow::bail!("provide --symbol <name> or --id <id>");
        }
    };

    if let Some(info) = nav.info(target_id)? {
        resp.node = Some(to_node(&info));
        if !is_json {
            print_symbol(&info);
        }
    } else if !is_json {
        println!("No entity found with id {}", target_id);
    }

    if cfg.edges {
        let edges = nav.expand(target_id)?;
        if is_json {
            resp.edges = Some(edges.iter().map(to_edge).collect());
        } else {
            print_edges_human(&edges);
        }
    }

    if cfg.callers {
        let callers = nav.k_hop_callers(target_id, cfg.depth)?;
        if is_json {
            resp.callers = Some(callers.iter().map(to_depth).collect());
        } else if callers.is_empty() {
            println!("  (no callers)");
        } else {
            println!("  callers:");
            for c in &callers {
                let path = c.info.file_path.as_deref().unwrap_or("-");
                println!(
                    "    {} id={} depth={} ({})",
                    c.info.name, c.info.id, c.depth, path
                );
            }
        }
    }

    if cfg.callees {
        let callees = nav.k_hop_callees(target_id, cfg.depth)?;
        if is_json {
            resp.callees = Some(callees.iter().map(to_depth).collect());
        } else if callees.is_empty() {
            println!("  (no callees)");
        } else {
            println!("  callees:");
            for c in &callees {
                let path = c.info.file_path.as_deref().unwrap_or("-");
                println!(
                    "    {} id={} depth={} ({})",
                    c.info.name, c.info.id, c.depth, path
                );
            }
        }
    }

    if let Some(ref chain_spec) = cfg.chain {
        let steps = parse_chain_steps(chain_spec)?;
        let results = nav.chain(target_id, &steps)?;
        if is_json {
            resp.chain = Some(results.iter().map(to_node).collect());
        } else if results.is_empty() {
            println!("  (chain returned no results)");
        } else {
            println!("  chain:");
            for r in &results {
                let path = r.file_path.as_deref().unwrap_or("-");
                println!("    {} id={} ({})", r.name, r.id, path);
            }
        }
    }

    if is_json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
    }

    Ok(())
}

fn print_symbol(info: &SymbolInfo) {
    let path = info.file_path.as_deref().unwrap_or("<unknown>");
    println!(
        "id={} kind={} name=\"{}\" file={}",
        info.id,
        info.kind_normalized.as_deref().unwrap_or(&info.kind),
        info.name,
        path
    );
}

fn print_edges_human(hops: &[TypedEdgeHop]) {
    if hops.is_empty() {
        println!("  (no edges)");
        return;
    }
    for hop in hops {
        let dir = match hop.direction {
            sqlitegraph::backend::BackendDirection::Outgoing => "->",
            sqlitegraph::backend::BackendDirection::Incoming => "<-",
        };
        let path = hop.target.file_path.as_deref().unwrap_or("-");
        println!(
            "  {} [{}] {} id={} ({})",
            dir, hop.edge_type, hop.target.name, hop.target.id, path
        );
    }
}

fn parse_chain_steps(spec: &str) -> Result<Vec<sqlitegraph::multi_hop::ChainStep>> {
    use sqlitegraph::backend::BackendDirection;
    use sqlitegraph::multi_hop::ChainStep;

    let mut steps = Vec::new();
    for part in spec.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let (dir, edge_type) = if let Some(rest) = part.strip_prefix('<') {
            (BackendDirection::Incoming, rest.to_string())
        } else if let Some(rest) = part.strip_prefix('>') {
            (BackendDirection::Outgoing, rest.to_string())
        } else {
            (BackendDirection::Outgoing, part.to_string())
        };
        steps.push(ChainStep {
            direction: dir,
            edge_type: Some(edge_type),
        });
    }
    if steps.is_empty() {
        anyhow::bail!("chain spec must contain at least one step (e.g. '>CALLER,>CALLS')");
    }
    Ok(steps)
}
