use std::{
    io,
    process::{Command, Output},
};

fn flameframe(args: &[&str]) -> io::Result<Output> {
    Command::new(env!("CARGO_BIN_EXE_flameframe")).args(args).output()
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn short_help_flag_prints_help() -> io::Result<()> {
    let output = flameframe(&["-h"])?;

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let stdout = stdout(&output);
    assert!(stdout.contains("Usage: flameframe"), "stdout: {stdout}");
    assert!(stdout.contains("<COMMAND>"), "stdout: {stdout}");
    assert!(stdout.contains("upgrade"), "stdout: {stdout}");
    Ok(())
}

#[test]
fn short_version_flag_prints_version() -> io::Result<()> {
    let output = flameframe(&["-v"])?;

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let stdout = stdout(&output);
    assert!(stdout.contains(env!("CARGO_PKG_VERSION")), "stdout: {stdout}");
    Ok(())
}

#[test]
fn long_version_flag_prints_version() -> io::Result<()> {
    let output = flameframe(&["--version"])?;

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let stdout = stdout(&output);
    assert!(stdout.contains(env!("CARGO_PKG_VERSION")), "stdout: {stdout}");
    Ok(())
}

#[test]
fn upgrade_dry_run_does_not_execute_network_install() -> io::Result<()> {
    let output = flameframe(&["upgrade", "--dry-run", "--version", "v0.1.0"])?;

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let stdout = stdout(&output);
    assert!(stdout.contains("Would run:"), "stdout: {stdout}");
    assert!(stdout.contains("v0.1.0"), "stdout: {stdout}");
    Ok(())
}

#[test]
fn upgrade_rejects_unsafe_repo_values() -> io::Result<()> {
    let output = flameframe(&["upgrade", "--dry-run", "--repo", "bad repo/name"])?;

    assert!(!output.status.success(), "stdout: {}", stdout(&output));
    let stderr = stderr(&output);
    assert!(stderr.contains("GitHub-safe characters"), "stderr: {stderr}");
    Ok(())
}

#[test]
fn manual_documents_primary_workflows_and_upgrade() -> io::Result<()> {
    let output = flameframe(&["man"])?;

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let stdout = stdout(&output);
    assert!(stdout.contains("YouTube URL"), "stdout: {stdout}");
    assert!(stdout.contains("Local video file"), "stdout: {stdout}");
    assert!(stdout.contains("flameframe upgrade"), "stdout: {stdout}");
    Ok(())
}
