#![allow(dead_code)]
pub mod window {
    use core::arch::asm;

    pub fn present(pixels: *const u32, width: usize, height: usize) {
        unsafe {
            asm!(
                "syscall",
                in("rax") 502,
                in("rdi") pixels as u64,
                in("rsi") 0,
                in("rdx") 0,
                in("r10") width as u64,
                in("r8") height as u64,
                out("rcx") _, out("r11") _
            );
        }
    }

    pub fn present_rects(pixels: *const u32, width: usize, height: usize, rects: &[[u32; 4]]) {
        unsafe {
            asm!("syscall", in("rax") 504, in("rdi") pixels as u64, in("rsi") rects.as_ptr() as u64, in("rdx") rects.len() as u64, in("r10") width as u64, in("r8") height as u64, out("rcx") _, out("r11") _);
        }
    }

    pub fn poll_event() -> Option<[u64; 3]> {
        let type_val: u64;
        let data1: u64;
        let data2: u64;
        unsafe {
            asm!(
                "syscall",
                inout("rax") 503u64 => type_val,
                out("rdi") data1,
                out("rsi") data2,
                out("rcx") _, out("r11") _
            );
        }
        if type_val == 0 { None } else { Some([type_val, data1, data2]) }
    }

    pub fn wait_event(timeout_ms: u64) -> Option<[u64; 3]> {
        let type_val: u64;
        let data1: u64;
        let data2: u64;
        unsafe {
            asm!(
                "syscall",
                inout("rax") 505u64 => type_val,
                inout("rdi") timeout_ms => data1,
                out("rsi") data2,
                out("rcx") _, out("r11") _
            );
        }
        if type_val == 0 { None } else { Some([type_val, data1, data2]) }
    }
}

pub mod gfx {
    #[inline(always)]
    pub fn blend(bg: u32, fg: u32, alpha: u32) -> u32 {
        if alpha >= 255 { return fg | 0xFF000000; }
        if alpha == 0 { return bg | 0xFF000000; }
        let inv = 255 - alpha;
        let rb = (((fg & 0x00FF00FF) * alpha + (bg & 0x00FF00FF) * inv) >> 8) & 0x00FF00FF;
        let g = (((fg & 0x0000FF00) * alpha + (bg & 0x0000FF00) * inv) >> 8) & 0x0000FF00;
        0xFF000000 | rb | g
    }
}
pub use agent::*;
pub mod agent {
    use core::arch::asm;

    /// بروتوكول تحكم معزول لوكلاء الذكاء الاصطناعي (Agent Command Protocol)
    #[inline(always)]
    pub fn dispatch_agent_command(cmd_id: u32, arg_ptr: *const u8, len: usize) -> i64 {
        let ret: i64;
        unsafe {
            asm!(
                "syscall",
                inout("rax") 520i64 => ret,
                in("rdi") cmd_id as u64,
                in("rsi") arg_ptr as u64,
                in("rdx") len as u64,
                out("rcx") _, out("r11") _
            );
        }
        ret
    }

    pub fn audit_log(message: &str) {
        let bytes = message.as_bytes();
        let _ = dispatch_agent_command(3, bytes.as_ptr(), bytes.len());
    }

    pub fn trigger_kernel_audit() -> i64 {
        dispatch_agent_command(4, core::ptr::null(), 0)
    }

    pub fn query_kernel_heap_used() -> usize {
        dispatch_agent_command(6, core::ptr::null(), 0).max(0) as usize
    }
}

pub mod gpu {
    #[derive(Clone, Copy, Debug, Default)]
    pub struct GpuInfo {
        pub width: u32,
        pub height: u32,
        pub hw_scanout: bool,
        pub virgl_3d: bool,
        pub hw_cursor: bool,
        pub backend_id: u32, // 0=Software, 1=VirtIO-2D, 2=VirtIO-VirGL-3D
    }

    impl GpuInfo {
        pub fn backend_name(&self) -> &'static str {
            match self.backend_id {
                2 => "VirtIO-GPU + VirGL (OpenGL 3D)",
                1 => "VirtIO-GPU (2D Scanout)",
                _ => "Software Framebuffer (CPU)",
            }
        }
    }

    pub fn query_info() -> GpuInfo {
        let mut raw = [0u32; 6];
        if crate::syscall::sys_gpu_info(&mut raw) {
            GpuInfo {
                width: raw[0],
                height: raw[1],
                hw_scanout: raw[2] != 0,
                virgl_3d: raw[3] != 0,
                hw_cursor: raw[4] != 0,
                backend_id: raw[5],
            }
        } else {
            GpuInfo { width: 2560, height: 1440, ..Default::default() }
        }
    }

    pub fn submit_virgl_nop() -> bool {
        // VIRGL_CMD0(VIRGL_CCMD_NOP = 0, obj = 0, len = 0)
        let nop_cmd = [0u32];
        crate::syscall::sys_gpu_submit_3d(&nop_cmd)
    }
}

pub mod media {
    /// واجهة فك تشفير الصور الموحدة عبر Syscall 510 (مع Fallback آمن للصيغ الفرعية مثل Indexed PNG)
    pub fn decode_image(data: &[u8]) -> Result<(Vec<u32>, usize, usize, &'static str), String> {
        let mut meta = [0u32; 2];
        let probe = crate::syscall::sys_media_decode_image(data, core::ptr::null_mut(), 0, &mut meta);
        if probe == 0 && meta[0] > 0 && meta[1] > 0 {
            let w = meta[0] as usize;
            let h = meta[1] as usize;
            let mut pixels = vec![0u32; w * h];
            let fetch = crate::syscall::sys_media_decode_image(data, pixels.as_mut_ptr(), pixels.len(), &mut meta);
            if fetch == 0 {
                return Ok((pixels, w, h, "Kernel Syscall 510"));
            }
        }

        // Fallback للمستخدم في حال صيغة فرعية غير مدعومة في الكيرنل (مثل Indexed PNG)
        if data.len() >= 8 && &data[0..8] == &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A] {
            let (px, w, h) = crate::png::decode_png(data)?;
            return Ok((px, w, h, "Userspace PNG Fallback"));
        }
        Err("Image decode failed".to_string())
    }
}

pub mod sys {
    #[derive(Clone, Debug)]
    pub struct ProcessInfo {
        pub pid: u32,
        pub core_id: u32,
        pub name: String,
        pub mem_kb: u64,
        pub syscalls: u64,
        pub cpu_cycles: u64,
    }

    pub fn query_processes() -> Vec<ProcessInfo> {
        let mut raw = [crate::syscall::RawProcTelemetry::default(); 16];
        let count = crate::syscall::sys_proc_list(&mut raw);
        let mut out = Vec::with_capacity(count);
        for r in &raw[..count] {
            let nlen = r.name.iter().position(|&b| b == 0).unwrap_or(24);
            let name = String::from_utf8_lossy(&r.name[..nlen]).to_string();
            out.push(ProcessInfo {
                pid: r.pid,
                core_id: r.core_id,
                name,
                mem_kb: r.mem_bytes / 1024,
                syscalls: r.syscalls,
                cpu_cycles: r.cpu_cycles,
            });
        }
        out
    }
}

pub mod audio {
    pub fn is_available() -> bool {
        crate::syscall::sys_audio_is_active()
    }
    pub fn set_volume(vol_pct: u8) {
        crate::syscall::sys_audio_set_volume(vol_pct);
    }
    pub fn write_pcm_i16(samples: &[i16]) -> usize {
        if samples.is_empty() { return 0; }
        let bytes = unsafe { core::slice::from_raw_parts(samples.as_ptr() as *const u8, samples.len() * 2) };
        crate::syscall::sys_audio_write(bytes)
    }
}

/// طبقة تكامل الصوتيات والفيديو المدمجة في الـ Framework لأي تطبيق
pub mod media_session {
    use crate::ffi_media::FfmpegDecoder;

    pub struct EmbeddedPlayer {
        pub decoder: FfmpegDecoder,
        pub frame_buf: Vec<u32>,
        pub audio_buf: Vec<i16>,
        pub width: usize,
        pub height: usize,
        pub playing: bool,
    }

    impl EmbeddedPlayer {
        pub fn open(path: &str, target_w: usize, target_h: usize) -> Result<Self, String> {
            let mut decoder = FfmpegDecoder::open(path)?;
            let mut frame_buf = vec![0xFF000000; target_w * target_h];
            let _ = decoder.next_frame(&mut frame_buf, target_w, target_h);
            Ok(Self {
                decoder,
                frame_buf,
                audio_buf: vec![0i16; 8192],
                width: target_w,
                height: target_h,
                playing: true,
            })
        }

        pub fn step(&mut self) -> bool {
            if !self.playing { return false; }
            if let Ok(Some(_)) = self.decoder.next_frame(&mut self.frame_buf, self.width, self.height) {
                let n = self.decoder.pop_audio(&mut self.audio_buf);
                if n > 0 {
                    crate::sdk::audio::write_pcm_i16(&self.audio_buf[..n]);
                }
                return true;
            }
            false
        }
    }
}
