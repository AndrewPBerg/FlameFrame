use std::{
    env, fs,
    io::{self, Write},
    process::{self, Command, Output, Stdio},
    time::{SystemTime, UNIX_EPOCH},
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
fn url_download_defaults_to_project_flameframe_directory() -> io::Result<()> {
    let output = flameframe(&["download", "--help"])?;

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    let stdout = stdout(&output);
    assert!(stdout.contains("[default: .flameframe]"), "stdout: {stdout}");
    assert!(!stdout.contains("data/downloads"), "stdout: {stdout}");
    Ok(())
}

#[test]
fn uninstall_requires_confirmation() -> io::Result<()> {
    let mut child = Command::new(env!("CARGO_BIN_EXE_flameframe"))
        .arg("uninstall")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;
    child.stdin.take().ok_or_else(|| io::Error::other("stdin is not piped"))?.write_all(b"n\n")?;
    let output = child.wait_with_output()?;

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert!(stdout(&output).contains("Uninstall cancelled."));
    Ok(())
}

#[cfg(not(windows))]
#[test]
fn uninstall_removes_the_binary_currently_running() -> io::Result<()> {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH).map_err(io::Error::other)?.as_nanos();
    let copy = env::temp_dir().join(format!("flameframe-uninstall-test-{}-{nonce}", process::id()));
    fs::copy(env!("CARGO_BIN_EXE_flameframe"), &copy)?;

    let mut child = Command::new(&copy)
        .arg("uninstall")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;
    child.stdin.take().ok_or_else(|| io::Error::other("stdin is not piped"))?.write_all(b"y\n")?;
    let output = child.wait_with_output()?;

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert!(stdout(&output).contains("Removed:"));
    assert!(!copy.exists(), "binary was not removed: {}", copy.display());
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
    assert!(stdout.contains("flameframe uninstall"), "stdout: {stdout}");
    Ok(())
}
