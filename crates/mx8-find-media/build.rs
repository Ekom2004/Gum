use std::env;
use std::path::{Path, PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=wrapper.h");
    println!("cargo:rerun-if-env-changed=MX8_FFMPEG_PREFIX");
    println!("cargo:rerun-if-env-changed=HOMEBREW_PREFIX");

    let prefix = ffmpeg_prefix().expect("failed to locate FFmpeg installation prefix");
    let include_dir = prefix.join("include");
    let lib_dir = prefix.join("lib");

    assert!(
        include_dir.is_dir(),
        "FFmpeg include directory not found at {}",
        include_dir.display()
    );
    assert!(
        lib_dir.is_dir(),
        "FFmpeg lib directory not found at {}",
        lib_dir.display()
    );

    println!("cargo:rustc-link-search=native={}", lib_dir.display());
    println!("cargo:rustc-link-lib=avformat");
    println!("cargo:rustc-link-lib=avcodec");
    println!("cargo:rustc-link-lib=avutil");
    println!("cargo:rustc-link-lib=swscale");

    let bindings = bindgen::Builder::default()
        .header("wrapper.h")
        .clang_arg(format!("-I{}", include_dir.display()))
        .allowlist_type("AV.*")
        .allowlist_type("SwsContext")
        .allowlist_function("av.*")
        .allowlist_function("sws_.*")
        .allowlist_function("swscale_.*")
        .allowlist_var("AV.*")
        .allowlist_var("SWS_.*")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .generate_comments(false)
        .layout_tests(false)
        .generate()
        .expect("failed to generate libav bindings");

    let out_path = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR is not set"));
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("failed to write libav bindings");
}

fn ffmpeg_prefix() -> Option<PathBuf> {
    env::var_os("MX8_FFMPEG_PREFIX")
        .map(PathBuf::from)
        .filter(|path| path.is_dir())
        .or_else(|| {
            env::var_os("HOMEBREW_PREFIX")
                .map(PathBuf::from)
                .filter(|path| path.is_dir())
        })
        .or_else(|| default_prefixes().into_iter().find(|path| path.is_dir()))
}

fn default_prefixes() -> Vec<PathBuf> {
    vec![
        Path::new("/opt/homebrew/opt/ffmpeg").to_path_buf(),
        Path::new("/opt/homebrew").to_path_buf(),
        Path::new("/usr/local/opt/ffmpeg").to_path_buf(),
        Path::new("/usr/local").to_path_buf(),
    ]
}
