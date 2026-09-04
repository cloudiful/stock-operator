#!/bin/sh
set -eu

# Generate macOS icon assets from vector source.
# Uses macOS built-ins sips and iconutil; no permanent dependencies.
# Rasterizes icons/icon.svg to a high-resolution PNG and builds icons/icon.icns.

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
icons_dir="$script_dir"

# Fallback to git root if script location does not contain the source (e.g. invoked elsewhere)
if [ ! -f "$icons_dir/icon.svg" ]; then
  if command -v git >/dev/null 2>&1; then
    root=$(git rev-parse --show-toplevel 2>/dev/null || true)
    if [ -n "${root:-}" ] && [ -f "$root/icons/icon.svg" ]; then
      icons_dir="$root/icons"
    fi
  fi
fi

src_svg="$icons_dir/icon.svg"
dst_png="$icons_dir/icon.png"
dst_icns="$icons_dir/icon.icns"

if [ ! -f "$src_svg" ]; then
  echo "missing icon source: $src_svg" >&2
  exit 1
fi

if ! command -v sips >/dev/null 2>&1; then
  echo "required command not found: sips (macOS only)" >&2
  exit 1
fi

if ! command -v iconutil >/dev/null 2>&1; then
  echo "required command not found: iconutil (macOS only)" >&2
  exit 1
fi

tmpdir=$(mktemp -d "${TMPDIR:-/tmp}/stock-operator-icon.XXXXXX")
trap 'rm -rf "$tmpdir"' EXIT INT TERM HUP

tmp_png="$tmpdir/icon-1024.png"
iconset="$tmpdir/icon.iconset"
mkdir -p "$iconset"

echo "rasterizing $src_svg -> $dst_png (1024x1024)..." >&2
sips -s format png "$src_svg" --out "$tmp_png" >/dev/null
sips -z 1024 1024 "$tmp_png" --out "$dst_png" >/dev/null

if [ ! -f "$dst_png" ]; then
  echo "failed to create $dst_png" >&2
  exit 1
fi

echo "generating iconset..." >&2
sips -z 16 16 "$dst_png" --out "$iconset/icon_16x16.png" >/dev/null
sips -z 32 32 "$dst_png" --out "$iconset/icon_16x16@2x.png" >/dev/null
sips -z 32 32 "$dst_png" --out "$iconset/icon_32x32.png" >/dev/null
sips -z 64 64 "$dst_png" --out "$iconset/icon_32x32@2x.png" >/dev/null
sips -z 128 128 "$dst_png" --out "$iconset/icon_128x128.png" >/dev/null
sips -z 256 256 "$dst_png" --out "$iconset/icon_128x128@2x.png" >/dev/null
sips -z 256 256 "$dst_png" --out "$iconset/icon_256x256.png" >/dev/null
sips -z 512 512 "$dst_png" --out "$iconset/icon_256x256@2x.png" >/dev/null
sips -z 512 512 "$dst_png" --out "$iconset/icon_512x512.png" >/dev/null
sips -z 1024 1024 "$dst_png" --out "$iconset/icon_512x512@2x.png" >/dev/null

echo "compiling $dst_icns..." >&2
iconutil -c icns "$iconset" -o "$dst_icns" >/dev/null

if [ ! -f "$dst_icns" ]; then
  echo "failed to create $dst_icns" >&2
  exit 1
fi

echo "generated $dst_png and $dst_icns" >&2
