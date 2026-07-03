use std::{
    fs::{self, File},
    io::{ErrorKind, Read},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoMetadata {
    pub raw:              Value,
    pub duration_seconds: Option<f64>,
    pub video_streams:    usize,
    pub audio_streams:    usize,
}

#[derive(Debug, Clone)]
pub struct FrameScanConfig {
    pub fps:    f64,
    pub width:  usize,
    pub height: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct FrameSample {
    pub sample_index:      usize,
    pub timestamp_ms:      u64,
    pub mean_luma:         f64,
    pub mad_from_previous: Option<f64>,
}

pub fn probe_video(video: &Path) -> Result<VideoMetadata> {
    let output = Command::new("ffprobe")
        .args(["-v", "error", "-show_format", "-show_streams", "-of", "json"])
        .arg(video)
        .output()
        .with_context(|| format!("failed to start ffprobe for {}", video.display()))?;

    if !output.status.success() {
        bail!("ffprobe failed: {}", String::from_utf8_lossy(&output.stderr));
    }

    let raw: Value =
        serde_json::from_slice(&output.stdout).context("ffprobe emitted invalid JSON")?;
    let duration_seconds = raw
        .get("format")
        .and_then(|format| format.get("duration"))
        .and_then(Value::as_str)
        .and_then(|duration| duration.parse::<f64>().ok());
    let streams: &[Value] = raw.get("streams").and_then(Value::as_array).map_or(&[], Vec::as_slice);
    let video_streams = streams_with_type(streams, "video");
    let audio_streams = streams_with_type(streams, "audio");

    Ok(VideoMetadata { raw, duration_seconds, video_streams, audio_streams })
}

pub fn scan_frames(video: &Path, config: &FrameScanConfig) -> Result<Vec<FrameSample>> {
    validate_scan_config(config)?;

    let frame_size =
        config.width.checked_mul(config.height).context("analysis frame dimensions overflowed")?;
    let filter = format!(
        "fps={},scale={}:{}:force_original_aspect_ratio=decrease,pad={}:{}:-1:-1,format=gray",
        config.fps, config.width, config.height, config.width, config.height
    );

    let mut child = Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-i"])
        .arg(video)
        .args(["-vf", &filter, "-f", "rawvideo", "pipe:1"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("failed to start ffmpeg for {}", video.display()))?;

    let mut stdout = child.stdout.take().context("ffmpeg stdout was not captured")?;
    let mut buffer = vec![0_u8; frame_size];
    let mut previous: Option<Vec<u8>> = None;
    let mut samples = Vec::new();
    let mut sample_index = 0_usize;

    while read_frame(&mut stdout, &mut buffer)? {
        let mean_luma = mean_luma(&buffer);
        let mad_from_previous =
            previous.as_deref().map(|previous| mean_absolute_difference(previous, &buffer));
        let timestamp_ms = timestamp_ms(sample_index, config.fps);

        samples.push(FrameSample { sample_index, timestamp_ms, mean_luma, mad_from_previous });
        previous = Some(buffer.clone());
        sample_index = sample_index.saturating_add(1);
    }

    let output = child.wait_with_output().context("failed to wait for ffmpeg frame scan")?;
    if !output.status.success() {
        bail!("ffmpeg frame scan failed: {}", String::from_utf8_lossy(&output.stderr));
    }

    Ok(samples)
}

pub fn split_video(video: &Path, out: &Path, segment_seconds: u64) -> Result<()> {
    if segment_seconds == 0 {
        bail!("segment seconds must be greater than zero");
    }

    fs::create_dir_all(out).with_context(|| format!("failed to create {}", out.display()))?;
    let pattern = out.join("part_%03d.mp4");
    let status = Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-y", "-i"])
        .arg(video)
        .args(["-map", "0", "-c", "copy", "-f", "segment", "-segment_time"])
        .arg(segment_seconds.to_string())
        .args(["-reset_timestamps", "1"])
        .arg(&pattern)
        .status()
        .with_context(|| format!("failed to start ffmpeg split for {}", video.display()))?;

    if !status.success() {
        bail!("ffmpeg failed to split {}", video.display());
    }

    Ok(())
}

pub fn extract_frame(video: &Path, timestamp_ms: u64, output_path: &Path) -> Result<()> {
    let timestamp = format_seconds(timestamp_ms);
    let status = Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-y", "-ss", &timestamp, "-i"])
        .arg(video)
        .args(["-frames:v", "1", "-q:v", "2"])
        .arg(output_path)
        .status()
        .with_context(|| {
            format!("failed to start ffmpeg extraction for {}", output_path.display())
        })?;

    if !status.success() {
        bail!("ffmpeg failed to extract frame at {timestamp}s to {}", output_path.display());
    }

    Ok(())
}

pub fn ensure_parent_dir(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    Ok(())
}

pub fn create_file(path: &Path) -> Result<File> {
    ensure_parent_dir(path)?;
    File::create(path).with_context(|| format!("failed to create {}", path.display()))
}

fn streams_with_type(streams: &[Value], codec_type: &str) -> usize {
    streams
        .iter()
        .filter(|stream| stream.get("codec_type").and_then(Value::as_str) == Some(codec_type))
        .count()
}

fn validate_scan_config(config: &FrameScanConfig) -> Result<()> {
    if config.fps <= 0.0 {
        bail!("analysis fps must be greater than zero");
    }
    if config.width == 0 || config.height == 0 {
        bail!("analysis dimensions must be greater than zero");
    }
    Ok(())
}

fn read_frame(reader: &mut impl Read, buffer: &mut [u8]) -> Result<bool> {
    let mut filled = 0_usize;
    while filled < buffer.len() {
        match reader.read(&mut buffer[filled..]) {
            Ok(0) if filled == 0 => return Ok(false),
            Ok(0) => bail!("ffmpeg ended mid-frame"),
            Ok(read) => filled = filled.saturating_add(read),
            Err(error) if error.kind() == ErrorKind::Interrupted => {}
            Err(error) => return Err(error).context("failed to read ffmpeg raw frame"),
        }
    }
    Ok(true)
}

#[allow(clippy::cast_precision_loss)]
fn mean_luma(buffer: &[u8]) -> f64 {
    let total: u64 = buffer.iter().map(|pixel| u64::from(*pixel)).sum();
    total as f64 / buffer.len() as f64
}

#[allow(clippy::cast_precision_loss)]
fn mean_absolute_difference(previous: &[u8], current: &[u8]) -> f64 {
    let total: u64 =
        previous.iter().zip(current).map(|(left, right)| u64::from(left.abs_diff(*right))).sum();
    total as f64 / previous.len() as f64
}

#[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss, clippy::cast_sign_loss)]
fn timestamp_ms(sample_index: usize, fps: f64) -> u64 {
    ((sample_index as f64 / fps) * 1000.0).round() as u64
}

#[allow(clippy::cast_precision_loss)]
fn format_seconds(timestamp_ms: u64) -> String {
    format!("{:.3}", timestamp_ms as f64 / 1000.0)
}

pub fn default_output_dir(video: &Path) -> PathBuf {
    let stem = video.file_stem().and_then(|stem| stem.to_str()).unwrap_or("video");
    video.with_file_name(format!("{stem}.frameflame"))
}
