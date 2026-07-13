use std::{
    cmp::max,
    fmt::Arguments,
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use serde::Serialize;

use crate::{
    cli::{
        ContextArgs, DownloadArgs, IngestArgs, InspectArgs, Mode, ProcessArgs, SplitArgs,
        VerifyArgs, ZoomArgs,
    },
    context::{self as context_builder, ContextConfig},
    diagnostics,
    ffmpeg::{self, FrameSample, FrameScanConfig, VideoMetadata},
    workspace,
    ytdlp::{self, DownloadConfig},
};

#[derive(Debug, Serialize)]
struct Manifest<'a> {
    source_input: &'a str,
    source_video: &'a Path,
    mode:         &'a str,
    budget:       usize,
    analysis_fps: f64,
    metadata:     &'a VideoMetadata,
}

#[derive(Debug, Serialize)]
struct FrameRecord<'a> {
    frame_id:          String,
    timestamp_ms:      u64,
    image_path:        &'a Path,
    mean_luma:         f64,
    mad_from_previous: Option<f64>,
    selection_reason:  &'a str,
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Serialize)]
struct Verification {
    dir:             PathBuf,
    ok:              bool,
    video:           bool,
    pack:            bool,
    manifest:        bool,
    frames_jsonl:    bool,
    selected_frames: usize,
    captions:        usize,
    caption_context: bool,
    inspect_context: bool,
    visual_context:  bool,
    segments:        usize,
    zoom_frames:     usize,
    errors:          Vec<String>,
}

pub fn process(args: &ProcessArgs) -> Result<()> {
    workspace::ensure_gitignore_for(&args.work_dir)?;
    fs::create_dir_all(&args.work_dir)
        .with_context(|| format!("failed to create {}", args.work_dir.display()))?;

    let cache = workspace::ProcessCache::for_input(&args.input, &process_cache_variant(args))?;
    let _lock = cache.lock()?;
    if cache.restore(&args.work_dir)? {
        output_line(format_args!("inspect cache: hit ({})", cache.display().display()))?;
        return finish_process(args, process_video_path(args));
    }

    output_line(format_args!("inspect cache: miss ({})", cache.display().display()))?;
    let video = process_uncached(args)?;
    finish_process(args, video)?;
    cache.store(&args.work_dir)?;
    output_line(format_args!("inspect cache: stored ({})", cache.display().display()))?;
    Ok(())
}

fn process_uncached(args: &ProcessArgs) -> Result<PathBuf> {
    let input_is_url = ytdlp::is_probably_url(&args.input);
    let local_video = if input_is_url {
        None
    } else {
        Some(copy_process_input_video(&args.input, &args.work_dir)?)
    };
    let ingest_input =
        local_video.as_ref().map_or_else(|| args.input.clone(), |path| path.display().to_string());
    let pack = args.work_dir.join("video.frameflame");
    let ingest_args = IngestArgs {
        input:           ingest_input,
        out:             Some(pack),
        mode:            Mode::Fast,
        budget:          args.budget,
        fps:             args.fps,
        download_out:    args.work_dir.clone(),
        work_dir:        Some(args.work_dir.clone()),
        timeout_seconds: args.timeout_seconds,
        max_height:      args.max_height,
        sub_langs:       args.sub_langs.clone(),
        no_captions:     args.no_captions,
        dry_run:         false,
    };
    ingest(&ingest_args)?;

    let video = local_video.unwrap_or_else(|| args.work_dir.join("video.mp4"));
    split(&SplitArgs {
        video:           video.clone(),
        out:             Some(args.work_dir.join("segments")),
        segment_seconds: args.segment_seconds,
    })?;
    Ok(video)
}

fn finish_process(args: &ProcessArgs, video: PathBuf) -> Result<()> {
    let pack = args.work_dir.join("video.frameflame");
    context(&ContextArgs {
        dir:            args.work_dir.clone(),
        captions:       None,
        video:          Some(video.clone()),
        pack:           Some(pack.clone()),
        window_seconds: args.window_seconds,
    })?;

    verify(&VerifyArgs {
        dir:              args.work_dir.clone(),
        video:            Some(video),
        pack:             Some(pack),
        json:             false,
        min_frames:       1,
        require_segments: true,
        require_zoom:     false,
    })
}

fn process_video_path(args: &ProcessArgs) -> PathBuf {
    if ytdlp::is_probably_url(&args.input) {
        return args.work_dir.join("video.mp4");
    }

    let source = PathBuf::from(&args.input);
    let extension = source.extension().and_then(|value| value.to_str()).unwrap_or("mp4");
    args.work_dir.join(format!("video.{extension}"))
}

fn process_cache_variant(args: &ProcessArgs) -> String {
    format!(
        "cache_version=1;budget={};fps={};segment_seconds={};window_seconds={};max_height={};sub_langs={};no_captions={}",
        args.budget,
        args.fps,
        args.segment_seconds,
        args.window_seconds,
        args.max_height,
        args.sub_langs,
        args.no_captions
    )
}

fn copy_process_input_video(input: &str, work_dir: &Path) -> Result<PathBuf> {
    let source = PathBuf::from(input);
    if !source.is_file() {
        bail!("input video does not exist: {}", source.display());
    }

    let extension = source.extension().and_then(|value| value.to_str()).unwrap_or("mp4");
    let target = work_dir.join(format!("video.{extension}"));
    if source != target {
        fs::copy(&source, &target).with_context(|| {
            format!("failed to copy {} to {}", source.display(), target.display())
        })?;
    }
    Ok(target)
}

pub fn ingest(args: &IngestArgs) -> Result<()> {
    diagnostics::require_ffmpeg_tools()?;

    if ytdlp::is_probably_url(&args.input) {
        diagnostics::require_ytdlp()?;
        if args.dry_run {
            output_line(format_args!("url: {}", args.input))?;
            output_line(format_args!("download_out: {}", args.download_out.display()))?;
            output_line(format_args!("mode: {}", args.mode.as_str()))?;
            return Ok(());
        }

        let download = ytdlp::download_url(
            &args.input,
            &DownloadConfig {
                output_root:     args.download_out.clone(),
                output_dir:      args.work_dir.clone(),
                timeout_seconds: args.timeout_seconds,
                max_height:      args.max_height,
                captions:        !args.no_captions,
                sub_langs:       args.sub_langs.clone(),
            },
        )?;
        output_line(format_args!("downloaded: {}", download.video_path.display()))?;
        return ingest_video_path(args, &download.video_path, &args.input);
    }

    let video = PathBuf::from(&args.input);
    if !video.is_file() {
        bail!("input video does not exist: {}", video.display());
    }

    if args.dry_run {
        let out = args.out.clone().unwrap_or_else(|| ffmpeg::default_output_dir(&video));
        output_line(format_args!("video: {}", video.display()))?;
        output_line(format_args!("out: {}", out.display()))?;
        output_line(format_args!("mode: {}", args.mode.as_str()))?;
        return Ok(());
    }

    ingest_video_path(args, &video, &args.input)
}

fn ingest_video_path(args: &IngestArgs, video: &Path, source_input: &str) -> Result<()> {
    let out = args.out.clone().unwrap_or_else(|| ffmpeg::default_output_dir(video));

    fs::create_dir_all(out.join("selected"))
        .with_context(|| format!("failed to create {}", out.display()))?;

    let metadata = ffmpeg::probe_video(video)?;
    let samples =
        ffmpeg::scan_frames(video, &FrameScanConfig { fps: args.fps, width: 320, height: 180 })?;
    let selected = select_samples(&samples, args.budget);

    write_manifest(&out, args, source_input, video, &metadata)?;
    write_frames(&out, video, &selected)?;
    write_index(&out, video, &metadata, selected.len())?;

    output_line(format_args!("wrote {}", out.display()))?;
    Ok(())
}

pub fn download(args: &DownloadArgs) -> Result<()> {
    diagnostics::require_ytdlp()?;
    workspace::ensure_gitignore_for(args.dir.as_ref().unwrap_or(&args.out))?;

    let result = ytdlp::download_url(
        &args.url,
        &DownloadConfig {
            output_root:     args.out.clone(),
            output_dir:      args.dir.clone(),
            timeout_seconds: args.timeout_seconds,
            max_height:      args.max_height,
            captions:        !args.no_captions,
            sub_langs:       args.sub_langs.clone(),
        },
    )?;

    output_line(format_args!("downloaded: {}", result.video_path.display()))?;
    output_line(format_args!("directory: {}", result.directory.display()))?;
    if result.sidecars.is_empty() {
        output_line(format_args!("captions: none found"))?;
    } else {
        for sidecar in result.sidecars {
            output_line(format_args!("sidecar: {}", sidecar.display()))?;
        }
    }

    Ok(())
}

pub fn inspect(args: &InspectArgs) -> Result<()> {
    if ytdlp::is_probably_url(&args.target) {
        diagnostics::require_ytdlp()?;
        let metadata = ytdlp::inspect_url(&args.target, args.timeout_seconds)?;
        output_line(format_args!("{}", serde_json::to_string_pretty(&metadata)?))?;
        return Ok(());
    }

    let target = PathBuf::from(&args.target);
    if target.is_dir() {
        inspect_pack(&target)
    } else if target.is_file() {
        diagnostics::require_ffmpeg_tools()?;
        let metadata = ffmpeg::probe_video(&target)?;
        output_line(format_args!("{}", serde_json::to_string_pretty(&metadata)?))?;
        Ok(())
    } else {
        bail!("target does not exist: {}", target.display());
    }
}

pub fn split(args: &SplitArgs) -> Result<()> {
    diagnostics::require_ffmpeg_tools()?;
    if !args.video.is_file() {
        bail!("video does not exist: {}", args.video.display());
    }

    let out = args.out.clone().unwrap_or_else(|| args.video.with_file_name("segments"));
    ffmpeg::split_video(&args.video, &out, args.segment_seconds)?;
    output_line(format_args!("wrote {}", out.display()))?;
    Ok(())
}

pub fn context(args: &ContextArgs) -> Result<()> {
    diagnostics::require_ffmpeg_tools()?;
    let written = context_builder::build(&ContextConfig {
        dir:            args.dir.clone(),
        captions:       args.captions.clone(),
        video:          args.video.clone(),
        pack:           args.pack.clone(),
        window_seconds: args.window_seconds,
    })?;

    for path in written {
        output_line(format_args!("wrote {}", path.display()))?;
    }
    Ok(())
}

pub fn verify(args: &VerifyArgs) -> Result<()> {
    let verification = verify_dir(args)?;
    if args.json {
        output_line(format_args!("{}", serde_json::to_string_pretty(&verification)?))?;
    } else {
        output_line(format_args!("dir: {}", verification.dir.display()))?;
        output_line(format_args!("ok: {}", verification.ok))?;
        output_line(format_args!("video: {}", verification.video))?;
        output_line(format_args!("pack: {}", verification.pack))?;
        output_line(format_args!("selected_frames: {}", verification.selected_frames))?;
        output_line(format_args!("captions: {}", verification.captions))?;
        output_line(format_args!("segments: {}", verification.segments))?;
        output_line(format_args!("zoom_frames: {}", verification.zoom_frames))?;
        for error in &verification.errors {
            output_line(format_args!("error: {error}"))?;
        }
    }

    if verification.ok { Ok(()) } else { bail!("verification failed for {}", args.dir.display()) }
}

pub fn zoom(args: &ZoomArgs) -> Result<()> {
    diagnostics::require_ffmpeg_tools()?;

    let center_ms = parse_timestamp_ms(&args.at)?;
    let duration_ms =
        ffmpeg::probe_video(&args.video)?.duration_seconds.map(seconds_to_ms).transpose()?;
    if duration_ms.is_some_and(|duration_ms| center_ms > duration_ms) {
        bail!("zoom timestamp {} is beyond video duration", args.at);
    }

    let half_window_ms = seconds_to_ms(args.window / 2.0)?;
    let start_ms = center_ms.saturating_sub(half_window_ms);
    let frame_count = zoom_frame_count(args.window, args.fps)?;
    let step_ms = seconds_to_ms(1.0 / args.fps)?;
    let out = args.out.clone().unwrap_or_else(|| PathBuf::from("zoom.frames"));

    fs::create_dir_all(&out).with_context(|| format!("failed to create {}", out.display()))?;
    let mut extracted = 0_usize;
    for index in 0..frame_count {
        let index_ms =
            u64::try_from(index).context("zoom frame index overflowed")?.saturating_mul(step_ms);
        let timestamp_ms = start_ms.saturating_add(index_ms);
        if duration_ms.is_some_and(|duration_ms| timestamp_ms > duration_ms) {
            continue;
        }
        let image_path = out.join(format!("{index:06}.jpg"));
        ffmpeg::extract_frame(&args.video, timestamp_ms, &image_path)?;
        extracted = extracted.saturating_add(1);
    }

    if extracted == 0 {
        bail!("zoom produced no frames for {}", args.at);
    }
    output_line(format_args!("wrote {}", out.display()))?;
    Ok(())
}

fn verify_dir(args: &VerifyArgs) -> Result<Verification> {
    let dir = &args.dir;
    let video = args.video.clone().unwrap_or_else(|| dir.join("video.mp4"));
    let pack_dir = args.pack.clone().unwrap_or_else(|| dir.join("video.frameflame"));
    let captions = count_direct_files_with_extension(dir, "srt")?;
    let segments = count_files_with_extension(&dir.join("segments"), "mp4")?;
    let zoom_frames = count_files_with_extension(&dir.join("zooms"), "jpg")?;
    let selected_frames = count_files_with_extension(&pack_dir.join("selected"), "jpg")?;

    let mut errors = Vec::new();
    require(video.is_file(), &format!("missing video file: {}", video.display()), &mut errors);
    require(
        pack_dir.is_dir(),
        &format!("missing pack directory: {}", pack_dir.display()),
        &mut errors,
    );
    require(pack_dir.join("manifest.json").is_file(), "missing pack manifest.json", &mut errors);
    require(pack_dir.join("frames.jsonl").is_file(), "missing pack frames.jsonl", &mut errors);
    require(
        selected_frames >= args.min_frames,
        &format!(
            "selected frame count {selected_frames} is less than --min-frames {}",
            args.min_frames
        ),
        &mut errors,
    );
    if captions > 0 {
        require(
            dir.join("video.context.md").is_file(),
            "captions exist but video.context.md is missing",
            &mut errors,
        );
    }
    require(dir.join("inspect.context.md").is_file(), "missing inspect.context.md", &mut errors);
    require(
        dir.join("inspect.visual.context.md").is_file(),
        "missing inspect.visual.context.md",
        &mut errors,
    );
    if args.require_segments {
        require(
            segments > 0,
            "--require-segments set but no segments/*.mp4 files exist",
            &mut errors,
        );
    }
    if args.require_zoom {
        require(
            zoom_frames > 0,
            "--require-zoom set but no zooms/**/*.jpg files exist",
            &mut errors,
        );
    }

    Ok(Verification {
        dir: dir.clone(),
        ok: errors.is_empty(),
        video: video.is_file(),
        pack: pack_dir.is_dir(),
        manifest: pack_dir.join("manifest.json").is_file(),
        frames_jsonl: pack_dir.join("frames.jsonl").is_file(),
        selected_frames,
        captions,
        caption_context: dir.join("video.context.md").is_file(),
        inspect_context: dir.join("inspect.context.md").is_file(),
        visual_context: dir.join("inspect.visual.context.md").is_file(),
        segments,
        zoom_frames,
        errors,
    })
}

fn require(condition: bool, message: &str, errors: &mut Vec<String>) {
    if !condition {
        errors.push(message.to_string());
    }
}

fn count_direct_files_with_extension(dir: &Path, extension: &str) -> Result<usize> {
    if !dir.is_dir() {
        return Ok(0);
    }

    let mut count = 0_usize;
    for entry in fs::read_dir(dir).with_context(|| format!("failed to read {}", dir.display()))? {
        let path = entry?.path();
        if path.is_file() && path.extension().and_then(|value| value.to_str()) == Some(extension) {
            count = count.saturating_add(1);
        }
    }
    Ok(count)
}

fn count_files_with_extension(dir: &Path, extension: &str) -> Result<usize> {
    if !dir.is_dir() {
        return Ok(0);
    }

    let mut count = 0_usize;
    for entry in fs::read_dir(dir).with_context(|| format!("failed to read {}", dir.display()))? {
        let path = entry?.path();
        if path.is_dir() {
            count = count.saturating_add(count_files_with_extension(&path, extension)?);
        } else if path.extension().and_then(|value| value.to_str()) == Some(extension) {
            count = count.saturating_add(1);
        }
    }
    Ok(count)
}

fn select_samples(samples: &[FrameSample], budget: usize) -> Vec<FrameSample> {
    if samples.is_empty() || budget == 0 {
        return Vec::new();
    }

    let step = max(1, samples.len().div_ceil(budget));
    samples.iter().step_by(step).take(budget).cloned().collect()
}

fn write_manifest(
    out: &Path,
    args: &IngestArgs,
    source_input: &str,
    video: &Path,
    metadata: &VideoMetadata,
) -> Result<()> {
    let manifest = Manifest {
        source_input,
        source_video: video,
        mode: args.mode.as_str(),
        budget: args.budget,
        analysis_fps: args.fps,
        metadata,
    };
    let file = ffmpeg::create_file(&out.join("manifest.json"))?;
    serde_json::to_writer_pretty(file, &manifest).context("failed to write manifest.json")
}

fn write_frames(out: &Path, video: &Path, selected: &[FrameSample]) -> Result<()> {
    let mut file = ffmpeg::create_file(&out.join("frames.jsonl"))?;
    for (record_index, sample) in selected.iter().enumerate() {
        let image_path = PathBuf::from("selected").join(format!("{record_index:06}.jpg"));
        ffmpeg::extract_frame(video, sample.timestamp_ms, &out.join(&image_path))?;
        let record = FrameRecord {
            frame_id:          format!("f_{record_index:06}"),
            timestamp_ms:      sample.timestamp_ms,
            image_path:        &image_path,
            mean_luma:         sample.mean_luma,
            mad_from_previous: sample.mad_from_previous,
            selection_reason:  "uniform coverage",
        };
        serde_json::to_writer(&mut file, &record).context("failed to write frame record")?;
        writeln!(file).context("failed to write frames.jsonl newline")?;
    }
    Ok(())
}

fn write_index(
    out: &Path,
    video: &Path,
    metadata: &VideoMetadata,
    frame_count: usize,
) -> Result<()> {
    let duration = metadata
        .duration_seconds
        .map_or_else(|| "unknown".to_string(), |duration| format!("{duration:.2}s"));
    let content = format!(
        "# FlameFrame Evidence Pack\n\n- source: `{}`\n- duration: {duration}\n- video streams: {}\n- audio streams: {}\n- selected frames: {frame_count}\n\nStart with `frames.jsonl`, then inspect `selected/` images only when needed.\n",
        video.display(),
        metadata.video_streams,
        metadata.audio_streams
    );
    fs::write(out.join("index.md"), content).context("failed to write index.md")
}

fn inspect_pack(pack: &Path) -> Result<()> {
    let index = pack.join("index.md");
    if index.is_file() {
        output_line(format_args!(
            "{}",
            fs::read_to_string(&index).context("failed to read index.md")?
        ))?;
        return Ok(());
    }

    let manifest = pack.join("manifest.json");
    if manifest.is_file() {
        output_line(format_args!(
            "{}",
            fs::read_to_string(&manifest).context("failed to read manifest.json")?
        ))?;
        return Ok(());
    }

    bail!("{} is not a FlameFrame evidence pack", pack.display())
}

#[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss, clippy::cast_sign_loss)]
fn zoom_frame_count(window: f64, fps: f64) -> Result<usize> {
    if !window.is_finite() || !fps.is_finite() || window <= 0.0 || fps <= 0.0 {
        bail!("window and fps must be positive finite numbers");
    }

    let frames = (window * fps).ceil();
    if frames > usize::MAX as f64 {
        bail!("zoom frame count is too large");
    }

    Ok(max(1, frames as usize))
}

fn parse_timestamp_ms(value: &str) -> Result<u64> {
    if !value.contains(':') {
        return seconds_to_ms(value.parse::<f64>().context("timestamp is not a number")?);
    }

    let parts: Vec<_> = value.split(':').collect();
    if parts.len() > 3 {
        bail!("timestamp must be seconds, MM:SS, or HH:MM:SS");
    }

    let mut seconds = 0.0;
    for part in parts {
        seconds = seconds * 60.0 + part.parse::<f64>().context("invalid timestamp component")?;
    }
    seconds_to_ms(seconds)
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn seconds_to_ms(seconds: f64) -> Result<u64> {
    if !seconds.is_finite() || seconds < 0.0 {
        bail!("seconds must be a non-negative finite number");
    }
    Ok((seconds * 1000.0).round() as u64)
}

fn output_line(args: Arguments<'_>) -> Result<()> {
    let mut stdout = io::stdout().lock();
    writeln!(stdout, "{args}").context("failed to write command output")?;
    Ok(())
}
