pub mod elf;
pub mod ext2;
pub mod image;
pub mod tar;

use alloc::string::String;
use alloc::vec::Vec;
use crate::drivers::ata::{read_entire_file, scan_shared_disk, write_file_content, MediaType};
use crate::fs::ext2::EXT2_FS;
use crate::fs::tar::INITRD;
use core::ptr::addr_of_mut;

#[derive(Clone, Debug)]
pub enum FsItem {
    Directory(String),
    File(String, usize, MediaType),
}

pub fn list_directory_contents(folder: &str) -> Vec<FsItem> {
    let mut items = Vec::new();

    match folder {
        "" => {
            items.push(FsItem::Directory(String::from("Storage")));
            items.push(FsItem::Directory(String::from("RootFS")));
            items.push(FsItem::Directory(String::from("Initrd")));
        }
        "Storage" => {
            if let Ok(files) = scan_shared_disk() {
                for f in files {
                    items.push(FsItem::File(f.name, f.size as usize, f.media_type));
                }
            }
        }
        "RootFS" => {
            unsafe {
                if let Some(fs) = &*addr_of_mut!(EXT2_FS) {
                    if let Ok(entries) = fs.list_directory(2) {
                        for (name, ino) in entries {
                            if let Ok(inode) = fs.read_inode(ino) {
                                let lower = name.to_ascii_lowercase();
                                let m_type = if lower.ends_with(".png") || lower.ends_with(".jpg") {
                                    MediaType::Image
                                } else if lower.ends_with(".elf") {
                                    MediaType::Executable
                                } else {
                                    MediaType::Text
                                };
                                items.push(FsItem::File(name, inode.size as usize, m_type));
                            }
                        }
                    }
                }
            }
        }
        "Initrd" => {
            unsafe {
                if let Some(archive) = &*addr_of_mut!(INITRD) {
                    for f in &archive.files {
                        let lower = f.name.to_ascii_lowercase();
                        let m_type = if lower.ends_with(".png") || lower.ends_with(".jpg") {
                            MediaType::Image
                        } else if lower.ends_with(".elf") {
                            MediaType::Executable
                        } else {
                            MediaType::Text
                        };
                        items.push(FsItem::File(f.name.clone(), f.size, m_type));
                    }
                }
            }
        }
        _ => {}
    }

    items
}

pub fn vfs_list_all_images() -> Vec<String> {
    let mut list = Vec::new();
    if let Ok(files) = scan_shared_disk() {
        for f in files {
            if f.media_type == MediaType::Image {
                list.push(f.name);
            }
        }
    }
    unsafe {
        if let Some(archive) = &*addr_of_mut!(INITRD) {
            for f in &archive.files {
                let l = f.name.to_ascii_lowercase();
                if l.ends_with(".png") || l.ends_with(".jpg") || l.ends_with(".jpeg") {
                    list.push(f.name.clone());
                }
            }
        }
    }
    list
}

pub fn vfs_read_bytes(path: &str) -> Result<Vec<u8>, &'static str> {
    let clean = path.trim().strip_prefix("/").unwrap_or(path.trim());

    unsafe {
        if let Some(fs) = &*addr_of_mut!(EXT2_FS) {
            if let Ok(entries) = fs.list_directory(2) {
                if let Some((_, ino)) = entries.iter().find(|(name, _)| name.eq_ignore_ascii_case(clean)) {
                    if let Ok(inode) = fs.read_inode(*ino) {
                        return fs.read_file_data(&inode);
                    }
                }
            }
        }
    }

    if let Ok(files) = scan_shared_disk() {
        if let Some(f) = files.iter().find(|x| x.name.eq_ignore_ascii_case(clean)) {
            return read_entire_file(1, f);
        }
    }

    unsafe {
        if let Some(archive) = &*addr_of_mut!(INITRD) {
            if let Some(f) = archive.files.iter().find(|x| x.name.eq_ignore_ascii_case(clean)) {
                let slice = core::slice::from_raw_parts(f.data_ptr, f.size);
                return Ok(slice.to_vec());
            }
        }
    }

    Err("File not found")
}

pub fn vfs_save_text_file(filename: &str, content: &[u8]) -> Result<(), &'static str> {
    write_file_content(1, filename, content)
}
