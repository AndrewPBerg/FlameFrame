use std::path::PathBuf;

use clap::{ArgAction, Args, Parser, Subcommand, ValueEnum};

#[derive(Debug, Parser)]
#[command(
    version,
    about = "Local-first video-to-context compiler for AI agents",
    disable_version_flag = true
)]
pub struct Cli {
    /// Print version.
    #[arg(short = 'v', long = "version", action = ArgAction::Version, num_args = 0)]
    pub version: Option<bool>,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Print the built-in manual.
    Man,
    /// Check local tool availability and versions.
    Doctor(DoctorArgs),
    /// Upgrade flameframe from GitHub Releases using the install script.
    Upgrade(UpgradeArgs),
    /// Remove the FlameFrame binary currently running.
    Uninstall,
    /// Run the full agent-context workflow: ingest, split, context, verify.
    Process(ProcessArgs),
    /// Compile a video into a .frameflame evidence pack.
    Ingest(IngestArgs),
    /// Download a URL video with captions using yt-dlp.
    Download(DownloadArgs),
    /// Print a compact summary of an evidence pack or URL metadata.
    Inspect(InspectArgs),
    /// Split a local video into segment files.
    Split(SplitArgs),
    /// Build transcript and visual markdown context for a downloaded video
    /// directory.
    Context(ContextArgs),
    /// Verify a processed work directory has the expected agent artifacts.
    Verify(VerifyArgs),
    /// Extract a focused frame window around a timestamp.
    Zoom(ZoomArgs),
}

#[derive(Debug, Args)]
pub struct DoctorArgs {
    /// Emit machine-readable JSON diagnostics.
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct UpgradeArgs {
    /// GitHub release version to install, or latest.
    #[arg(long, default_value = "latest")]
    pub version:     String,
    /// GitHub owner/repo to install from.
    #[arg(long, default_value = "AndrewPBerg/FlameFrame")]
    pub repo:        String,
    /// Installation directory to pass to the install script.
    #[arg(long)]
    pub install_dir: Option<PathBuf>,
    /// Print the upgrade command without running it.
    #[arg(long)]
    pub dry_run:     bool,
}

#[derive(Debug, Args)]
pub struct ProcessArgs {
    /// Input video path or HTTP(S) URL.
    pub input:           String,
    /// Deterministic work directory for generated artifacts.
    #[arg(long)]
    pub work_dir:        PathBuf,
    /// Maximum selected frames to write.
    #[arg(long, default_value_t = 32)]
    pub budget:          usize,
    /// Low-resolution analysis FPS.
    #[arg(long, default_value_t = 2.0)]
    pub fps:             f64,
    /// Segment length in seconds.
    #[arg(long, default_value_t = 300)]
    pub segment_seconds: u64,
    /// Transcript markdown window size in seconds.
    #[arg(long, default_value_t = 60)]
    pub window_seconds:  u64,
    /// Kill URL download if it takes longer than this many seconds.
    #[arg(long, default_value_t = 900)]
    pub timeout_seconds: u64,
    /// Maximum URL video height to download.
    #[arg(long, default_value_t = 480)]
    pub max_height:      u16,
    /// Subtitle/caption languages to request for URL input.
    #[arg(long, default_value = "en,en-orig")]
    pub sub_langs:       String,
    /// Do not download captions/subtitles for URL input.
    #[arg(long)]
    pub no_captions:     bool,
}

#[derive(Debug, Args)]
pub struct IngestArgs {
    /// Input video path or HTTP(S) URL.
    pub input:           String,
    /// Output evidence pack directory. Defaults to <video-stem>.frameflame.
    #[arg(long)]
    pub out:             Option<PathBuf>,
    /// Ingest mode. Fast is visual-only; balanced reserves hooks for
    /// transcript/scene lanes.
    #[arg(long, value_enum, default_value_t = Mode::Fast)]
    pub mode:            Mode,
    /// Maximum selected frames to write.
    #[arg(long, default_value_t = 32)]
    pub budget:          usize,
    /// Low-resolution analysis FPS.
    #[arg(long, default_value_t = 2.0)]
    pub fps:             f64,
    /// Directory for URL downloads before ingest.
    #[arg(long, default_value = crate::workspace::PROJECT_WORKSPACE)]
    pub download_out:    PathBuf,
    /// Exact work directory for URL input. Makes repeatable agent workflows
    /// easy.
    #[arg(long)]
    pub work_dir:        Option<PathBuf>,
    /// Kill URL download if it takes longer than this many seconds.
    #[arg(long, default_value_t = 900)]
    pub timeout_seconds: u64,
    /// Maximum URL video height to download.
    #[arg(long, default_value_t = 480)]
    pub max_height:      u16,
    /// Subtitle/caption languages to request for URL input.
    #[arg(long, default_value = "en,en-orig")]
    pub sub_langs:       String,
    /// Do not download captions/subtitles for URL input.
    #[arg(long)]
    pub no_captions:     bool,
    /// Do not write files; validate tools and show the planned output
    /// directory.
    #[arg(long)]
    pub dry_run:         bool,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum Mode {
    Fast,
    Balanced,
}

impl Mode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Fast => "fast",
            Self::Balanced => "balanced",
        }
    }
}

#[derive(Debug, Args)]
pub struct DownloadArgs {
    /// HTTP(S) video URL.
    pub url:             String,
    /// Root directory for downloaded URL media.
    #[arg(long, default_value = crate::workspace::PROJECT_WORKSPACE)]
    pub out:             PathBuf,
    /// Exact download directory instead of creating a timestamped child under
    /// --out.
    #[arg(long)]
    pub dir:             Option<PathBuf>,
    /// Kill yt-dlp if the download takes longer than this many seconds.
    #[arg(long, default_value_t = 900)]
    pub timeout_seconds: u64,
    /// Maximum video height to download. Defaults to lower-resolution 480p.
    #[arg(long, default_value_t = 480)]
    pub max_height:      u16,
    /// Subtitle/caption languages to request from yt-dlp.
    #[arg(long, default_value = "en,en-orig")]
    pub sub_langs:       String,
    /// Do not download captions/subtitles.
    #[arg(long)]
    pub no_captions:     bool,
}

#[derive(Debug, Args)]
pub struct InspectArgs {
    /// Evidence pack directory, video path, or HTTP(S) URL.
    pub target:          String,
    /// Kill URL metadata inspection if it takes longer than this many seconds.
    #[arg(long, default_value_t = 60)]
    pub timeout_seconds: u64,
}

#[derive(Debug, Args)]
pub struct SplitArgs {
    /// Input video path.
    pub video:           PathBuf,
    /// Output directory for segment files. Defaults to <video-dir>/segments.
    #[arg(long)]
    pub out:             Option<PathBuf>,
    /// Segment length in seconds.
    #[arg(long, default_value_t = 300)]
    pub segment_seconds: u64,
}

#[derive(Debug, Args)]
pub struct ContextArgs {
    /// Download/work directory containing video.mp4 and captions.
    pub dir:            PathBuf,
    /// Caption sidecar file. Defaults to <dir>/video.en.srt, then first .srt in
    /// dir.
    #[arg(long)]
    pub captions:       Option<PathBuf>,
    /// Video file. Defaults to <dir>/video.mp4.
    #[arg(long)]
    pub video:          Option<PathBuf>,
    /// FlameFrame pack directory. Defaults to <dir>/video.frameflame when
    /// present.
    #[arg(long)]
    pub pack:           Option<PathBuf>,
    /// Transcript markdown window size in seconds.
    #[arg(long, default_value_t = 60)]
    pub window_seconds: u64,
}

#[derive(Debug, Args)]
pub struct VerifyArgs {
    /// Download/work directory to verify.
    pub dir:              PathBuf,
    /// Video file. Defaults to <dir>/video.mp4.
    #[arg(long)]
    pub video:            Option<PathBuf>,
    /// FlameFrame pack directory. Defaults to <dir>/video.frameflame.
    #[arg(long)]
    pub pack:             Option<PathBuf>,
    /// Emit machine-readable JSON verification output.
    #[arg(long)]
    pub json:             bool,
    /// Minimum selected frame images required in video.frameflame/selected.
    #[arg(long, default_value_t = 1)]
    pub min_frames:       usize,
    /// Require at least one split segment under segments/.
    #[arg(long)]
    pub require_segments: bool,
    /// Require at least one zoom frame under zooms/.
    #[arg(long)]
    pub require_zoom:     bool,
}

#[derive(Debug, Args)]
pub struct ZoomArgs {
    /// Input video path.
    pub video:  PathBuf,
    /// Center timestamp, e.g. 00:01:24 or 84.0.
    #[arg(long)]
    pub at:     String,
    /// Window size in seconds.
    #[arg(long, default_value_t = 8.0)]
    pub window: f64,
    /// Extraction FPS for the focused window.
    #[arg(long, default_value_t = 4.0)]
    pub fps:    f64,
    /// Output directory for zoom frames.
    #[arg(long)]
    pub out:    Option<PathBuf>,
}
