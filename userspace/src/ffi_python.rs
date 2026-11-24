use std::ffi::{CStr, CString};
use std::fs;
use std::os::raw::{c_char, c_int};

#[cfg(eos_cpython)]
extern "C" {
    fn eos_py_init() -> c_int;
    fn eos_py_run_code(code: *const c_char, out_buf: *mut c_char, out_cap: c_int) -> c_int;
}

#[no_mangle]
pub extern "C" fn eos_cb_spawn(app_name: *const c_char) -> c_int {
    if app_name.is_null() { return -1; }
    let name = unsafe { CStr::from_ptr(app_name).to_string_lossy() };
    let slug = name.to_ascii_lowercase().replace(' ', "_");
    let args = format!("eos_{}.elf --app {}", slug, name.replace(' ', "_"));
    crate::syscall::sys_spawn_elf("user_app.elf", &args) as c_int
}

#[no_mangle]
pub extern "C" fn eos_cb_notify(title: *const c_char, msg: *const c_char) {
    if title.is_null() || msg.is_null() { return; }
    let t = unsafe { CStr::from_ptr(title).to_string_lossy() };
    let m = unsafe { CStr::from_ptr(msg).to_string_lossy() };
    crate::framework::portal::push_toast(&t, &m);
}

#[no_mangle]
pub extern "C" fn eos_cb_clipboard_set(text: *const c_char) {
    if text.is_null() { return; }
    let s = unsafe { CStr::from_ptr(text).to_string_lossy() };
    let _ = crate::syscall::sys_clipboard_set(&s);
}

#[no_mangle]
pub extern "C" fn eos_cb_clipboard_get(out_buf: *mut c_char, max_len: c_int) -> c_int {
    if out_buf.is_null() || max_len <= 0 { return 0; }
    let clip = crate::syscall::sys_clipboard_get();
    let bytes = clip.as_bytes();
    let n = bytes.len().min(max_len as usize);
    unsafe {
        core::ptr::copy_nonoverlapping(bytes.as_ptr(), out_buf as *mut u8, n);
    }
    n as c_int
}

#[no_mangle]
pub extern "C" fn eos_cb_audit(msg: *const c_char) {
    if msg.is_null() { return; }
    let s = unsafe { CStr::from_ptr(msg).to_string_lossy() };
    crate::sdk::agent::audit_log(&s);
}

pub fn is_available() -> bool {
    cfg!(eos_cpython)
}

pub fn init_runtime() -> bool {
    #[cfg(eos_cpython)]
    {
        unsafe { eos_py_init() == 0 }
    }
    #[cfg(not(eos_cpython))]
    {
        false
    }
}

pub fn run_code(code: &str) -> Result<String, String> {
    #[cfg(eos_cpython)]
    {
        let c_code = CString::new(code).map_err(|_| "Null byte in Python code".to_string())?;
        let mut out_buf = vec![0u8; 16384];
        let rc = unsafe {
            eos_py_run_code(
                c_code.as_ptr(),
                out_buf.as_mut_ptr() as *mut c_char,
                out_buf.len() as c_int,
            )
        };
        let end = out_buf.iter().position(|&b| b == 0).unwrap_or(out_buf.len());
        let output = String::from_utf8_lossy(&out_buf[..end]).to_string();
        if rc == 0 {
            Ok(output)
        } else {
            Err(if output.is_empty() {
                format!("CPython exited with code {}", rc)
            } else {
                output
            })
        }
    }
    #[cfg(not(eos_cpython))]
    {
        let _ = code;
        Err("CPython 3.12 static library not linked yet. Run `./build_cpython_musl.sh` on host, then rebuild EOS.".into())
    }
}

pub fn run_file(path: &str) -> Result<String, String> {
    let candidates = [
        path.to_string(),
        format!("/{}", path.trim_start_matches('/')),
    ];
    for p in &candidates {
        if let Ok(src) = fs::read_to_string(p) {
            return run_code(&src);
        }
    }
    Err(format!("python: can't open file '{}': No such file", path))
}
