//! CLI front-end for the #1733 per-role floor selector.
//!
//! Reads a candidate catalog JSONL (metadata only -- NO 25GB blobs) and emits the
//! cheapest-first `source:asset_id` subset that just meets every editor role floor, as JSON to
//! stdout. It is the SINGLE source of truth the Python asset intake calls for `--floor` mode:
//! the intake never re-derives roles, it only consumes this selection. Any failure (missing
//! catalog, malformed line, unknown `--floor` role, or floors the catalog cannot satisfy) is a
//! LOUD structured error on stderr with a non-zero exit -- never a silent partial answer.
//!
//! Usage:
//!   litd-editor-asset-floor-select --catalog <path.jsonl> [--floor <role>=<count> ...]
//!
//! With no `--floor` overrides the default editor floors are used (architecture>=500,
//! character>=100, terrain_surface>=250, foliage>=100, vfx_light>=40, grave_ruin>=40,
//! audio>=100). Passing `--floor` overrides REPLACES the goal set with exactly the named roles.

use std::{fs, path::PathBuf, process::ExitCode};

use litd_editor::asset_floor_selection::{parse_catalog, parse_floor_overrides, select_floor};

fn main() -> ExitCode {
    match run() {
        Ok(json) => {
            println!("{json}");
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("ASSET_FLOOR_SELECT_FAILED: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<String, String> {
    let args = parse_args(std::env::args().skip(1).collect())?;
    let text = fs::read_to_string(&args.catalog)
        .map_err(|error| format!("failed to read catalog {}: {error}", args.catalog.display()))?;
    let entries = parse_catalog(&text).map_err(|error| error.to_string())?;
    let floors = parse_floor_overrides(&args.floor_overrides).map_err(|error| error.to_string())?;
    let selection = select_floor(&entries, &floors).map_err(|error| error.to_string())?;
    serde_json::to_string_pretty(&selection)
        .map_err(|error| format!("failed to serialize selection: {error}"))
}

struct Args {
    catalog: PathBuf,
    floor_overrides: Vec<String>,
}

fn parse_args(argv: Vec<String>) -> Result<Args, String> {
    let mut catalog: Option<PathBuf> = None;
    let mut floor_overrides = Vec::new();
    let mut iter = argv.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--catalog" => {
                let value = iter
                    .next()
                    .ok_or_else(|| "--catalog requires a path argument".to_string())?;
                catalog = Some(PathBuf::from(value));
            }
            "--floor" => {
                let value = iter
                    .next()
                    .ok_or_else(|| "--floor requires a role=count argument".to_string())?;
                floor_overrides.push(value);
            }
            other => {
                return Err(format!(
                    "unknown argument {other:?}; usage: --catalog <path.jsonl> [--floor <role>=<count> ...]"
                ));
            }
        }
    }
    let catalog = catalog.ok_or_else(|| "--catalog <path.jsonl> is required".to_string())?;
    Ok(Args {
        catalog,
        floor_overrides,
    })
}
