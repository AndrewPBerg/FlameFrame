use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio, id},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, bail};
use serde::Serialize;

use crate::workspace;

#[derive(Debug, Clone)]
pub struct DownloadConfig {
    pub output_root:     PathBuf,
    pub output_dir:      Option<PathBuf>,
    pub timeout_seconds: u64,
    pub max_height:      u16,
    pub captions:        bool,
    pub sub_langs:       String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DownloadResult {
    pub video_path: PathBuf,
    pub directory:  PathBuf,
    pub sidecars:   Vec<PathBuf>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UrlMetadata {
    pub id:               Option<String>,
    pub title:            Option<String>,
    pub uploader:         Option<String>,
    pub duration_seconds: Option<f64>,
    pub webpage_url:      Option<String>,
    pub extractor:        Option<String>,
}

pub fn is_probably_url(input: &str) -> bool {
    input.starts_with("http://") || input.starts_with("https://")
}

pub fn inspect_url(url: &str, timeout_seconds: u64) -> Result<UrlMetadata> {
    validate_url(url)?;

    let output = run_output_with_timeout(
        Command::new("yt-dlp")
            .args([
                "--no-playlist",
                "--skip-download",
                "--print",
                "%(id)s",
                "--print",
                "%(title)s",
                "--print",
                "%(uploader)s",
                "--print",
                "%(duration)s",
                "--print",
                "%(webpage_url)s",
                "--print",
                "%(extractor)s",
            ])
            .arg(url),
        Duration::from_secs(timeout_seconds),
        "yt-dlp metadata inspect",
    )?;

    let text = String::from_utf8(output).context("yt-dlp emitted non-UTF-8 metadata")?;
    let mut lines = text.lines();
    Ok(UrlMetadata {
        id:               clean_printed_field(lines.next()),
        title:            clean_printed_field(lines.next()),
        uploader:         clean_printed_field(lines.next()),
        duration_seconds: clean_printed_field(lines.next())
            .and_then(|duration| duration.parse().ok()),
        webpage_url:      clean_printed_field(lines.next()),
        extractor:        clean_printed_field(lines.next()),
    })
}

pub fn download_url(url: &str, config: &DownloadConfig) -> Result<DownloadResult> {
    validate_url(url)?;
    let job_dir =
        config.output_dir.clone().unwrap_or_else(|| config.output_root.join(job_dir_name()));
    workspace::ensure_gitignore_for(&job_dir)?;
    fs::create_dir_all(&job_dir)
        .with_context(|| format!("failed to create {}", job_dir.display()))?;

    let format = format_selector(config.max_height);
    let mut command = Command::new("yt-dlp");
    command
        .args([
            "--no-playlist",
            "--restrict-filenames",
            "--merge-output-format",
            "mp4",
            "--format",
            &format,
            "--paths",
        ])
        .arg(&job_dir)
        .args(["--output", "video.%(ext)s"]);

    if config.captions {
        command.args([
            "--write-subs",
            "--write-auto-subs",
            "--sub-langs",
            &config.sub_langs,
            "--sub-format",
            "srt/vtt/best",
        ]);
    }

    command.arg(url);
    run_status_with_timeout(
        &mut command,
        Duration::from_secs(config.timeout_seconds),
        "yt-dlp download",
    )?;

    let video_path = find_downloaded_video(&job_dir)?;
    let sidecars = find_sidecars(&job_dir, &video_path)?;
    Ok(DownloadResult { video_path, directory: job_dir, sidecars })
}

fn validate_url(url: &str) -> Result<()> {
    if is_probably_url(url) { Ok(()) } else { bail!("expected http(s) URL, got {url}") }
}

fn run_status_with_timeout(command: &mut Command, timeout: Duration, label: &str) -> Result<()> {
    let mut child = command
        .spawn()
        .with_context(|| format!("failed to start {label}; is yt-dlp installed and on PATH?"))?;

    let status = wait_with_timeout(&mut child, timeout, label)?;
    if !status.success() {
        bail!("{label} failed with status {status}");
    }

    Ok(())
}

fn run_output_with_timeout(
    command: &mut Command,
    timeout: Duration,
    label: &str,
) -> Result<Vec<u8>> {
    let mut child =
        command.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().with_context(|| {
            format!("failed to start {label}; is yt-dlp installed and on PATH?")
        })?;

    let status = wait_with_timeout(&mut child, timeout, label)?;
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    if let Some(mut pipe) = child.stdout.take() {
        pipe.read_to_end(&mut stdout).with_context(|| format!("failed to read {label} stdout"))?;
    }
    if let Some(mut pipe) = child.stderr.take() {
        pipe.read_to_end(&mut stderr).with_context(|| format!("failed to read {label} stderr"))?;
    }

    if !status.success() {
        bail!("{label} failed: {}", String::from_utf8_lossy(&stderr));
    }

    Ok(stdout)
}

fn wait_with_timeout(child: &mut Child, timeout: Duration, label: &str) -> Result<ExitStatus> {
    let started = Instant::now();
    loop {
        if let Some(status) = child.try_wait().with_context(|| format!("failed to poll {label}"))? {
            return Ok(status);
        }

        if started.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            bail!("{label} timed out after {} seconds", timeout.as_secs());
        }

        thread::sleep(Duration::from_millis(250));
    }
}

fn find_downloaded_video(job_dir: &Path) -> Result<PathBuf> {
    let mut candidates = Vec::new();
    for entry in
        fs::read_dir(job_dir).with_context(|| format!("failed to read {}", job_dir.display()))?
    {
        let path = entry?.path();
        if path.is_file() && is_video_file(&path) {
            candidates.push(path);
        }
    }

    candidates.sort();
    candidates.into_iter().next().with_context(|| {
        format!("yt-dlp completed but no downloaded video was found in {}", job_dir.display())
    })
}

fn find_sidecars(job_dir: &Path, video_path: &Path) -> Result<Vec<PathBuf>> {
    let mut sidecars = Vec::new();
    for entry in
        fs::read_dir(job_dir).with_context(|| format!("failed to read {}", job_dir.display()))?
    {
        let path = entry?.path();
        if path.is_file() && path != video_path && !is_temporary_download(&path) {
            sidecars.push(path);
        }
    }
    sidecars.sort();
    Ok(sidecars)
}

fn is_video_file(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| matches!(extension, "mp4" | "mkv" | "mov" | "webm" | "m4v"))
}

fn is_temporary_download(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| matches!(extension, "part" | "ytdl" | "temp"))
}

fn format_selector(max_height: u16) -> String {
    format!(
        "bestvideo[height<={max_height}][ext=mp4]+bestaudio[ext=m4a]/bestvideo[height<={max_height}]+bestaudio/best[height<={max_height}][ext=mp4]/best[height<={max_height}]"
    )
}

fn job_dir_name() -> String {
    let seconds =
        SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |duration| duration.as_secs());
    format!("{seconds}-{}", id())
}

fn clean_printed_field(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty() && *value != "NA" && *value != "None")
        .map(ToOwned::to_owned)
}
