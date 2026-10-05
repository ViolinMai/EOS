pub mod elf;
pub mod ext2;
pub mod image;
pub mod tar;

use alloc::string::String;
use alloc::vec::Vec;
use crate::drivers::ata::{read_entire_file, write_file_content, MediaType};
use core::ptr::addr_of_mut;

#[derive(Clone, Debug)]
pub enum FsItem {
    Directory(String, u32),
    File(String, usize, MediaType),
}

pub fn resolve_path(path: &str) -> String {
    let mut p = path.trim().replace('\\', "/");
    while p.starts_with("./") { p = String::from(&p[2..]); }
    while p.starts_with('/') { p = String::from(&p[1..]); }
    p
}

pub fn vfs_list_dir(path: &str) -> Result<Vec<FsItem>, &'static str> {
    let p = resolve_path(path);
    if p.is_empty() {
        return Ok(alloc::vec![
            FsItem::Directory(String::from("EOS SHARE"), 0),
            FsItem::Directory(String::from("RootFS"), 0),
            FsItem::Directory(String::from("Initrd"), 0)
        ]);
    }
    if p.starts_with("EOS SHARE") {
        let sub = p.strip_prefix("EOS SHARE").unwrap().trim_matches('/');
        return FAT_FS.list_dir(sub);
    }

    let mut items = Vec::new();
    if p == "RootFS" {
        unsafe {
            if let Some(fs) = &*core::ptr::addr_of_mut!(crate::fs::ext2::EXT2_FS) {
                if let Ok(entries) = fs.list_directory(2) {
                    for (name, ino) in entries {
                        if let Ok(inode) = fs.read_inode(ino) {
                            let is_dir = (inode.mode & 0x4000) != 0;
                            if is_dir { items.push(FsItem::Directory(name, ino)); }
                            else { items.push(FsItem::File(name, inode.size as usize, crate::drivers::ata::MediaType::Text)); }
                        }
                    }
                }
            }
        }
    } else if p == "Initrd" {
        unsafe {
            if let Some(archive) = &*core::ptr::addr_of_mut!(crate::fs::tar::INITRD) {
                for f in &archive.files {
                    items.push(FsItem::File(f.name.clone(), f.size, crate::drivers::ata::MediaType::Text));
                }
            }
        }
    } else { return Err("Directory not found"); }
    Ok(items)
}

pub fn vfs_stat(path: &str) -> Result<(u64, u32), &'static str> {
    let p = resolve_path(path);
    if p.is_empty() || p == "EOS SHARE" || p == "RootFS" || p == "Initrd" {
        return Ok((0, 0o040755));
    }
    if p.starts_with("EOS SHARE") || !p.starts_with('/') {
        let clean = p.strip_prefix("EOS SHARE").unwrap_or(&p).trim_matches('/');
        if let Ok(res) = FAT_FS.stat(clean) {
            return Ok(res);
        }
    }
    if let Ok(b) = vfs_read_bytes(&p) { return Ok((b.len() as u64, 0o100644)); }
    if vfs_list_dir(&p).is_ok() { return Ok((0, 0o040755)); }
    Err("Not found")
}

pub fn vfs_read_bytes(path: &str) -> Result<Vec<u8>, &'static str> {
    let p = resolve_path(path);
    unsafe {
        if let Some(fs) = &*addr_of_mut!(crate::fs::ext2::EXT2_FS) {
            if let Ok(entries) = fs.list_directory(2) {
                if let Some((_, ino)) = entries.iter().find(|(n, _)| n.eq_ignore_ascii_case(&p)) {
                    if let Ok(inode) = fs.read_inode(*ino) { return fs.read_file_data(&inode); }
                }
            }
        }
    }
    if p.starts_with("EOS SHARE") || !p.starts_with('/') {
        let clean = p.strip_prefix("EOS SHARE").unwrap_or(&p).trim_matches('/');
        if let Ok(bytes) = FAT_FS.read_bytes(clean) {
            return Ok(bytes);
        }
    }
    unsafe {
        if let Some(archive) = &*addr_of_mut!(crate::fs::tar::INITRD) {
            if let Some(f) = archive.files.iter().find(|x| x.name.eq_ignore_ascii_case(&p)) {
                return Ok(core::slice::from_raw_parts(f.data_ptr, f.size).to_vec());
            }
        }
    }
    Err("File not found")
}

pub fn vfs_save_text_file(filename: &str, content: &[u8]) -> Result<(), &'static str> {
    write_file_content(1, filename, content)
}

pub fn list_directory_contents(folder: &str, _dir_cluster: u32) -> Vec<FsItem> {
    vfs_list_dir(folder).unwrap_or_default()
}

pub trait FileSystem: Send + Sync {
    fn read_bytes(&self, path: &str) -> Result<Vec<u8>, &'static str>;
    fn list_dir(&self, path: &str) -> Result<Vec<FsItem>, &'static str>;
    fn stat(&self, path: &str) -> Result<(u64, u32), &'static str>;
}

pub struct FatFileSystem {
    pub drive: u8,
}

impl FileSystem for FatFileSystem {
    fn read_bytes(&self, path: &str) -> Result<Vec<u8>, &'static str> {
        let clean = path.trim_matches('/');
        let layout = crate::drivers::ata::get_fat_layout(self.drive)?;
        let mut curr_cluster = if let crate::drivers::ata::FatType::Fat32 = layout.fat_type { layout.root_cluster } else { 0 };
        let parts: Vec<&str> = clean.split('/').filter(|s| !s.is_empty()).collect();
        if parts.is_empty() { return Err("Invalid path"); }
        for i in 0..parts.len().saturating_sub(1) {
            let files = crate::drivers::ata::scan_dir_cluster(self.drive, curr_cluster)?;
            if let Some(dir) = files.iter().find(|x| x.is_dir && x.name.eq_ignore_ascii_case(parts[i])) {
                curr_cluster = dir.first_cluster;
            } else {
                return Err("Path segment not found");
            }
        }
        let files = crate::drivers::ata::scan_dir_cluster(self.drive, curr_cluster)?;
        if let Some(f) = files.iter().find(|x| !x.is_dir && x.name.eq_ignore_ascii_case(parts.last().unwrap())) {
            read_entire_file(self.drive, f)
        } else {
            Err("File not found on FAT disk")
        }
    }

    fn list_dir(&self, path: &str) -> Result<Vec<FsItem>, &'static str> {
        let clean = path.trim_matches('/');
        let layout = crate::drivers::ata::get_fat_layout(self.drive)?;
        let mut curr_cluster = if let crate::drivers::ata::FatType::Fat32 = layout.fat_type { layout.root_cluster } else { 0 };
        if !clean.is_empty() {
            for part in clean.split('/').filter(|s| !s.is_empty()) {
                let files = crate::drivers::ata::scan_dir_cluster(self.drive, curr_cluster)?;
                if let Some(dir) = files.iter().find(|x| x.is_dir && x.name.eq_ignore_ascii_case(part)) {
                    curr_cluster = dir.first_cluster;
                } else {
                    return Err("Directory not found in FAT");
                }
            }
        }
        Ok(crate::drivers::ata::scan_dir_cluster(self.drive, curr_cluster).unwrap_or_default().into_iter()
            .map(|f| if f.is_dir { FsItem::Directory(f.name, f.first_cluster) } else { FsItem::File(f.name, f.size as usize, f.media_type) }).collect())
    }

    fn stat(&self, path: &str) -> Result<(u64, u32), &'static str> {
        let clean = path.trim_matches('/');
        if clean.is_empty() { return Ok((0, 0o040755)); }
        let layout = crate::drivers::ata::get_fat_layout(self.drive)?;
        let mut curr_cluster = if let crate::drivers::ata::FatType::Fat32 = layout.fat_type { layout.root_cluster } else { 0 };
        let parts: Vec<&str> = clean.split('/').filter(|s| !s.is_empty()).collect();
        for i in 0..parts.len().saturating_sub(1) {
            let files = crate::drivers::ata::scan_dir_cluster(self.drive, curr_cluster)?;
            if let Some(dir) = files.iter().find(|x| x.is_dir && x.name.eq_ignore_ascii_case(parts[i])) {
                curr_cluster = dir.first_cluster;
            }
        }
        let files = crate::drivers::ata::scan_dir_cluster(self.drive, curr_cluster)?;
        if let Some(target) = files.iter().find(|x| x.name.eq_ignore_ascii_case(parts.last().unwrap())) {
            let mode = if target.is_dir { 0o040755 } else { 0o100644 };
            Ok((target.size as u64, mode))
        } else {
            Err("Not found on FAT disk")
        }
    }
}

pub static FAT_FS: FatFileSystem = FatFileSystem { drive: 1 };
