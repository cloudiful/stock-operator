#!/usr/bin/env nu

def main [
  --identity: string = "-"
  --output: path = "target/stock-operator/Stock Operator.app"
] {
  let root = (git rev-parse --show-toplevel | str trim | path expand)
  let manifest = ($root | path join "Cargo.toml")
  let metadata = (^cargo metadata --no-deps --format-version 1 --manifest-path $manifest | from json)
  let package = ($metadata.packages | where name == "stock-operator" | first)
  let version = $package.version
  let target_dir = ($metadata.target_directory | path expand)
  let app = if ($output | path type) == "absolute" { $output } else { $root | path join $output }
  let contents = ($app | path join "Contents")
  let macos = ($contents | path join "MacOS")
  let helpers = ($contents | path join "Helpers")

  ^cargo build --release -p stock-operator --manifest-path $manifest

  let helper = (
    glob ($target_dir | path join "release/build/stock-operator-*/out/window-ocr")
    | where {|path| ($path | path type) == "file" }
    | sort-by {|path| (ls $path | first).modified }
    | last
  )
  let binary = ($target_dir | path join "release/stock-operator")
  if not ($binary | path exists) { error make {msg: $"missing release binary: ($binary)"} }
  if ($helper | is-empty) { error make {msg: "missing release OCR helper; inspect the stock-operator build warnings"} }

  if ($app | path exists) { rm --recursive --force $app }
  mkdir $macos $helpers
  cp $binary ($macos | path join "stock-operator")
  cp $helper ($helpers | path join "window-ocr")
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
