use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=src/ffi_ffmpeg.c");
    println!("cargo:rerun-if-changed=vendor/ffmpeg/lib/libavcodec.a");
    println!("cargo::rustc-check-cfg=cfg(eos_ffmpeg)");

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let ffmpeg_dir = manifest_dir.join("vendor").join("ffmpeg");
    let lib_dir = ffmpeg_dir.join("lib");
    let inc_dir = ffmpeg_dir.join("include");

    if !lib_dir.join("libavcodec.a").exists() {
        println!("cargo:warning=FFmpeg static libs not found in userspace/vendor/ffmpeg/lib — building without FFmpeg");
        return;
    }

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let obj_path = out_dir.join("ffi_ffmpeg.o");
    let lib_path = out_dir.join("libffi_ffmpeg.a");

    let status = Command::new("musl-gcc")
        .args([
            "-c",
            "-O3",
            "-I",
            inc_dir.to_str().unwrap(),
            manifest_dir.join("src/ffi_ffmpeg.c").to_str().unwrap(),
            "-o",
            obj_path.to_str().unwrap(),
        ])
        .status()
        .expect("Failed to execute musl-gcc for ffi_ffmpeg.c");

    if !status.success() {
        panic!("musl-gcc failed to compile src/ffi_ffmpeg.c");
    }

    let ar_status = Command::new("ar")
        .args(["rcs", lib_path.to_str().unwrap(), obj_path.to_str().unwrap()])
        .status()
        .expect("Failed to execute ar");

    if !ar_status.success() {
        panic!("ar failed to create libffi_ffmpeg.a");
    }

    println!("cargo:rustc-cfg=eos_ffmpeg");
    println!("cargo:rustc-link-search=native={}", out_dir.display());
    println!("cargo:rustc-link-search=native={}", lib_dir.display());
    println!("cargo:rustc-link-lib=static=ffi_ffmpeg");
    println!("cargo:rustc-link-lib=static=avformat");
    println!("cargo:rustc-link-lib=static=avcodec");
    println!("cargo:rustc-link-lib=static=swresample");
    println!("cargo:rustc-link-lib=static=swscale");
    println!("cargo:rustc-link-lib=static=avutil");
}
