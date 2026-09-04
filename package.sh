#!/bin/sh
set -eu

identity="-"
output="target/stock-operator/Stock Operator.app"
target=""

while [ $# -gt 0 ]; do
  case "$1" in
    --identity)
      if [ $# -lt 2 ]; then
        echo "missing value for --identity" >&2
        exit 1
      fi
      identity="$2"
      shift 2
      ;;
    --identity=*)
      identity="${1#*=}"
      shift
      ;;
    --output)
      if [ $# -lt 2 ]; then
        echo "missing value for --output" >&2
        exit 1
      fi
      output="$2"
      shift 2
      ;;
    --output=*)
      output="${1#*=}"
      shift
      ;;
    --target)
      if [ $# -lt 2 ]; then
        echo "missing value for --target" >&2
        exit 1
      fi
      target="$2"
      shift 2
      ;;
    --target=*)
      target="${1#*=}"
      shift
      ;;
    -h|--help)
      echo "Usage: sh package.sh [--identity IDENTITY] [--output PATH] [--target TARGET]" >&2
      exit 0
      ;;
    --)
      shift
      break
      ;;
    -*)
      echo "unknown option: $1" >&2
      exit 1
      ;;
    *)
      echo "unknown argument: $1" >&2
      exit 1
      ;;
  esac
done

root=$(git rev-parse --show-toplevel)
root=$(cd "$root" && pwd)
manifest="$root/Cargo.toml"

metadata=$(cargo metadata --no-deps --format-version 1 --manifest-path "$manifest")
target_dir=$(printf '%s' "$metadata" | sed -n 's/.*"target_directory"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p')
version=$(printf '%s' "$metadata" | sed -n 's/.*"name"[[:space:]]*:[[:space:]]*"stock-operator"[[:space:]]*,[[:space:]]*"version"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p')

if [ -z "$target_dir" ]; then
  echo "failed to determine target directory from cargo metadata" >&2
  exit 1
fi
if [ -z "$version" ]; then
  echo "failed to determine package version from cargo metadata" >&2
  exit 1
fi

if [ -z "$target" ]; then
  release_dir="$target_dir/release"
else
  release_dir="$target_dir/$target/release"
fi

case "$output" in
  /*) app="$output" ;;
  *) app="$root/$output" ;;
esac
contents="$app/Contents"
macos="$contents/MacOS"
helpers="$contents/Helpers"
resources="$contents/Resources"

if [ ! -f "$root/tauri.conf.json" ]; then
  echo "missing tauri.conf.json" >&2
  exit 1
fi
if [ ! -f "$root/ui/index.html" ]; then
  echo "missing ui/index.html" >&2
  exit 1
fi

if [ -f "$root/ui/package.json" ]; then
  echo "building frontend (ui) with bun..."
  bun install --cwd "$root/ui"
  bun run --cwd "$root/ui" build
  if [ ! -f "$root/ui/dist/index.html" ]; then
    echo "frontend build failed: missing ui/dist/index.html" >&2
    exit 1
  fi
fi

if [ -z "$target" ]; then
  MACOSX_DEPLOYMENT_TARGET=26.0 cargo build --release -p stock-operator --manifest-path "$manifest"
else
  MACOSX_DEPLOYMENT_TARGET=26.0 cargo build --release --target "$target" -p stock-operator --manifest-path "$manifest"
fi

tauri_bundle="$release_dir/bundle/macos/Stock Operator.app"
binary="$release_dir/stock-operator"
helper=$(ls -t "$release_dir"/build/stock-operator-*/out/window-ocr 2>/dev/null | head -n 1 || true)
if [ -n "$helper" ] && [ ! -f "$helper" ]; then
  helper=""
fi

if [ ! -f "$binary" ]; then
  echo "missing release binary: $binary" >&2
  exit 1
fi
if [ -z "$helper" ]; then
  echo "missing release OCR helper; inspect the stock-operator build warnings" >&2
  exit 1
fi

if [ -e "$tauri_bundle" ]; then
  echo "found Tauri bundle at $tauri_bundle, copying helper into it"
  if [ -e "$app" ]; then
    rm -rf "$app"
  fi
  cp -R "$tauri_bundle" "$app"
  mkdir -p "$helpers"
  cp "$helper" "$helpers/window-ocr"
  chmod 755 "$helpers/window-ocr"
  codesign --force --sign "$identity" --identifier "com.cloudiful.stock-operator.window-ocr" "$helpers/window-ocr"
  codesign --force --sign "$identity" --identifier "com.cloudiful.stock-operator" "$macos/stock-operator"
  codesign --force --sign "$identity" "$app"
  codesign --verify --deep --strict --verbose=2 "$app"
  plutil -lint "$contents/Info.plist"
  printf '%s\n' "$app"
  exit 0
fi

if [ -e "$app" ]; then
  rm -rf "$app"
fi
mkdir -p "$macos" "$helpers" "$resources"
cp "$binary" "$macos/stock-operator"
cp "$helper" "$helpers/window-ocr"
icon_src="$root/icons/icon.icns"
if [ -f "$icon_src" ]; then
  cp "$icon_src" "$resources/icon.icns"
fi
sed "s/__VERSION__/$version/g" "$root/macos/Info.plist.template" > "$contents/Info.plist"
chmod 755 "$macos/stock-operator" "$helpers/window-ocr"
codesign --force --sign "$identity" --identifier "com.cloudiful.stock-operator.window-ocr" "$helpers/window-ocr"
codesign --force --sign "$identity" --identifier "com.cloudiful.stock-operator" "$macos/stock-operator"
codesign --force --sign "$identity" "$app"
codesign --verify --deep --strict --verbose=2 "$app"
plutil -lint "$contents/Info.plist"
printf '%s\n' "$app"
