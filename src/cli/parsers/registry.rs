use crate::cli::Command;
use anyhow::Result;
use magellan::OutputFormat;
use std::path::PathBuf;

use crate::cli::parsers::*;

// ============================================================================
// Command Parsers - Individual command parsing functions
// ============================================================================

// ============================================================================
// Registry Parsers
// ============================================================================

pub fn parse_registry_scan_args(args: &[String]) -> Result<Command> {
    let mut root: Option<PathBuf> = None;
    let mut output_format = OutputFormat::Human;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--root" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--root requires an argument"));
                }
                root = Some(PathBuf::from(&args[i + 1]));
                i += 2;
            }
            "--output" => {
                let value = parse_required_arg(args, &mut i, "--output")?;
                output_format = parse_output_format(&value)?;
            }
            _ => i += 1,
        }
    }

    let root = root.unwrap_or_else(|| PathBuf::from("."));

    Ok(Command::RegistryScan {
        root,
        output_format,
    })
}

pub fn parse_registry_list_args(args: &[String]) -> Result<Command> {
    let mut root: Option<PathBuf> = None;
    let mut output_format = OutputFormat::Human;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--root" => {
                if i + 1 >= args.len() {
                    return Err(anyhow::anyhow!("--root requires an argument"));
                }
                root = Some(PathBuf::from(&args[i + 1]));
                i += 2;
            }
            "--output" => {
                let value = parse_required_arg(args, &mut i, "--output")?;
                output_format = parse_output_format(&value)?;
            }
            _ => i += 1,
        }
    }

    let root = root.unwrap_or_else(|| PathBuf::from("."));

    Ok(Command::RegistryList {
        root,
        output_format,
    })
}
