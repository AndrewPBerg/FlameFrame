use std::{
    env, fs,
    io::{self, BufRead, Write},
    path::Path,
};

#[cfg(windows)]
use anyhow::bail;
use anyhow::{Context, Result};

pub fn run() -> Result<()> {
    let executable =
        env::current_exe().context("failed to determine the running FlameFrame binary")?;
    let mut stdout = io::stdout().lock();
    let mut stdin = io::stdin().lock();

    write!(
        stdout,
        "Remove the FlameFrame binary currently running at {}? [y/N] ",
        executable.display()
    )
    .context("failed to write uninstall confirmation")?;
    stdout.flush().context("failed to flush uninstall confirmation")?;

    let mut response = String::new();
    stdin.read_line(&mut response).context("failed to read uninstall confirmation")?;
    if !response.trim().eq_ignore_ascii_case("y") {
        writeln!(stdout, "Uninstall cancelled.")
            .context("failed to write uninstall cancellation")?;
        return Ok(());
    }

    remove_running_binary(&executable)?;
    writeln!(stdout, "Removed: {}", executable.display())
        .context("failed to write uninstall result")?;
    Ok(())
}

#[cfg(not(windows))]
fn remove_running_binary(executable: &Path) -> Result<()> {
    fs::remove_file(executable)
        .with_context(|| format!("failed to remove running binary {}", executable.display()))
}

#[cfg(windows)]
fn remove_running_binary(executable: &Path) -> Result<()> {
    use std::{
        fs::File,
        process::Command,
        time::{SystemTime, UNIX_EPOCH},
    };

    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock is before the Unix epoch")?
        .as_nanos();
    let script =
        env::temp_dir().join(format!("flameframe-uninstall-{}-{nonce}.cmd", std::process::id()));
    let mut file = File::create(&script)
        .with_context(|| format!("failed to create uninstall helper {}", script.display()))?;
    writeln!(file, "@echo off")?;
    writeln!(file, "timeout /t 1 /nobreak > nul")?;
    writeln!(file, "del /f /q \"{}\"", executable.display())?;
    writeln!(file, "del /f /q \"%~f0\"")?;
    drop(file);

    let status = Command::new("cmd")
        .args(["/C", "start", "\"\"", "/B", &script.display().to_string()])
        .status()
        .context("failed to start uninstall helper")?;
    if !status.success() {
        bail!("failed to start uninstall helper");
    }
    Ok(())
}
