#!/usr/bin/env sh
set -eu

repo="${FLAMEFRAME_REPO:-AndrewPBerg/FlameFrame}"
version="${FLAMEFRAME_VERSION:-latest}"
bin_dir="${FLAMEFRAME_INSTALL_DIR:-$HOME/.local/bin}"

die() {
  echo "flameframe install: $*" >&2
  exit 1
}

need() {
  command -v "$1" >/dev/null 2>&1 || die "missing required command: $1"
}

need curl
need tar
need uname

os="$(uname -s)"
arch="$(uname -m)"

case "$os:$arch" in
  Linux:x86_64|Linux:amd64)
    target="x86_64-unknown-linux-gnu"
    ;;
  Darwin:x86_64|Darwin:amd64)
    target="x86_64-apple-darwin"
    ;;
  Darwin:arm64|Darwin:aarch64)
    target="aarch64-apple-darwin"
    ;;
  *)
    die "unsupported OS/arch: $os $arch"
    ;;
esac

asset="flameframe-$target.tar.gz"
if [ "$version" = "latest" ]; then
  url="https://github.com/$repo/releases/latest/download/$asset"
else
  url="https://github.com/$repo/releases/download/$version/$asset"
fi

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT INT TERM

mkdir -p "$bin_dir"
echo "Downloading $url"
curl -fsSL "$url" -o "$tmp/$asset"
tar -xzf "$tmp/$asset" -C "$tmp"

binary="$tmp/flameframe-$target/flameframe"
[ -f "$binary" ] || die "release asset did not contain flameframe"

cp "$binary" "$bin_dir/flameframe"
chmod 755 "$bin_dir/flameframe"

echo "Installed: $bin_dir/flameframe"
"$bin_dir/flameframe" --version

case ":$PATH:" in
  *":$bin_dir:"*) ;;
  *)
    echo "Warning: $bin_dir is not on PATH. Add it, for example:"
    echo "  export PATH=\"$bin_dir:\$PATH\""
    ;;
esac

echo "Next: install ffmpeg/ffprobe and yt-dlp, then run: flameframe doctor"
