fn main() {
    tauri_build::build();

    // Compile the AuraAudio Swift package (ScreenCaptureKit loopback) on macOS.
    // Requires: Xcode Command Line Tools (xcode-select --install)
    #[cfg(target_os = "macos")]
    compile_swift_audio();
}

#[cfg(target_os = "macos")]
fn compile_swift_audio() {
    use std::process::Command;

    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let out_dir = std::env::var("OUT_DIR").unwrap();
    let pkg_path = format!("{}/swift-audio", manifest_dir);
    let lib_out = format!("{}/libAuraAudio.a", out_dir);

    // Detect architecture
    let target_arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    let triple = if target_arch == "aarch64" {
        "arm64-apple-macosx13.0"
    } else {
        "x86_64-apple-macosx13.0"
    };

    // Build the Swift package into a static library
    let status = Command::new("swiftc")
        .args([
            "-module-name",
            "AuraAudio",
            "-emit-library",
            "-static",
            "-target",
            triple,
            "-O",
            // Sources
            &format!("{}/Sources/AuraAudio/Loopback.swift", pkg_path),
            &format!("{}/Sources/AuraAudio/ProcessTap.swift", pkg_path),
            &format!("{}/Sources/AuraAudio/Calendar.swift", pkg_path),
            &format!("{}/Sources/AuraAudio/Permissions.swift", pkg_path),
            // Frameworks
            "-framework",
            "ScreenCaptureKit",
            "-framework",
            "CoreMedia",
            "-framework",
            "CoreAudio",
            "-framework",
            "AVFoundation",
            "-framework",
            "AVFAudio",
            "-framework",
            "Foundation",
            "-framework",
            "EventKit",
            "-o",
            &lib_out,
        ])
        .status()
        .expect("swiftc not found. Install Xcode Command Line Tools: xcode-select --install");

    assert!(
        status.success(),
        "Swift compilation of AuraAudio failed (see output above)"
    );

    // Tell Cargo where to find the lib and what to link
    println!("cargo:rustc-link-search=native={}", out_dir);
    println!("cargo:rustc-link-lib=static=AuraAudio");

    // Required system frameworks
    println!("cargo:rustc-link-lib=framework=ScreenCaptureKit");
    println!("cargo:rustc-link-lib=framework=CoreMedia");
    println!("cargo:rustc-link-lib=framework=CoreAudio");
    println!("cargo:rustc-link-lib=framework=AVFoundation");
    println!("cargo:rustc-link-lib=framework=AVFAudio");
    println!("cargo:rustc-link-lib=framework=Foundation");
    println!("cargo:rustc-link-lib=framework=EventKit");

    // Rebuild if Swift sources change
    println!(
        "cargo:rerun-if-changed={}/Sources/AuraAudio/Loopback.swift",
        pkg_path
    );
    println!(
        "cargo:rerun-if-changed={}/Sources/AuraAudio/ProcessTap.swift",
        pkg_path
    );
    println!(
        "cargo:rerun-if-changed={}/Sources/AuraAudio/Calendar.swift",
        pkg_path
    );
    println!(
        "cargo:rerun-if-changed={}/Sources/AuraAudio/Permissions.swift",
        pkg_path
    );
    println!("cargo:rerun-if-changed={}/Package.swift", pkg_path);
}
