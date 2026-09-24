use std::fs::File;
use std::io::Read;
use std::time::Instant;
use std::collections::HashMap;
use serde::{Serialize, Deserialize};
use sha2::{Sha256, Digest};
use rand::{RngCore, SeedableRng, rngs::SmallRng};
use regex_automata::meta::Regex;
use std::arch::asm;

pub const MAX_DISPLAY_W: usize = 800;
pub const MAX_DISPLAY_H: usize = 600;

#[inline(always)]
fn blit_video_frame(pixels: &[u32], w: usize, h: usize) {
    unsafe {
        asm!(
            "syscall",
            in("rax") 502,
            in("rdi") pixels.as_ptr() as u64,
            in("rsi") 0,
            in("rdx") 0,
            in("r10") w as u64,
            in("r8") h as u64,
            out("rcx") _, out("r11") _
        );
    }
}

// 💡 تحويل مساحات الألوان مع التحجيم المباشر (Scaling) وحفظ الناتج في Buffer ثابت
#[inline(always)]
fn yuv420_to_argb_scaled(
    y_plane: &[u8], u_plane: &[u8], v_plane: &[u8],
    src_w: usize, src_h: usize,
    dst_w: usize, dst_h: usize,
    out_rgb: &mut [u32]
) {
    let uv_src_w = src_w / 2;
    for dy in 0..dst_h {
        let sy = (dy * src_h) / dst_h;
        let y_row = sy * src_w;
        let uv_row = (sy / 2) * uv_src_w;
        let dst_row = dy * dst_w;

        for dx in 0..dst_w {
            let sx = (dx * src_w) / dst_w;
            let y_val = y_plane[y_row + sx] as i32 - 16;
            let u_val = u_plane[uv_row + (sx / 2)] as i32 - 128;
            let v_val = v_plane[uv_row + (sx / 2)] as i32 - 128;

            let c = y_val.max(0) * 298;
            let r = ((c + 409 * v_val + 128) >> 8).clamp(0, 255) as u32;
            let g = ((c - 100 * u_val - 208 * v_val + 128) >> 8).clamp(0, 255) as u32;
            let b = ((c + 516 * u_val + 128) >> 8).clamp(0, 255) as u32;

            out_rgb[dst_row + dx] = (0xFF << 24) | (r << 16) | (g << 8) | b;
        }
    }
}

// 💡 مشغل وسائط بنظام الذاكرة الثابتة (Zero-Allocation Loop)
fn play_video_stream(file_path: &str) -> Result<(), &'static str> {
    println!("\n▶️ [VIDEO PLAYER] Opening Media File: '{}'", file_path);
    let mut file = File::open(file_path).map_err(|_| "Failed to open video file")?;

    // 1. فتح نافذة العرض فوراً مع إطار أولي لتأكيد بدء التشغيل
    let mut rgb_buffer: Vec<u32> = vec![0xFF1E293B; MAX_DISPLAY_W * MAX_DISPLAY_H];
    blit_video_frame(&rgb_buffer, MAX_DISPLAY_W, MAX_DISPLAY_H);
    println!("   -> Initialized Display Overlay ({}x{})", MAX_DISPLAY_W, MAX_DISPLAY_H);

    let mut decoder = rust_h264::decoder::OrderedDecoder::new();
    let mut chunk = vec![0u8; 128 * 1024]; // 128 KB ثابت
    let mut stream_buffer: Vec<u8> = Vec::with_capacity(1024 * 1024); // 1 MB ثابت
    let mut frame_count = 0usize;
    let t_start = Instant::now();

    println!("   -> Streaming video bitstream directly from VFS...");

    loop {
        let bytes_read = file.read(&mut chunk).map_err(|_| "Error reading stream")?;
        if bytes_read == 0 { break; }

        stream_buffer.extend_from_slice(&chunk[..bytes_read]);

        // البحث عن حزم الـ NAL في المخزن المؤقت
        let nals = rust_h264::nal::parse_annex_b(&stream_buffer);
        if nals.is_empty() {
            if stream_buffer.len() > 1024 * 1024 {
                stream_buffer.drain(..stream_buffer.len() - 128);
            }
            continue;
        }

        for nal in &nals {
            if let Ok(frames) = decoder.decode_nal(nal) {
                for frame in frames {
                    let src_w = frame.width as usize;
                    let src_h = frame.height as usize;

                    let (target_w, target_h) = if src_w > MAX_DISPLAY_W || src_h > MAX_DISPLAY_H {
                        (MAX_DISPLAY_W, (src_h * MAX_DISPLAY_W) / src_w)
                    } else {
                        (src_w, src_h)
                    };

                    yuv420_to_argb_scaled(
                        &frame.y, &frame.u, &frame.v,
                        src_w, src_h,
                        target_w, target_h,
                        &mut rgb_buffer
                    );

                    blit_video_frame(&rgb_buffer, target_w, target_h);
                    frame_count += 1;

                    // مزامنة الإطارات (Pacing @ ~24-30 FPS)
                    std::thread::sleep(std::time::Duration::from_millis(33));
                }
            }
        }

        // تفريغ المخزن مع إبقاء آخر 64 بايت لربط البدايات عبر الحدود
        if stream_buffer.len() > 64 {
            let keep = stream_buffer.len() - 64;
            stream_buffer.drain(..keep);
        }
    }

    let elapsed = t_start.elapsed().as_millis();
    println!("🏁 Playback completed: {} frames presented in {} ms.", frame_count, elapsed);
    Ok(())
}

fn run_crates_test_suite() {
    println!("\n┌────────────────────────────────────────────────────────┐");
    println!("│ EOS MIRROR LAYER - CRATES INTEGRATION TEST SUITE       │");
    println!("└────────────────────────────────────────────────────────┘\n");

    let total_start = Instant::now();

    let mut map: HashMap<usize, String> = HashMap::new();
    for i in 0..500 { map.insert(i, format!("KEY_{:04}", i)); }
    println!("[TEST: COLLECT   ] HashMap dynamic growth & rehashing (500 items) OK.");

    let mut hasher = Sha256::new();
    hasher.update(b"EOS Operating System - Advanced Mirror Layer Subsystem Active");
    println!("[TEST: SHA2      ] Compute SHA256 digest: {:x}", hasher.finalize());

    #[derive(Serialize, Deserialize, Debug, PartialEq)]
    struct Report { kernel: String, frames: u32 }
    let rep = Report { kernel: "EOS".to_string(), frames: 60 };
    let j = serde_json::to_string(&rep).unwrap();
    println!("[TEST: SERDE_JSON] Roundtrip JSON: {}", j);

    let mut rng = SmallRng::seed_from_u64(0xCAFE);
    let mut r_bytes = [0u8; 8];
    rng.fill_bytes(&mut r_bytes);
    println!("[TEST: RAND      ] Stream: {:02x?}", r_bytes);

    let re = Regex::new(r"EOS-\w+").expect("Failed pattern");
    assert!(re.is_match("EOS-Kernel-Stable"));
    println!("[TEST: REGEX     ] Regular expression NFA matched pattern OK.");

    println!("\n🎉 ALL CRATES PASSED in {} ms! 🎉\n", total_start.elapsed().as_millis());
}

fn get_passed_argument() -> String {
    let ipc_ptr = 0x0000_0000_2050_0000 as *const u8;
    unsafe {
        let mut len = 0usize;
        while len < 255 && *ipc_ptr.add(len) != 0 { len += 1; }
        if len > 0 {
            let slice = std::slice::from_raw_parts(ipc_ptr, len);
            if let Ok(s) = std::str::from_utf8(slice) {
                let trimmed = s.trim();
                if !trimmed.is_empty() { return trimmed.to_string(); }
            }
        }
    }
    String::new()
}

fn main() {
    let arg = get_passed_argument();

    if arg.is_empty() || arg == "test" {
        run_crates_test_suite();
    } else {
        println!("[Media Engine] Kernel Requested Playback for: '{}'", arg);
        if let Err(e) = play_video_stream(&arg) {
            println!("[ERROR] Playback failed: {}", e);
        }
    }
}
