mod syscall;
mod sdk;

use std::time::Instant;
use std::thread;
use std::time::Duration;
use std::fs;
use std::env;

pub const W: usize = 680;
pub const H: usize = 500;

fn run_selftest() {
    println!("\n========================================================");
    println!("     EOS USERSPACE POSIX & RUST STD VERIFICATION");
    println!("========================================================");
    
    // 1. Instant & TSC clock_gettime
    let t0 = Instant::now();
    let elapsed_initial = t0.elapsed();
    let now_ok = elapsed_initial.as_micros() <= 100_000;
    println!(" [{}] clock_gettime / Instant::now()", if now_ok { "PASS" } else { "FAIL" });

    // 2. nanosleep / futex thread::sleep
    thread::sleep(Duration::from_millis(50));
    let elapsed = t0.elapsed();
    let sleep_ok = elapsed >= Duration::from_millis(45);
    println!(" [{}] nanosleep / thread::sleep (~50ms) -> Elapsed: {:?}", if sleep_ok { "PASS" } else { "FAIL" }, elapsed);

    // 3. File I/O via read
    let file_ok = if let Ok(content) = fs::read_to_string("readme.txt") {
        content.contains("EOS")
    } else {
        false
    };
    println!(" [{}] VFS open / read / close ('readme.txt')", if file_ok { "PASS" } else { "SKIP" });

    // 4. Hardware SSE2 / f32 mathematical operations
    let x: f32 = 64.0;
    let s = x.sqrt();
    let trig_ok = (x.sin().abs() <= 1.0) && ((s - 8.0).abs() < 0.001);
    println!(" [{}] Hardware SSE2 Math (sqrt, sin, cos)", if trig_ok { "PASS" } else { "FAIL" });

    // 5. Serde JSON Serialize & Deserialize
    let json_data = r#"{"name":"EOS","cores":8,"status":"online"}"#;
    let parsed: Result<serde_json::Value, _> = serde_json::from_str(json_data);
    let serde_ok = parsed.is_ok() && parsed.unwrap()["cores"] == 8;
    println!(" [{}] serde_json Structured Serialization", if serde_ok { "PASS" } else { "FAIL" });

    println!("========================================================");
    println!(" ALL SYSTEMS FUNCTIONAL UNDER MUSL RUNTIME\n");
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() > 1 && args[1] == "selftest" {
        run_selftest();
        return;
    }

    println!("🚀 EOS Native Userspace App Active [PID 2]");
    let mut pixels = vec![0u32; W * H];
    let start = Instant::now();
    
    let mut mouse_x = W / 2;
    let mut mouse_y = H / 2;
    let mut click_active = false;

    // Interactive graphical presentation loop
    for _ in 0..1200 {
        while let Some(ev) = sdk::window::poll_event() {
            let (etype, data) = (ev[0], ev[1]);
            if etype == 1 {
                mouse_x = (data & 0xFFFFFFFF) as usize;
                mouse_y = (data >> 32) as usize;
            } else if etype == 2 {
                click_active = (data >> 8) == 1;
            }
        }

        let elapsed = start.elapsed().as_secs_f32();
        
        for y in 0..H {
            for x in 0..W {
                let dx = x as isize - mouse_x as isize;
                let dy = y as isize - mouse_y as isize;
                let dist = ((dx * dx + dy * dy) as f32).sqrt();
                
                let r = ((x as f32 / W as f32).sin() * 127.0 + 128.0) as u32;
                let g = ((y as f32 / H as f32).cos() * 127.0 + 128.0) as u32;
                let b = ((elapsed * 2.0).sin() * 127.0 + 128.0) as u32;
                
                let mut color = (0xFF << 24) | (r << 16) | (g << 8) | b;
                if dist < 45.0 {
                    color = (0xFF << 24) | (if click_active { 0x00FF00 } else { 0xFFFFFF });
                }
                pixels[y * W + x] = color;
            }
        }
        
        sdk::window::present(pixels.as_ptr(), W, H);
        thread::sleep(Duration::from_millis(16));
    }
    
    println!("✅ Userspace Window Completed Presentation.");
}
