# FlameFrame

FlameFrame is a local-first video-to-context compiler for AI agents.

It turns either a YouTube URL or a local video file into a work directory containing transcript markdown, selected visual frames, split video segments, and optional zoom frames.

## Install from GitHub Releases

Unix/macOS:

```bash
curl -fsSL https://raw.githubusercontent.com/AndrewPBerg/FlameFrame/main/install.sh | sh
```

Windows PowerShell:

```powershell
irm https://raw.githubusercontent.com/AndrewPBerg/FlameFrame/main/install.ps1 | iex
```

Runtime tools must be installed separately and available on `PATH`:

- `ffmpeg`
- `ffprobe`
- `yt-dlp` for YouTube/URL inputs

Check the install:

```bash
flameframe -h
flameframe -v
flameframe doctor
```

Upgrade later:

```bash
flameframe upgrade

# Remove the binary currently running (prompts for confirmation)
flameframe uninstall
```

## Agent skills

Install the FlameFrame skill for an agent. Installations are user-wide by default;
pass `--project` to write into the current repository instead.

```bash
flameframe agent install --claude
flameframe agent install --codex --project
flameframe agent install --agents
flameframe agent install --pi --project
```

The Pi install also adds a session-scoped evidence browser extension. After
restarting Pi or running `/reload`, FlameFrame runs appear in the session shelf;
use `/flameframe-browser` to inspect their selected frames. Use `o` for a native terminal
image preview and its complete transcript window (`j`/`k` scroll it), `c` to copy the
selected image path (or place it in the editor when no clipboard helper is available),
and `z` to prepare a zoom command.

## Main workflows

YouTube URL:

```bash
flameframe process 'https://www.youtube.com/watch?v=HddLMOxE1Dk' \
  --work-dir .flameframe/river-monsters \
  --max-height 480 \
  --budget 40
```

Local video file:

```bash
flameframe process ./recording.mp4 \
  --work-dir .flameframe/recording.context \
  --budget 16 \
  --fps 1 \
  --segment-seconds 60
```

FlameFrame writes generated project-scoped artifacts under `.flameframe/` by default and creates `.flameframe/.gitignore` so evidence packs, downloads, segments, and zooms stay out of git. Completed `process` runs are also cached under the system temporary directory; a later run with the same URL or local file path and processing settings reports an `inspect cache: hit` and reuses the indexed artifacts.

Agent review order:

1. Read `<work-dir>/video.context.md` first when captions exist.
2. Read `<work-dir>/inspect.visual.context.md` next.
3. Open selected frame images only when markdown is not enough.
4. Run `flameframe zoom ...` for timestamps that need closer visual evidence.

## Manual

The CLI carries its own manual:

```bash
flameframe man
```

## Release

Push a version tag to build and upload binaries:

```bash
git tag v0.1.1
git push origin v0.1.1
```

The release workflow publishes binaries for Linux x64, macOS Apple Silicon, and Windows x64.

## Development

```bash
cargo +nightly fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
```
