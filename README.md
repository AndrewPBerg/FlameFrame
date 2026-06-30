# FlameFrame

Local-first video-to-context compiler for AI agents.

FlameFrame burns video down into compact, timestamped evidence packs: selected frames, metadata, scores, and agent-readable indexes. See [`vision.md`](vision.md) for product direction.

## Development

```bash
cargo +nightly fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
```

## Pre-commit

```bash
pre-commit install
pre-commit install --hook-type commit-msg
pre-commit run --all-files
pre-commit run --hook-stage manual --all-files
```
