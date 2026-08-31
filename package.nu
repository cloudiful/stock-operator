#!/usr/bin/env nu

def main [
  --identity: string = "-"
  --output: path = "target/stock-operator/Stock Operator.app"
  --target: string = ""
] {
  let root = (git rev-parse --show-toplevel | str trim | path expand)
  let manifest = ($root | path join "Cargo.toml")
  let metadata = (^cargo metadata --no-deps --format-version 1 --manifest-path $manifest | from json)
  let package = ($metadata.packages | where name == "stock-operator" | first)
  let version = $package.version
  let target_dir = ($metadata.target_directory | path expand)
  let release_dir = if ($target | is-empty) { $target_dir | path join "release" } else { $target_dir | path join $target | path join "release" }
  let app = if ($output | str starts-with "/") { $output } else { $root | path join $output }
  let contents = ($app | path join "Contents")
  let macos = ($contents | path join "MacOS")
  let helpers = ($contents | path join "Helpers")
  let resources = ($contents | path join "Resources")

  # Verify Tauri + UI assets are present for desktop bundling
  if not (($root | path join "tauri.conf.json") | path exists) { error make {msg: "missing tauri.conf.json"} }
  if not (($root | path join "ui/index.html") | path exists) { error make {msg: "missing ui/index.html"} }

  # Build frontend assets (Vite) before Rust - required for Tauri frontendDist ui/dist.
  if (($root | path join "ui/package.json") | path exists) {
    print "building frontend (ui) with bun..."
    ^bun install --cwd ($root | path join "ui")
    ^bun run --cwd ($root | path join "ui") build
    if not (($root | path join "ui/dist/index.html") | path exists) { error make {msg: "frontend build failed: missing ui/dist/index.html"} }
  }

  # Build release binary (Tauri codegen runs via build.rs). Keep deployment target 26.0 aligned with Info.plist.
  with-env {MACOSX_DEPLOYMENT_TARGET: "26.0"} {
    if ($target | is-empty) {
      ^cargo build --release -p stock-operator --manifest-path $manifest
    } else {
      ^cargo build --release --target $target -p stock-operator --manifest-path $manifest
    }
  }

  # Prefer an already-produced Tauri bundle (when `cargo tauri build` was used), otherwise collect manual artifacts.
  let tauri_bundle = ($release_dir | path join "bundle/macos/Stock Operator.app")
  let use_tauri_bundle = ($tauri_bundle | path exists)

  let helper_candidates = (
    glob ($release_dir | path join "build/stock-operator-*/out/window-ocr")
    | where {|path| ($path | path type) == "file" }
    | sort-by {|path| (ls $path | first).modified }
  )
  let helper = if ($helper_candidates | is-empty) { "" } else { $helper_candidates | last }
  let binary = ($release_dir | path join "stock-operator")
  if not ($binary | path exists) { error make {msg: $"missing release binary: ($binary)"} }
  if ($helper | is-empty) { error make {msg: "missing release OCR helper; inspect the stock-operator build warnings"} }

  if $use_tauri_bundle {
    print $"found Tauri bundle at ($tauri_bundle), copying helper into it"
    if ($app | path exists) { rm --recursive --force $app }
    cp --recursive $tauri_bundle $app
    # Ensure Helpers exists and contains window-ocr
    mkdir $helpers
    cp $helper ($helpers | path join "window-ocr")
    ^chmod 755 ($helpers | path join "window-ocr")
    ^codesign --force --sign $identity --identifier "com.cloudiful.stock-operator.window-ocr" ($helpers | path join "window-ocr")
    ^codesign --force --sign $identity --identifier "com.cloudiful.stock-operator" ($macos | path join "stock-operator")
    ^codesign --force --sign $identity $app
    ^codesign --verify --deep --strict --verbose=2 $app
    ^plutil -lint ($contents | path join "Info.plist")
    print $app
    return
  }

  if ($app | path exists) { rm --recursive --force $app }
  mkdir $macos $helpers $resources
  cp $binary ($macos | path join "stock-operator")
  cp $helper ($helpers | path join "window-ocr")
  # Bundle icon for Finder/Dock (CFBundleIconFile expects icon in Resources)
  let icon_src = ($root | path join "icons/icon.icns")
  if ($icon_src | path exists) {
    cp $icon_src ($resources | path join "icon.icns")
  }
  open ($root | path join "macos/Info.plist.template")
  | str replace --all "__VERSION__" $version
  | save ($contents | path join "Info.plist")

  ^chmod 755 ($macos | path join "stock-operator") ($helpers | path join "window-ocr")
  ^codesign --force --sign $identity --identifier "com.cloudiful.stock-operator.window-ocr" ($helpers | path join "window-ocr")
  ^codesign --force --sign $identity --identifier "com.cloudiful.stock-operator" ($macos | path join "stock-operator")
  ^codesign --force --sign $identity $app
  ^codesign --verify --deep --strict --verbose=2 $app
  ^plutil -lint ($contents | path join "Info.plist")

  print $app
}
