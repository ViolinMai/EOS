use std::ffi::CString;

#[cfg(eos_ffmpeg)]
#[repr(C)]
struct EosVideoCtx {
    _private: [u8; 0],
}

#[cfg(eos_ffmpeg)]
extern "C" {
    fn eos_video_open(path: *const i8) -> *mut EosVideoCtx;
    fn eos_video_get_info(
        ctx: *mut EosVideoCtx,
        w: *mut i32,
        h: *mut i32,
        fps: *mut f64,
        dur_ms: *mut i64,
        codec_buf: *mut i8,
        buf_len: i32,
    );
    fn eos_video_get_extended_info(
        ctx: *mut EosVideoCtx,
        v_bitrate: *mut i64,
        bit_depth: *mut i32,
        a_sr: *mut i32,
        a_ch: *mut i32,
        a_bitrate: *mut i64,
        pix_fmt_buf: *mut i8,
        cs_buf: *mut i8,
        cr_buf: *mut i8,
        acodec_buf: *mut i8,
        buf_len: i32,
    );
    fn eos_video_seek_ms(ctx: *mut EosVideoCtx, target_ms: i64) -> i32;
    fn eos_video_next_frame(
        ctx: *mut EosVideoCtx,
        out_argb: *mut u32,
        dst_w: i32,
        dst_h: i32,
        out_pts_ms: *mut i64,
    ) -> i32;
    fn eos_video_pop_audio(ctx: *mut EosVideoCtx, out_pcm: *mut i16, max_samples: i32) -> i32;
    fn eos_video_close(ctx: *mut EosVideoCtx);
}

#[derive(Clone, Debug)]
pub struct VideoMeta {
    pub width: usize,
    pub height: usize,
    pub fps: f64,
    pub duration_ms: i64,
    pub codec: String,
    pub video_bitrate_kbps: i64,
    pub bit_depth: u8,
    pub pix_fmt: String,
    pub color_space: String,
    pub color_range: String,
    pub audio_codec: String,
    pub audio_sample_rate: i32,
    pub audio_channels: i32,
    pub audio_bitrate_kbps: i64,
}

pub struct FfmpegDecoder {
    #[cfg(eos_ffmpeg)]
    ctx: *mut EosVideoCtx,
    pub meta: VideoMeta,
}

impl FfmpegDecoder {
    pub fn is_available() -> bool {
        cfg!(eos_ffmpeg)
    }

    #[cfg(eos_ffmpeg)]
    pub fn open(path: &str) -> Result<Self, String> {
        let c_path = CString::new(path).map_err(|_| "Invalid path".to_string())?;
        let ctx = unsafe { eos_video_open(c_path.as_ptr()) };
        if ctx.is_null() {
            return Err(format!("FFmpeg could not open '{}'", path));
        }

        let mut w = 0i32;
        let mut h = 0i32;
        let mut fps = 30.0f64;
        let mut dur_ms = 0i64;
        let mut codec_buf = [0i8; 64];
        unsafe {
            eos_video_get_info(ctx, &mut w, &mut h, &mut fps, &mut dur_ms, codec_buf.as_mut_ptr(), 64);
        }
        let cstr_to_string = |buf: &[i8; 64]| -> String {
            let bytes: Vec<u8> = buf.iter().take_while(|&&b| b != 0).map(|&b| b as u8).collect();
            String::from_utf8_lossy(&bytes).to_string()
        };
        let codec = cstr_to_string(&codec_buf).to_uppercase();

        let mut v_br = 0i64;
        let mut b_depth = 8i32;
        let mut a_sr = 0i32;
        let mut a_ch = 0i32;
        let mut a_br = 0i64;
        let mut pfmt_buf = [0i8; 64];
        let mut cs_buf = [0i8; 64];
        let mut cr_buf = [0i8; 64];
        let mut ac_buf = [0i8; 64];

        unsafe {
            eos_video_get_extended_info(
                ctx,
                &mut v_br,
                &mut b_depth,
                &mut a_sr,
                &mut a_ch,
                &mut a_br,
                pfmt_buf.as_mut_ptr(),
                cs_buf.as_mut_ptr(),
                cr_buf.as_mut_ptr(),
                ac_buf.as_mut_ptr(),
                64,
            );
        }

        Ok(Self {
            ctx,
            meta: VideoMeta {
                width: w.max(1) as usize,
                height: h.max(1) as usize,
                fps: if fps > 0.1 { fps } else { 30.0 },
                duration_ms: dur_ms.max(0),
                codec,
                video_bitrate_kbps: (v_br / 1000).max(0),
                bit_depth: b_depth.clamp(8, 16) as u8,
                pix_fmt: cstr_to_string(&pfmt_buf),
                color_space: cstr_to_string(&cs_buf),
                color_range: cstr_to_string(&cr_buf),
                audio_codec: cstr_to_string(&ac_buf).to_uppercase(),
                audio_sample_rate: a_sr.max(0),
                audio_channels: a_ch.max(0),
                audio_bitrate_kbps: (a_br / 1000).max(0),
            },
        })
    }

    #[cfg(not(eos_ffmpeg))]
    pub fn open(_path: &str) -> Result<Self, String> {
        Err("FFmpeg static libs not linked".into())
    }

    #[cfg(eos_ffmpeg)]
    pub fn next_frame(&mut self, out_argb: &mut [u32], dst_w: usize, dst_h: usize) -> Result<Option<i64>, String> {
        if out_argb.len() < dst_w * dst_h {
            return Err("Output buffer too small".into());
        }
        let mut pts_ms = 0i64;
        let res = unsafe {
            eos_video_next_frame(self.ctx, out_argb.as_mut_ptr(), dst_w as i32, dst_h as i32, &mut pts_ms)
        };
        match res {
            1 => Ok(Some(pts_ms)),
            0 => Ok(None), // EOF
            _ => Err("FFmpeg decode error".into()),
        }
    }

    #[cfg(not(eos_ffmpeg))]
    pub fn next_frame(&mut self, _out_argb: &mut [u32], _dst_w: usize, _dst_h: usize) -> Result<Option<i64>, String> {
        Ok(None)
    }

    #[cfg(eos_ffmpeg)]
    pub fn seek_ms(&mut self, target_ms: i64) -> bool {
        unsafe { eos_video_seek_ms(self.ctx, target_ms) == 0 }
    }

    #[cfg(not(eos_ffmpeg))]
    pub fn seek_ms(&mut self, _target_ms: i64) -> bool {
        false
    }

    #[cfg(eos_ffmpeg)]
    pub fn pop_audio(&mut self, out: &mut [i16]) -> usize {
        let n = unsafe { eos_video_pop_audio(self.ctx, out.as_mut_ptr(), out.len() as i32) };
        if n > 0 { n as usize } else { 0 }
    }

    #[cfg(not(eos_ffmpeg))]
    pub fn pop_audio(&mut self, _out: &mut [i16]) -> usize {
        0
    }
}

impl Drop for FfmpegDecoder {
    fn drop(&mut self) {
        #[cfg(eos_ffmpeg)]
        unsafe {
            if !self.ctx.is_null() {
                eos_video_close(self.ctx);
                self.ctx = core::ptr::null_mut();
            }
        }
    }
}
