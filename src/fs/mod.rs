pub mod elf;
pub mod ext2;
pub mod image;
pub mod tar;

use alloc::string::String;
use alloc::vec::Vec;
use crate::drivers::ata::{read_entire_file, scan_shared_disk, scan_dir_cluster, write_file_content, MediaType};
use crate::fs::ext2::EXT2_FS;
use crate::fs::tar::INITRD;
use core::ptr::addr_of_mut;

#[derive(Clone, Debug)]
pub enum FsItem {
    Directory(String, u32), // Name + Cluster
    File(String, usize, MediaType),
}

pub fn list_directory_contents(folder: &str, dir_cluster: u32) -> Vec<FsItem> {
    let mut items = Vec::new();

    if folder == "" {
        items.push(FsItem::Directory(String::from("EOS SHARE"), 0));
        items.push(FsItem::Directory(String::from("RootFS"), 0));
        items.push(FsItem::Directory(String::from("Initrd"), 0));
        return items;
    }

    if folder.starts_with("EOS SHARE") {
        if dir_cluster >= 2 {
            if let Ok(files) = scan_dir_cluster(1, dir_cluster) {
                for f in files {
                    if f.is_dir {
                        items.push(FsItem::Directory(f.name, f.first_cluster));
                    } else {
                        items.push(FsItem::File(f.name, f.size as usize, f.media_type));
                    }
                }
            }
        } else if let Ok(files) = scan_shared_disk() {
            for f in files {
                if f.is_dir {
                    items.push(FsItem::Directory(f.name, f.first_cluster));
                } else {
                    items.push(FsItem::File(f.name, f.size as usize, f.media_type));
                }
            }
        }
        return items;
    }

    match folder {
        "RootFS" => {
            unsafe {
                if let Some(fs) = &*addr_of_mut!(EXT2_FS) {
                    if let Ok(entries) = fs.list_directory(2) {
                        for (name, ino) in entries {
                            if let Ok(inode) = fs.read_inode(ino) {
                                let is_dir = (inode.mode & 0x4000) != 0;
                                if is_dir {
                                    items.push(FsItem::Directory(name, ino));
                                } else {
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

    // Check shared FAT32 storage disk
    if let Ok(files) = scan_shared_disk() {
        if let Some(f) = files.iter().find(|x| x.name.eq_ignore_ascii_case(clean)) {
            return read_entire_file(1, f);
        }
        if clean.contains('/') {
            let parts: Vec<&str> = clean.split('/').collect();
            if let Some(sub) = files.iter().find(|x| x.is_dir && x.name.eq_ignore_ascii_case(parts[0])) {
                if let Ok(subfiles) = scan_dir_cluster(1, sub.first_cluster) {
                    if let Some(f) = subfiles.iter().find(|x| x.name.eq_ignore_ascii_case(parts[1])) {
                        return read_entire_file(1, f);
                    }
                }
            }
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
