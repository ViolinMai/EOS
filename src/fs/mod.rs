pub mod elf;
pub mod ext2;
pub mod tar;

use alloc::string::String;
use alloc::vec::Vec;
use crate::drivers::ata::{read_entire_file, scan_shared_disk};
use crate::fs::ext2::EXT2_FS;
use crate::fs::tar::INITRD;
use core::ptr::addr_of_mut;

#[derive(Clone, Debug)]
pub enum VfsNode {
    Ext2(String, usize),
    Disk(String, u32),
    Ramdisk(String, usize),
}

// 💡 طبقة الـ VFS الموحدة: مسح جميع أنظمة الملفات وعرضها كشجرة واحدة
pub fn vfs_list_all() -> Vec<VfsNode> {
    let mut nodes = Vec::new();

    // 1. استعراض نظام الملفات الأصلي Linux ext2
    unsafe {
        if let Some(fs) = &*addr_of_mut!(EXT2_FS) {
            if let Ok(entries) = fs.list_directory(2) { // Inode 2 هو Root Directory
                for (name, ino) in entries {
                    if let Ok(inode) = fs.read_inode(ino) {
                        nodes.push(VfsNode::Ext2(name, inode.size as usize));
                    }
                }
            }
        }
    }

    // 2. استعراض ملفات القرص المشترك FAT32
    if let Ok(files) = scan_shared_disk() {
        for f in files {
            nodes.push(VfsNode::Disk(f.name.clone(), f.size));
        }
    }

    // 3. استعراض ملفات الـ Ramdisk TarFS
    unsafe {
        if let Some(archive) = &*addr_of_mut!(INITRD) {
            for f in &archive.files {
                nodes.push(VfsNode::Ramdisk(f.name.clone(), f.size));
            }
        }
    }

    nodes
}

// 💡 قراءة الملفات عبر المسار المعياري مع دعم ext2
pub fn vfs_read_bytes(path: &str) -> Result<Vec<u8>, &'static str> {
    let clean = path.trim().strip_prefix("/").unwrap_or(path.trim());

    // 1. البحث في ext2 RootFS أولاً
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

    // 2. البحث في قرص FAT32 المشترك
    if let Ok(files) = scan_shared_disk() {
        if let Some(f) = files.iter().find(|x| x.name.eq_ignore_ascii_case(clean)) {
            return read_entire_file(1, f);
        }
    }

    // 3. البحث في الـ Ramdisk
    unsafe {
        if let Some(archive) = &*addr_of_mut!(INITRD) {
            if let Some(f) = archive.files.iter().find(|x| x.name == clean) {
                let slice = core::slice::from_raw_parts(f.data_ptr, f.size);
                return Ok(slice.to_vec());
            }
        }
    }

    Err("File not found in any mounted filesystem (ext2/FAT32/TarFS)")
}
