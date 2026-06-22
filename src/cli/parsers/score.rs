use anyhow::Result;
use magellan::OutputFormat;

use crate::cli::parsers::*;
use crate::cli::Command;

// ============================================================================
// Score Parsers
// ============================================================================

pub fn parse_score_args(args: &[String]) -> Result<Command> {
    let mut db: Option<String> = None;
    let mut top = None;
    let mut min_score = None;
    let mut min_churn = None;
    let mut min_complexity = None;
    let mut min_lifetime = None;
    let mut output_format = OutputFormat::Human;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--db" | "-d" => {
                let value = parse_required_arg(args, &mut i, "--db")?;
                db = Some(value);
            }
            "--top" | "-t" => {
                let value = parse_required_arg(args, &mut i, "--top")?;
                top = Some(value.parse::<usize>()?);
            }
            "--min-score" => {
                let value = parse_required_arg(args, &mut i, "--min-score")?;
                min_score = Some(value.parse::<f64>()?);
            }
            "--min-churn" => {
                let value = parse_required_arg(args, &mut i, "--min-churn")?;
                min_churn = Some(value.parse::<i64>()?);
            }
            "--min-complexity" => {
                let value = parse_required_arg(args, &mut i, "--min-complexity")?;
                min_complexity = Some(value.parse::<i64>()?);
            }
            "--min-lifetime" => {
                let value = parse_required_arg(args, &mut i, "--min-lifetime")?;
                min_lifetime = Some(value.parse::<i64>()?);
            }
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

    let db = db.ok_or_else(|| anyhow::anyhow!("score command requires --db <path>"))?;

    Ok(Command::Score {
        db: db.into(),
        top,
        min_score,
        min_churn,
        min_complexity,
        min_lifetime,
        output: Some(output_format),
    })
}
