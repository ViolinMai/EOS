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
        if sub.is_empty() {
            return Ok(scan_shared_disk().unwrap_or_default().into_iter()
                .map(|f| if f.is_dir { FsItem::Directory(f.name, f.first_cluster) } else { FsItem::File(f.name, f.size as usize, f.media_type) }).collect());
        }
        if let Ok(files) = scan_shared_disk() {
            if let Some(dir) = files.iter().find(|x| x.is_dir && x.name.eq_ignore_ascii_case(sub)) {
                return Ok(scan_dir_cluster(1, dir.first_cluster).unwrap_or_default().into_iter()
                    .map(|f| if f.is_dir { FsItem::Directory(f.name, f.first_cluster) } else { FsItem::File(f.name, f.size as usize, f.media_type) }).collect());
            }
        }
    }
    
    let mut items = Vec::new();
    if p == "RootFS" {
        unsafe {
            if let Some(fs) = &*addr_of_mut!(EXT2_FS) {
                if let Ok(entries) = fs.list_directory(2) {
                    for (name, ino) in entries {
                        if let Ok(inode) = fs.read_inode(ino) {
                            let is_dir = (inode.mode & 0x4000) != 0;
                            if is_dir { items.push(FsItem::Directory(name, ino)); }
                            else { items.push(FsItem::File(name, inode.size as usize, MediaType::Text)); }
                        }
                    }
                }
            }
        }
    } else if p == "Initrd" {
        unsafe {
            if let Some(archive) = &*addr_of_mut!(INITRD) {
                for f in &archive.files {
                    items.push(FsItem::File(f.name.clone(), f.size, MediaType::Text));
                }
            }
        }
    } else { return Err("Directory not found"); }
    Ok(items)
}

pub fn vfs_stat(path: &str) -> Result<(u64, u32), &'static str> {
    let p = resolve_path(path);
    if p.is_empty() || p == "EOS SHARE" || p == "RootFS" || p == "Initrd" { return Ok((0, 0o040755)); }
    if let Ok(b) = vfs_read_bytes(&p) { return Ok((b.len() as u64, 0o100644)); }
    if vfs_list_dir(&p).is_ok() { return Ok((0, 0o040755)); }
    Err("Not found")
}

pub fn vfs_read_bytes(path: &str) -> Result<Vec<u8>, &'static str> {
    let p = resolve_path(path);
    unsafe {
        if let Some(fs) = &*addr_of_mut!(EXT2_FS) {
            if let Ok(entries) = fs.list_directory(2) {
                if let Some((_, ino)) = entries.iter().find(|(n, _)| n.eq_ignore_ascii_case(&p)) {
                    if let Ok(inode) = fs.read_inode(*ino) { return fs.read_file_data(&inode); }
                }
            }
        }
    }
    if let Ok(files) = scan_shared_disk() {
        if let Some(f) = files.iter().find(|x| x.name.eq_ignore_ascii_case(&p)) { return read_entire_file(1, f); }
        if p.contains('/') {
            let parts: Vec<&str> = p.split('/').collect();
            if let Some(sub) = files.iter().find(|x| x.is_dir && x.name.eq_ignore_ascii_case(parts[0])) {
                if let Ok(subfiles) = scan_dir_cluster(1, sub.first_cluster) {
                    if let Some(f) = subfiles.iter().find(|x| x.name.eq_ignore_ascii_case(parts[1])) { return read_entire_file(1, f); }
                }
            }
        }
    }
    unsafe {
        if let Some(archive) = &*addr_of_mut!(INITRD) {
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
