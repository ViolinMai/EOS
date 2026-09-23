pub mod elf;
pub mod ext2;
pub mod image;
pub mod tar;

use alloc::string::String;
use alloc::vec::Vec;
use crate::drivers::ata::{read_entire_file, scan_shared_disk, write_file_content};
use crate::fs::ext2::EXT2_FS;
use crate::fs::tar::INITRD;
use core::ptr::addr_of_mut;

#[derive(Clone, Debug)]
pub enum FsItem {
    Directory(String),
    File(String, usize),
}

// 💡 متصفح الملفات بنظام المجلدات الحقيقية
pub fn list_directory_contents(folder: &str) -> Vec<FsItem> {
    let mut items = Vec::new();

    match folder {
        "" => {
            // المجلد الرئيسي (Root) يعرض الأقراص والمجلدات الأساسية
            items.push(FsItem::Directory(String::from("Storage")));
            items.push(FsItem::Directory(String::from("RootFS")));
            items.push(FsItem::Directory(String::from("Initrd")));
        }
        "Storage" => {
            // محتويات قرص الويندوز المشترك C:\EOS_SHARE
            if let Ok(files) = scan_shared_disk() {
                for f in files {
                    items.push(FsItem::File(f.name, f.size as usize));
                }
            }
        }
        "RootFS" => {
            // محتويات قرص Linux ext2 الداخلي
            unsafe {
                if let Some(fs) = &*addr_of_mut!(EXT2_FS) {
                    if let Ok(entries) = fs.list_directory(2) {
                        for (name, ino) in entries {
                            if let Ok(inode) = fs.read_inode(ino) {
                                items.push(FsItem::File(name, inode.size as usize));
                            }
                        }
                    }
                }
            }
        }
        "Initrd" => {
            // محتويات قرص الـ Ramdisk الأساسي للنظام
            unsafe {
                if let Some(archive) = &*addr_of_mut!(INITRD) {
                    for f in &archive.files {
                        items.push(FsItem::File(f.name.clone(), f.size));
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
            let l = f.name.to_ascii_lowercase();
            if l.ends_with(".png") || l.ends_with(".jpg") || l.ends_with(".jpeg") {
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

    // 1. فحص ext2 RootFS
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

    // 2. فحص قرص التخزين المشترك FAT32
    if let Ok(files) = scan_shared_disk() {
        if let Some(f) = files.iter().find(|x| x.name.eq_ignore_ascii_case(clean)) {
            return read_entire_file(1, f);
        }
    }

    // 3. فحص الـ Ramdisk
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
