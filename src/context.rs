use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::ffmpeg;

#[derive(Debug, Clone)]
pub struct ContextConfig {
    pub dir:            PathBuf,
    pub captions:       Option<PathBuf>,
    pub video:          Option<PathBuf>,
    pub pack:           Option<PathBuf>,
    pub window_seconds: u64,
}

#[derive(Debug, Clone)]
struct CaptionEntry {
    start_ms: u64,
    end_ms:   u64,
    text:     String,
}

#[derive(Debug, Deserialize)]
struct FrameRecord {
    frame_id:     String,
    timestamp_ms: u64,
    image_path:   PathBuf,
}

pub fn build(config: &ContextConfig) -> Result<Vec<PathBuf>> {
    if config.window_seconds == 0 {
        bail!("window seconds must be greater than zero");
    }

    let video = resolve_video(config)?;
    let captions = resolve_captions(config)?;
    let pack = resolve_pack(config);
    let entries = captions.as_deref().map(parse_srt).transpose()?.unwrap_or_default();

    let caption_context = captions.as_ref().map(|_| config.dir.join("video.context.md"));
    let inspect_context = config.dir.join("inspect.context.md");
    let visual_context = config.dir.join("inspect.visual.context.md");

    if let (Some(caption_context), Some(captions)) = (&caption_context, &captions) {
        write_caption_context(caption_context, captions, &entries, config.window_seconds)?;
    }
    write_inspect_context(
        &inspect_context,
        &config.dir,
        &video,
        caption_context.as_deref(),
        pack.as_deref(),
    )?;

    let mut written = caption_context.into_iter().collect::<Vec<_>>();
    written.push(inspect_context);
    if let Some(pack) = pack {
        write_visual_context(&visual_context, &config.dir, &video, &pack, &entries)?;
        written.push(visual_context);
    }

    Ok(written)
}

fn resolve_video(config: &ContextConfig) -> Result<PathBuf> {
    let video = config.video.clone().unwrap_or_else(|| config.dir.join("video.mp4"));
    if !video.is_file() {
        bail!("video does not exist: {}", video.display());
    }
    Ok(video)
}

fn resolve_captions(config: &ContextConfig) -> Result<Option<PathBuf>> {
    if let Some(captions) = &config.captions {
        if captions.is_file() {
            return Ok(Some(captions.clone()));
        }
        bail!("captions do not exist: {}", captions.display());
    }

    let default = config.dir.join("video.en.srt");
    if default.is_file() {
        return Ok(Some(default));
    }

    let mut candidates = Vec::new();
    for entry in fs::read_dir(&config.dir)
        .with_context(|| format!("failed to read {}", config.dir.display()))?
    {
        let path = entry?.path();
        if path.extension().and_then(|extension| extension.to_str()) == Some("srt") {
            candidates.push(path);
        }
    }
    candidates.sort();
    Ok(candidates.into_iter().next())
}

fn resolve_pack(config: &ContextConfig) -> Option<PathBuf> {
    let pack = config.pack.clone().unwrap_or_else(|| config.dir.join("video.frameflame"));
    pack.is_dir().then_some(pack)
}

fn parse_srt(path: &Path) -> Result<Vec<CaptionEntry>> {
    let text =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let mut entries = Vec::new();

    for block in text.split("\n\n") {
        let lines: Vec<_> = block.lines().map(str::trim).filter(|line| !line.is_empty()).collect();
        if lines.len() < 3 || !lines[1].contains("-->") {
            continue;
        }

        let (start, end) =
            lines[1].split_once("-->").context("invalid subtitle timestamp range")?;
        let text = lines[2..].join(" ");
        entries.push(CaptionEntry {
            start_ms: parse_srt_timestamp(start.trim())?,
            end_ms: parse_srt_timestamp(end.trim())?,
            text,
        });
    }

    if entries.is_empty() {
        bail!("no captions parsed from {}", path.display());
    }

    Ok(entries)
}

fn parse_srt_timestamp(value: &str) -> Result<u64> {
    let (time, millis) = value.split_once(',').context("invalid subtitle timestamp")?;
    let parts: Vec<_> = time.split(':').collect();
    if parts.len() != 3 {
        bail!("invalid subtitle timestamp: {value}");
    }

    let hours = parts[0].parse::<u64>().context("invalid subtitle hours")?;
    let minutes = parts[1].parse::<u64>().context("invalid subtitle minutes")?;
    let seconds = parts[2].parse::<u64>().context("invalid subtitle seconds")?;
    let millis = millis.parse::<u64>().context("invalid subtitle milliseconds")?;
    Ok((((hours * 60) + minutes) * 60 + seconds) * 1000 + millis)
}

fn write_caption_context(
    path: &Path,
    captions: &Path,
    entries: &[CaptionEntry],
    window_seconds: u64,
) -> Result<()> {
    let window_ms = window_seconds.checked_mul(1000).context("window seconds overflowed")?;
    let mut lines = vec![
        "# Video Caption Context".to_string(),
        String::new(),
        format!("- source: `{}`", captions.display()),
        format!("- captions: {}", entries.len()),
        format!("- grouping: {window_seconds} second windows"),
        String::new(),
    ];

    let mut current_bucket = entries[0].start_ms / window_ms;
    let mut buffer = Vec::new();
    for entry in entries {
        let bucket = entry.start_ms / window_ms;
        if bucket != current_bucket {
            push_caption_window(&mut lines, current_bucket, window_ms, &buffer);
            buffer.clear();
            current_bucket = bucket;
        }
        buffer.push(format!("[{}] {}", format_timestamp(entry.start_ms), entry.text));
    }
    push_caption_window(&mut lines, current_bucket, window_ms, &buffer);

    fs::write(path, lines.join("\n")).with_context(|| format!("failed to write {}", path.display()))
}

fn push_caption_window(lines: &mut Vec<String>, bucket: u64, window_ms: u64, buffer: &[String]) {
    lines.push(format!(
        "## {}–{}",
        format_timestamp(bucket.saturating_mul(window_ms)),
        format_timestamp(bucket.saturating_add(1).saturating_mul(window_ms))
    ));
    lines.push(String::new());
    lines.push(buffer.join(" "));
    lines.push(String::new());
}

fn write_inspect_context(
    path: &Path,
    dir: &Path,
    video: &Path,
    caption_context: Option<&Path>,
    pack: Option<&Path>,
) -> Result<()> {
    let metadata = ffmpeg::probe_video(video)?;
    let mut lines = vec![
        "# FlameFrame Inspect Context".to_string(),
        String::new(),
        format!("- source video: `{}`", video.display()),
        format!(
            "- duration: {}",
            metadata
                .duration_seconds
                .map_or_else(|| "unknown".to_string(), |duration| format!("{duration:.2}s"))
        ),
        format!("- video streams: {}", metadata.video_streams),
        format!("- audio streams: {}", metadata.audio_streams),
        caption_context.map_or_else(
            || "- caption context: none found".to_string(),
            |caption_context| {
                format!("- caption context: `{}`", display_relative(dir, caption_context))
            },
        ),
    ];

    if let Some(pack) = pack {
        lines.push(format!("- FlameFrame pack: `{}`", display_relative(dir, pack)));
    }

    lines.push(String::new());
    lines.push("## Segments".to_string());
    lines.push(String::new());
    for segment in segment_files(dir)? {
        let segment_metadata = ffmpeg::probe_video(&segment)?;
        let duration = segment_metadata
            .duration_seconds
            .map_or_else(|| "unknown".to_string(), |duration| format!("{duration:.2}s"));
        lines.push(format!("- `{}` — {duration}", display_relative(dir, &segment)));
    }

    fs::write(path, lines.join("\n")).with_context(|| format!("failed to write {}", path.display()))
}

fn segment_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let segments_dir = dir.join("segments");
    if !segments_dir.is_dir() {
        return Ok(Vec::new());
    }

    let mut segments = Vec::new();
    for entry in fs::read_dir(&segments_dir)
        .with_context(|| format!("failed to read {}", segments_dir.display()))?
    {
        let path = entry?.path();
        if path.extension().and_then(|extension| extension.to_str()) == Some("mp4") {
            segments.push(path);
        }
    }
    segments.sort();
    Ok(segments)
}

fn write_visual_context(
    path: &Path,
    dir: &Path,
    video: &Path,
    pack: &Path,
    entries: &[CaptionEntry],
) -> Result<()> {
    let records = parse_frame_records(&pack.join("frames.jsonl"))?;
    let mut lines = vec![
        "# Visual Inspect Context".to_string(),
        String::new(),
        "- transcript-first flow: read `video.context.md` when available, pick interesting timestamps, then inspect selected frames near those timestamps.".to_string(),
        "- image links are relative to this directory.".to_string(),
        "- if a still is insufficient, run `flameframe zoom <video> --at <HH:MM:SS> --window 12 --fps 2 --out <dir>/zooms/<timestamp>`.".to_string(),
        String::new(),
    ];

    for record in records {
        let image = pack.join(&record.image_path);
        lines.push(format!("## {} — {}", format_timestamp(record.timestamp_ms), record.frame_id));
        lines.push(String::new());
        lines.push(format!(
            "![{} at {}]({})",
            record.frame_id,
            format_timestamp(record.timestamp_ms),
            display_relative(dir, &image)
        ));
        lines.push(String::new());
        lines.push(caption_window_text(entries, record.timestamp_ms, 20_000));
        lines.push(String::new());
        let timestamp = format_timestamp(record.timestamp_ms);
        let zoom_dir = format!("zooms/{}", timestamp.replace(':', "-"));
        lines.push("Suggested zoom:".to_string());
        lines.push(String::new());
        lines.push(format!(
            "```bash\nflameframe zoom {} --at {timestamp} --window 12 --fps 2 --out {}\n```",
            video.display(),
            dir.join(zoom_dir).display()
        ));
        lines.push(String::new());
    }

    fs::write(path, lines.join("\n")).with_context(|| format!("failed to write {}", path.display()))
}

fn parse_frame_records(path: &Path) -> Result<Vec<FrameRecord>> {
    let text =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    text.lines()
        .map(|line| serde_json::from_str(line).context("failed to parse frame record"))
        .collect()
}

fn caption_window_text(entries: &[CaptionEntry], center_ms: u64, radius_ms: u64) -> String {
    if entries.is_empty() {
        return "No captions were available for this video; use the still image first, then run the suggested zoom command if motion/detail matters.".to_string();
    }

    let start = center_ms.saturating_sub(radius_ms);
    let end = center_ms.saturating_add(radius_ms);
    entries
        .iter()
        .filter(|entry| entry.end_ms >= start && entry.start_ms <= end)
        .map(|entry| format!("[{}] {}", format_timestamp(entry.start_ms), entry.text))
        .collect::<Vec<_>>()
        .join(" ")
}

fn display_relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root).unwrap_or(path).display().to_string()
}

fn format_timestamp(timestamp_ms: u64) -> String {
    let total = timestamp_ms / 1000;
    format!("{:02}:{:02}:{:02}", total / 3600, (total % 3600) / 60, total % 60)
}
