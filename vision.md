# FlameFrame Vision

FlameFrame is a local-first video-to-context compiler for AI agents.

It does **not** try to make an agent or model process raw video directly. Instead, it burns a video down into a compact, timestamped evidence pack: selected frames, speech/caption transcript windows, metadata, and selection reasons that Pi, Codex, or any local model can reason over.

## Why this exists

Agents are good at reasoning over text and images, but raw video is too large, repetitive, and expensive to feed into a model. Most videos contain long stretches of redundant frames, static screens, repeated speaker shots, or visually noisy transitions.

FlameFrame should make video useful to agents by extracting the parts worth looking at.

The core idea:

```text
video file
  -> ffprobe metadata
  -> ffmpeg normalized frame/audio streams
  -> Rust scoring, dedupe, and frame selection
  -> optional STT or embedded-caption transcript
  -> timestamped evidence pack
  -> agent answers with citations
```

## Product shape

FlameFrame should be a standalone CLI first, then integrations second.

The durable product is the evidence compiler. Codex skills, Pi tools, MCP servers, desktop apps, or Cascading Labs services can all wrap the same stable CLI/output format later.

Primary commands:

```bash
flameframe ingest demo.mp4 --mode fast --budget 32
flameframe inspect demo.frameflame
flameframe zoom demo.mp4 --at 00:01:24 --window 8 --fps 4
```

## Output contract

An ingest should create a directory like:

```text
demo.frameflame/
  index.md             # compact agent-readable summary/timeline
  manifest.json        # video metadata and run config
  frames.jsonl         # selected frame records and scores
  transcript.json      # optional timestamped transcript from audio STT or captions
  selected/            # selected high-res frames
    000001.jpg
    000002.jpg
```

`index.md` is the first file an agent should read. The JSON/JSONL files are for deeper inspection and future tooling.

Example frame record:

```json
{
  "frame_id": "f_000023",
  "timestamp_ms": 42150,
  "image_path": "selected/000023.jpg",
  "source": ["uniform", "visual_change", "transcript_alignment"],
  "scores": {
    "mad_last_kept": 0.041,
    "dhash_distance": 17,
    "scene_score": 0.0
  },
  "transcript_window": {
    "start_ms": 39000,
    "end_ms": 46000,
    "text": "Now we configure the API key and restart the local server."
  },
  "selection_reason": "visual change aligned with transcript topic shift"
}
```

## Implementation philosophy

FlameFrame is written in Rust because it is comfortable, shareable, memory-safe, and has a strong ecosystem for CLI tooling, JSON, concurrency, tests, and image processing.

Rust owns:

- CLI and configuration
- ffprobe/ffmpeg subprocess orchestration
- raw frame stream reading
- frame scoring and dedupe
- timestamp bookkeeping
- evidence pack generation
- cache/layout management

External tools own:

- video/audio decode: FFmpeg
- transcription from audio: whisper.cpp or faster-whisper
- transcript extraction from embedded captions/subtitles when available
- local vision model descriptions: Ollama/llama.cpp/LM Studio later

Avoid binding directly to libav in v0. Spawn FFmpeg and read normalized raw frames from stdout.

## Frame selection strategy

Do not rely on fixed interval sampling alone. Use multiple cheap candidate lanes, then dedupe and budget them.

Candidate lanes:

1. **Uniform coverage** — sample every N seconds so the whole timeline is represented.
2. **Scene/change detection** — use FFmpeg scene/scdet signals for cuts and big visual changes.
3. **Representative windows** — use FFmpeg thumbnail-like behavior to avoid picking bad transitional frames.
4. **Visual novelty** — compute luma diff, histograms, and dHash/pHash-like distances.
5. **Transcript/caption alignment** — keep frames near topic shifts, caption boundaries, or phrases like “look here”, “this”, “on screen”.
6. **Coverage guardrails** — prevent huge unrepresented gaps unless the video is truly static.
7. **Zoom pass** — let the agent request more frames around a timestamp when evidence is insufficient.

Initial fast path:

```bash
ffmpeg -hide_banner -loglevel error -i input.mp4 \
  -vf "fps=2,scale=320:180:force_original_aspect_ratio=decrease,pad=320:180:-1:-1,format=gray" \
  -f rawvideo pipe:1
```

Rust can then read fixed-size grayscale frames and compute cheap features without decoding images.

## Agent integration

Codex/Pi should not inspect raw video directly. They should call FlameFrame, then reason over the evidence pack.

Skill behavior:

1. If the user provides `.mp4`, `.mkv`, `.mov`, or `.webm`, run `flameframe ingest`.
2. Read `<video>.frameflame/index.md` first.
3. Use `frames.jsonl`, `transcript.json`, and selected images only when needed.
4. If evidence is insufficient, call `flameframe zoom` around the relevant timestamp.
5. Answer with timestamped evidence.
6. Do not invent visual details not present in the pack.

Pi integration can start as a custom tool, then later grow into an input hook that detects video paths and offers to process them.

## Modes

```text
fast
  metadata, low-rate frame scoring, visual dedupe, compact index

balanced
  fast + STT/caption transcript + scene lane + stronger timeline
```

Default for agents should be fast or balanced, with explicit zoom/deep follow-ups.

## Non-goals for v0

- No custom video decoder.
- No direct FFmpeg/libav bindings.
- No built-in Whisper implementation.
- No built-in VLM runtime.
- No cloud dependency.
- No promise that selected frames are exhaustive.

The v0 goal is a reliable, local, deterministic evidence pack.

## MVP roadmap

### v0: working evidence compiler

- `flameframe ingest <video> --out <dir>`
- ffprobe metadata
- FFmpeg low-res raw grayscale stream
- Rust luma diff + dHash dedupe
- selected high-res frame extraction
- `manifest.json`, `frames.jsonl`, `index.md`

### v1: useful agent context

- transcript via embedded captions/subtitles when present, or whisper.cpp/faster-whisper if installed
- FFmpeg scene/scdet candidate lane
- profile presets: screen, lecture, meeting, action, surveillance
- `flameframe zoom`

### v2: richer evidence

- better transcript/caption alignment
- better score blending
- compact exports for Codex/Pi

### v3: product/integration layer

- Codex skill
- Pi custom tool
- optional Pi auto-detection hook
- MCP/server mode if useful
- Cascading Labs integration if the personal tool proves valuable

## Cascading Labs boundary

This starts as a personal project.
It can become a Cascading Labs video context engine later when:

- the CLI contract is stable
- the evidence pack proves useful on real videos
- agents can reliably answer with timestamped citations
- there is a clear internal/customer workflow worth productizing


## Tagline

**FlameFrame burns video down to timestamped AI context.**
