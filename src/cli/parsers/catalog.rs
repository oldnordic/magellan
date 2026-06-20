use anyhow::Result;
use magellan::OutputFormat;

use crate::cli::parsers::*;
use crate::cli::Command;

// ============================================================================
// Catalog Parsers
// ============================================================================

pub fn parse_catalog_args(args: &[String]) -> Result<Command> {
    let mut output_format = OutputFormat::Human;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--output" | "--json" | "-j" => {
                if args[i] == "--json" || args[i] == "-j" {
                    output_format = OutputFormat::Json;
                    i += 1;
                } else {
                    let value = parse_required_arg(args, &mut i, "--output")?;
                    output_format = parse_output_format(&value)?;
                }
            }
            "--pretty" => {
                output_format = OutputFormat::Pretty;
                i += 1;
            }
            _ => i += 1,
        }
    }

    Ok(Command::Catalog { output_format })
}

pub fn parse_catalog_describe_args(args: &[String]) -> Result<Command> {
    let mut name: Option<String> = None;
    let mut output_format = OutputFormat::Human;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--output" | "--json" | "-j" => {
                if args[i] == "--json" || args[i] == "-j" {
                    output_format = OutputFormat::Json;
                    i += 1;
                } else {
                    let value = parse_required_arg(args, &mut i, "--output")?;
                    output_format = parse_output_format(&value)?;
                }
            }
            "--pretty" => {
                output_format = OutputFormat::Pretty;
                i += 1;
            }
            _ if !args[i].starts_with('-') => {
                name = Some(args[i].clone());
                i += 1;
            }
            _ => i += 1,
        }
    }

    let name = name.ok_or_else(|| {
        anyhow::anyhow!(
            "catalog describe requires a project name: magellan catalog describe <name>"
        )
    })?;

    Ok(Command::CatalogDescribe {
        name,
        output_format,
    })
}
