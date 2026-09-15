use std::{env, path::PathBuf, process::Command};

fn main() {
    // Tauri codegen: embed tauri.conf.json and generate context for `tauri::generate_context!`.
    // Keep this before the OCR helper so both env vars are available.
    // On non-macOS, or when tauri.conf.json is absent (e.g. `cargo check` on CI without frontend),
    // treat Tauri as best-effort and do not hard-fail the build.
    println!("cargo:rerun-if-changed=tauri.conf.json");
    println!("cargo:rerun-if-changed=capabilities/default.json");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos")
        && PathBuf::from("tauri.conf.json").exists()
    {
        // `try_build` returns Result instead of panicking like `build()`
        match tauri_build::try_build(tauri_build::Attributes::default()) {
            Ok(_) => {}
            Err(err) => {
                println!("cargo:warning=tauri build skipped: {err:#}");
            }
        }
    }

    // Windows images need an embedded manifest to bind Common-Controls v6
    // (`comctl32`); without it the loader aborts with `0xC0000139`.
    println!("cargo:rerun-if-changed=app.manifest");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        embed_windows_manifest();
    }

    println!("cargo:rerun-if-changed=macos/window_ocr.swift");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }

    let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let source = manifest_dir.join("macos/window_ocr.swift");
    let output = PathBuf::from(env::var_os("OUT_DIR").expect("out dir")).join("window-ocr");
    let swiftc = Command::new("xcrun")
        .args(["--find", "swiftc"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string());

    let Some(swiftc) = swiftc else {
        println!("cargo:warning=stock-operator OCR helper unavailable: xcrun swiftc not found");
        return;
    };
    let sdk = Command::new("xcrun")
        .args(["--sdk", "macosx", "--show-sdk-path"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string());
    let Some(sdk) = sdk else {
        println!("cargo:warning=stock-operator OCR helper unavailable: macOS SDK not found");
        return;
    };
    let swift_arch = match env::var("CARGO_CFG_TARGET_ARCH").as_deref() {
        Ok("aarch64") => "arm64",
        Ok("x86_64") => "x86_64",
        _ => {
            println!(
                "cargo:warning=stock-operator OCR helper unavailable: unsupported target arch"
            );
            return;
        }
    };
    let deployment_target =
        env::var("MACOSX_DEPLOYMENT_TARGET").unwrap_or_else(|_| "26.0".to_string());
    let target = format!("{swift_arch}-apple-macosx{deployment_target}");
    let compile = Command::new(swiftc)
        .args([
            "-parse-as-library",
            "-O",
            "-sdk",
            &sdk,
            "-target",
            &target,
            "-framework",
            "AppKit",
            "-framework",
            "ScreenCaptureKit",
            "-framework",
            "Vision",
            "-o",
        ])
        .arg(&output)
        .arg(&source)
        .output();
    match compile {
        Ok(helper_output) if helper_output.status.success() => {
            println!(
                "cargo:rustc-env=STOCK_OPERATOR_OCR_HELPER={}",
                output.display()
            );
        }
        Ok(helper_output) => println!(
            "cargo:warning=stock-operator OCR helper could not be compiled: {}",
            String::from_utf8_lossy(&helper_output.stderr).trim()
        ),
        Err(error) => {
            println!("cargo:warning=stock-operator OCR helper could not be executed: {error}")
        }
    }
}

/// MSVC only: link `app.manifest` into the binary as its `RT_MANIFEST` resource.
///
/// `link.exe` parses `/MANIFESTINPUT` values itself and needs a full path free of
/// the separators it splits on, so an unusable path degrades to a warning instead
/// of failing the build.
fn embed_windows_manifest() {
    if env::var("CARGO_CFG_TARGET_ENV").as_deref() != Ok("msvc") {
        println!("cargo:warning=app.manifest not embedded: target env is not MSVC");
        return;
    }
    let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let manifest = manifest_dir.join("app.manifest");
    if !manifest.is_file() {
        println!(
            "cargo:warning=app.manifest not embedded: {} is missing",
            manifest.display()
        );
        return;
    }
    let path = manifest.to_string_lossy();
    if path.contains([',', ';', '=']) {
        println!("cargo:warning=app.manifest not embedded: linker cannot take path {path}");
        return;
    }
    println!("cargo:rustc-link-arg-bins=/MANIFEST:EMBED");
    println!("cargo:rustc-link-arg-bins=/MANIFESTINPUT:{path}");
}
