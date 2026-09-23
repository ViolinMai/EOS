use alloc::string::String;
use alloc::vec::Vec;
use crate::log_info;
use core::ptr::addr_of_mut;

#[derive(Clone, Debug)]
pub struct TarFile {
    pub name: String,
    pub size: usize,
    pub data_ptr: *const u8,
}

pub struct TarArchive {
    pub files: Vec<TarFile>,
}

pub static mut INITRD: Option<TarArchive> = None;

fn parse_octal(slice: &[u8]) -> usize {
    let mut n = 0;
    for &b in slice {
        if b < b'0' || b > b'7' {
            if b == 0 || b == b' ' {
                continue;
            }
            break;
        }
        n = (n << 3) | ((b - b'0') as usize);
    }
    n
}

impl TarArchive {
    pub unsafe fn parse(base_ptr: *const u8, total_size: usize) -> Self {
        let mut files = Vec::new();
        let mut offset = 0;

        while offset + 512 <= total_size {
            let header = unsafe { base_ptr.add(offset) };

            if unsafe { *header } == 0 {
                break;
            }

            let name_slice = unsafe { core::slice::from_raw_parts(header, 100) };
            let name_len = name_slice.iter().position(|&c| c == 0).unwrap_or(100);
            let mut name = core::str::from_utf8(&name_slice[..name_len]).unwrap_or("unknown").trim();
            if let Some(stripped) = name.strip_prefix("./") {
                name = stripped;
            }

            let size_slice = unsafe { core::slice::from_raw_parts(header.add(124), 12) };
            let size = parse_octal(size_slice);

            let data_ptr = unsafe { header.add(512) };

            if !name.is_empty() && size > 0 {
                files.push(TarFile {
                    name: String::from(name),
                    size,
                    data_ptr,
                });
            }

            let blocks = (size + 511) / 512;
            offset += 512 + (blocks * 512);
        }

        Self { files }
    }

    #[allow(dead_code)]
    pub fn read_file(&self, filename: &str) -> Option<&'static str> {
        let clean_name = filename.trim().strip_prefix("./").unwrap_or(filename.trim());
        for file in &self.files {
            if file.name == clean_name {
                unsafe {
                    let slice = core::slice::from_raw_parts(file.data_ptr, file.size);
                    return core::str::from_utf8(slice).ok();
                }
            }
        }
        None
    }
}

pub unsafe fn init(base_ptr: *const u8, total_size: usize) {
    let archive = unsafe { TarArchive::parse(base_ptr, total_size) };
    log_info!("TarFS: Ramdisk mounted ({} files found).", archive.files.len());

    unsafe {
        if let Some(icon) = archive.files.iter().find(|f| f.name == "icon.png") {
            let bytes = core::slice::from_raw_parts(icon.data_ptr, 33);
            log_info!("icon.png[16..20] (width)  = {:02x} {:02x} {:02x} {:02x}",
                bytes[16], bytes[17], bytes[18], bytes[19]);
            log_info!("icon.png[20..24] (height) = {:02x} {:02x} {:02x} {:02x}",
                bytes[20], bytes[21], bytes[22], bytes[23]);
            log_info!("icon.png[24] (depth) = {}, [25] (color_type) = {}, [26] (compression) = {}",
                bytes[24], bytes[25], bytes[26]);
        }
    }

    unsafe {
        *addr_of_mut!(INITRD) = Some(archive);
    }
}
