use std::io::{self, Write};

use anyhow::{Context, Result};

pub const TEXT: &str = r#"FLAMEFRAME(1)

NAME
    flameframe - local-first video-to-context compiler for AI agents

SYNOPSIS
    flameframe -h
    flameframe -v
    flameframe doctor [--json]
    flameframe upgrade [--version <VERSION>] [--dry-run]
    flameframe process <URL_OR_VIDEO> --work-dir <DIR> [OPTIONS]
    flameframe inspect <URL_OR_VIDEO_OR_PACK> [--timeout-seconds <SECONDS>]
    flameframe zoom <VIDEO> --at <TIMESTAMP> [--window <SECONDS>] [--fps <FPS>] [--out <DIR>]
    flameframe man

DESCRIPTION
    FlameFrame turns a YouTube URL or local video file into an agent-readable work
    directory. The output is designed for AI coding agents: read markdown first,
    inspect selected frames only when needed, then zoom around timestamps that need
    more visual evidence.

    The full process pipeline is:

        ingest -> split -> context -> verify

    URL inputs are downloaded with yt-dlp. Local files are copied into the work
    directory. Video probing, splitting, selected frame extraction, and zooms are
    performed with ffmpeg/ffprobe.

PRIMARY USE CASES
    YouTube or web video URL:
        flameframe process 'https://www.youtube.com/watch?v=VIDEO_ID' \
          --work-dir data/example \
          --max-height 480 \
          --budget 40

    Local video file:
        flameframe process ./recording.mp4 \
          --work-dir data/recording.context \
          --budget 16 \
          --fps 1 \
          --segment-seconds 60

OUTPUT
    <work-dir>/video.mp4
        Normalized local video for URL downloads or copied local input.

    <work-dir>/video.context.md
        Transcript-first timestamp windows when captions exist.

    <work-dir>/inspect.visual.context.md
        Selected frame index, nearby transcript/no-caption guidance, and suggested
        zoom commands.

    <work-dir>/video.frameflame/
        Evidence pack containing manifest.json, frames.jsonl, selected images, and
        index.md.

    <work-dir>/segments/
        Split video chunks for localized review.

    <work-dir>/zooms/
        Optional focused frame windows created with flameframe zoom.

AGENT REVIEW ORDER
    1. Read video.context.md first when present.
    2. Read inspect.visual.context.md next.
    3. Open selected frame images only when markdown is not enough.
    4. Run flameframe zoom at timestamps that need motion/detail.

COMMANDS
    doctor
        Check ffmpeg, ffprobe, and yt-dlp availability. Use --json for machine
        readable diagnostics.

    upgrade
        Re-run the GitHub Release install script. Use --dry-run to preview the
        platform command without network or filesystem changes.

    process
        Run the complete workflow for a URL or local video.

    ingest
        Compile a video into a .frameflame evidence pack.

    download
        Download a URL video and captions using yt-dlp.

    inspect
        Print a compact summary of an evidence pack, local video, or URL metadata.

    split
        Split a local video into segments.

    context
        Build transcript and visual markdown context for a work directory.

    verify
        Verify that a processed work directory contains the expected artifacts.

    zoom
        Extract a focused frame window around a timestamp.

    man
        Print this manual.

DEPENDENCIES
    Runtime tools must be installed separately and available on PATH:

        ffmpeg
        ffprobe
        yt-dlp       required for URL inputs only

    Run `flameframe doctor` after installing FlameFrame. Run `flameframe upgrade`
    to update to the latest GitHub Release binary.

INSTALLATION
    FlameFrame is shipped as GitHub Release binaries. Install scripts download the
    right release asset for your OS/architecture and place the binary on PATH.

    Unix/macOS:
        curl -fsSL https://raw.githubusercontent.com/AndrewPBerg/FlameFrame/main/install.sh | sh

    Windows PowerShell:
        irm https://raw.githubusercontent.com/AndrewPBerg/FlameFrame/main/install.ps1 | iex

EXAMPLES
    Inspect a URL before processing:
        flameframe inspect 'https://www.youtube.com/watch?v=VIDEO_ID'

    Process a URL without captions:
        flameframe process 'https://www.youtube.com/watch?v=VIDEO_ID' \
          --work-dir data/no-captions \
          --no-captions

    Zoom around a timestamp:
        flameframe zoom data/example/video.mp4 \
          --at 00:10:00 \
          --window 12 \
          --fps 2 \
          --out data/example/zooms/00-10-00

NOTES
    FlameFrame is local-first. It does not call an AI API. It produces files that
    an AI agent can read.
"#;

pub fn print() -> Result<()> {
    let mut stdout = io::stdout().lock();
    stdout.write_all(TEXT.as_bytes()).context("failed to write manual")?;
    stdout.write_all(b"\n").context("failed to write manual newline")?;
    Ok(())
}
