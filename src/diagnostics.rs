use std::{
    env,
    ffi::OsString,
    fmt::Arguments,
    io::{self, Write},
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ToolStatus {
    pub name:      &'static str,
    pub available: bool,
    pub path:      Option<PathBuf>,
    pub version:   Option<String>,
    pub warning:   Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Diagnostics {
    pub tools:    Vec<ToolStatus>,
    pub warnings: Vec<String>,
}

pub fn run_doctor(json: bool) -> Result<()> {
    let diagnostics = collect();

    if json {
        output_line(format_args!("{}", serde_json::to_string_pretty(&diagnostics)?))?;
        return Ok(());
    }

    for tool in &diagnostics.tools {
        if tool.available {
            let path = tool
                .path
                .as_ref()
                .map_or_else(|| "unknown path".to_string(), |path| path.display().to_string());
            let version = tool.version.as_deref().unwrap_or("version unavailable");
            output_line(format_args!("ok: {} at {path} ({version})", tool.name))?;
        } else {
            let warning = tool.warning.as_deref().unwrap_or("missing required tool");
            output_line(format_args!("warn: {}: {warning}", tool.name))?;
        }
    }

    Ok(())
}

pub fn collect() -> Diagnostics {
    collect_tools(&["ffprobe", "ffmpeg", "yt-dlp"])
}

pub fn require_ffmpeg_tools() -> Result<Diagnostics> {
    require_tools(&["ffprobe", "ffmpeg"])
}

pub fn require_ytdlp() -> Result<Diagnostics> {
    require_tools(&["yt-dlp"])
}

fn require_tools(names: &[&'static str]) -> Result<Diagnostics> {
    let diagnostics = collect_tools(names);
    if diagnostics.warnings.is_empty() {
        return Ok(diagnostics);
    }

    anyhow::bail!("missing required local tools: {}", diagnostics.warnings.join("; "));
}

fn collect_tools(names: &[&'static str]) -> Diagnostics {
    let tools: Vec<_> = names.iter().copied().map(check_tool).collect();
    let warnings = tools.iter().filter_map(|tool| tool.warning.clone()).collect();

    Diagnostics { tools, warnings }
}

fn check_tool(name: &'static str) -> ToolStatus {
    let path = find_on_path(name);

    let output = Command::new(name).arg(version_arg(name)).output();
    match output {
        Ok(output) if output.status.success() => {
            let version = first_line(&output.stdout).or_else(|| first_line(&output.stderr));
            ToolStatus { name, available: true, path, version, warning: None }
        }
        Ok(output) => ToolStatus {
            name,
            available: false,
            path,
            version: first_line(&output.stderr),
            warning: Some(format!("{name} exists but returned status {}", output.status)),
        },
        Err(error) => ToolStatus {
            name,
            available: false,
            path,
            version: None,
            warning: Some(format!("{name} not runnable from PATH: {error}")),
        },
    }
}

fn version_arg(name: &str) -> &'static str {
    if name == "yt-dlp" { "--version" } else { "-version" }
}

fn first_line(bytes: &[u8]) -> Option<String> {
    String::from_utf8_lossy(bytes)
        .lines()
        .next()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
}

fn find_on_path(binary: &str) -> Option<PathBuf> {
    let path_var = env::var_os("PATH")?;
    env::split_paths(&path_var).find_map(|dir| candidate_path(&dir, binary))
}

fn candidate_path(dir: &Path, binary: &str) -> Option<PathBuf> {
    let candidate = dir.join(OsString::from(binary));
    if candidate.is_file() { Some(candidate) } else { None }
}

fn output_line(args: Arguments<'_>) -> Result<()> {
    let mut stdout = io::stdout().lock();
    writeln!(stdout, "{args}").context("failed to write diagnostic output")?;
    Ok(())
}
