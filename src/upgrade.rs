use std::{
    fmt::Arguments,
    io::{self, Write},
    process::Command,
};

use anyhow::{Context, Result, bail};

use crate::cli::UpgradeArgs;

pub fn run(args: &UpgradeArgs) -> Result<()> {
    validate_repo(&args.repo)?;
    validate_version(&args.version)?;

    if cfg!(windows) { run_windows(args) } else { run_unix(args) }
}

fn run_unix(args: &UpgradeArgs) -> Result<()> {
    let script = format!("https://raw.githubusercontent.com/{}/main/install.sh", args.repo);
    let command_text = format!("curl -fsSL {script} | sh");

    if args.dry_run {
        output_line(format_args!("Would run: {command_text}"))?;
        output_line(format_args!("FLAMEFRAME_REPO={}", args.repo))?;
        output_line(format_args!("FLAMEFRAME_VERSION={}", args.version))?;
        if let Some(install_dir) = &args.install_dir {
            output_line(format_args!("FLAMEFRAME_INSTALL_DIR={}", install_dir.display()))?;
        }
        return Ok(());
    }

    let mut command = Command::new("sh");
    command.arg("-c").arg(command_text);
    command.env("FLAMEFRAME_REPO", &args.repo).env("FLAMEFRAME_VERSION", &args.version);
    if let Some(install_dir) = &args.install_dir {
        command.env("FLAMEFRAME_INSTALL_DIR", install_dir);
    }

    run_status(&mut command, "flameframe upgrade")
}

fn run_windows(args: &UpgradeArgs) -> Result<()> {
    let script = format!("https://raw.githubusercontent.com/{}/main/install.ps1", args.repo);
    let mut command_text = format!(
        "$script = Join-Path $env:TEMP 'flameframe-install.ps1'; \
         Invoke-WebRequest -Uri '{}' -OutFile $script; \
         & $script -Version '{}' -Repo '{}'",
        escape_powershell_single_quoted(&script),
        escape_powershell_single_quoted(&args.version),
        escape_powershell_single_quoted(&args.repo),
    );

    if let Some(install_dir) = &args.install_dir {
        command_text.push_str(" -InstallDir '");
        command_text.push_str(&escape_powershell_single_quoted(&install_dir.display().to_string()));
        command_text.push('\'');
    }

    if args.dry_run {
        output_line(format_args!(
            "Would run: powershell -NoProfile -ExecutionPolicy Bypass -Command {command_text}"
        ))?;
        return Ok(());
    }

    let mut command = Command::new("powershell");
    command.args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", &command_text]);

    run_status(&mut command, "flameframe upgrade")
}

fn run_status(command: &mut Command, label: &str) -> Result<()> {
    let status = command.status().with_context(|| format!("failed to start {label}"))?;
    if status.success() { Ok(()) } else { bail!("{label} failed with status {status}") }
}

fn validate_repo(repo: &str) -> Result<()> {
    let mut parts = repo.split('/');
    let Some(owner) = parts.next() else { bail!("repo must be owner/name") };
    let Some(name) = parts.next() else { bail!("repo must be owner/name") };
    if parts.next().is_some() || !is_github_component(owner) || !is_github_component(name) {
        bail!("repo must be owner/name with GitHub-safe characters")
    }
    Ok(())
}

fn validate_version(version: &str) -> Result<()> {
    if version == "latest"
        || version
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-' | '+'))
    {
        Ok(())
    } else {
        bail!("version contains unsupported characters")
    }
}

fn is_github_component(value: &str) -> bool {
    !value.is_empty()
        && value.chars().all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-'))
}

fn escape_powershell_single_quoted(value: &str) -> String {
    value.replace('\'', "''")
}

fn output_line(args: Arguments<'_>) -> Result<()> {
    let mut stdout = io::stdout().lock();
    writeln!(stdout, "{args}").context("failed to write upgrade output")?;
    Ok(())
}
