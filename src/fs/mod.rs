pub mod elf;
pub mod ext2;
pub mod image;
pub mod tar;

use alloc::string::String;
use alloc::vec::Vec;
use crate::drivers::ata::{read_entire_file, write_file_content, MediaType};

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
    let clean = p.trim_matches('/');

    crate::log_info!("VFS", "vfs_list_dir requested for path: '{}'", clean);

    if clean.is_empty() || clean == "." {
        crate::log_info!("VFS", "Listing Root Virtual Mounts (EOS SHARE, RootFS, Initrd)");
        return Ok(alloc::vec![
            FsItem::Directory(alloc::string::String::from("EOS SHARE"), 0),
            FsItem::Directory(alloc::string::String::from("RootFS"), 0),
            FsItem::Directory(alloc::string::String::from("Initrd"), 0),
        ]);
    }

    if clean.eq_ignore_ascii_case("Initrd") {
        crate::log_info!("VFS", "Scanning Initrd ramdisk...");
        unsafe {
            if let Some(archive) = &*core::ptr::addr_of_mut!(crate::fs::tar::INITRD) {
                let mut list = alloc::vec![];
                for f in &archive.files {
                    let name = f.name.trim_start_matches("./").trim_start_matches("Initrd/");
                    if !name.is_empty() && !name.contains('/') {
                        let mtype = detect_file_media_type(name);
                        list.push(FsItem::File(alloc::string::String::from(name), f.size, mtype));
                    }
                }
                return Ok(list);
            }
        }
    }

    if clean.eq_ignore_ascii_case("RootFS") || clean.starts_with("RootFS/") || clean.starts_with("home") || clean.starts_with("etc") || clean.starts_with("bin") || clean.starts_with("usr") || clean.starts_with("var") {
        crate::log_info!("VFS", "Scanning EXT2 RootFS on Drive 2...");
        unsafe {
            if let Some(fs) = &*core::ptr::addr_of_mut!(crate::fs::ext2::EXT2_FS) {
                let target_subpath = clean.strip_prefix("RootFS/").unwrap_or(clean);

                if target_subpath.is_empty() || target_subpath.eq_ignore_ascii_case("RootFS") {
                    if let Ok(entries) = fs.list_directory(2) {
                        let mut items = alloc::vec![];
                        for (name, ino) in entries {
                            if name != "." && name != ".." && name != "lost+found" {
                                if let Ok(target_inode) = fs.read_inode(ino) {
                                    let is_dir = (target_inode.mode & 0o170000) == 0o040000;
                                    if is_dir {
                                        items.push(FsItem::Directory(name, 0));
                                    } else {
                                        let mtype = detect_file_media_type(&name);
                                        items.push(FsItem::File(name, target_inode.size as usize, mtype));
                                    }
                                }
                            }
                        }
                        return Ok(items);
                    }
                }

                let parts: alloc::vec::Vec<&str> = target_subpath.split('/').filter(|s| !s.is_empty()).collect();
                let mut current_ino = 2u32;
                let mut found_target = true;

                for part in parts {
                    if let Ok(entries) = fs.list_directory(current_ino) {
                        if let Some((_, next_ino)) = entries.iter().find(|(n, _)| n.eq_ignore_ascii_case(part)) {
                            current_ino = *next_ino;
                        } else {
                            found_target = false;
                            break;
                        }
                    } else {
                        found_target = false;
                        break;
                    }
                }

                if found_target {
                    if let Ok(entries) = fs.list_directory(current_ino) {
                        let mut items = alloc::vec![];
                        for (name, ino) in entries {
                            if name != "." && name != ".." {
                                if let Ok(target_inode) = fs.read_inode(ino) {
                                    let is_dir = (target_inode.mode & 0o170000) == 0o040000;
                                    if is_dir {
                                        items.push(FsItem::Directory(name, 0));
                                    } else {
                                        let mtype = detect_file_media_type(&name);
                                        items.push(FsItem::File(name, target_inode.size as usize, mtype));
                                    }
                                }
                            }
                        }
                        return Ok(items);
                    }
                }
            }
        }
    }

    if clean.to_ascii_uppercase().starts_with("EOS SHARE") {
        crate::log_info!("VFS", "Scanning FAT root on Drive 1...");
        let fat_sub = clean.strip_prefix("EOS SHARE/").unwrap_or("");
        if let Ok(items) = FAT_FS.list_dir(fat_sub) {
            return Ok(items);
        }
    }

    Err("Directory not found")
}

pub fn vfs_stat(path: &str) -> Result<(u64, u32), &'static str> {
    let p = resolve_path(path);
    if is_settings_file(&p) {
        if let Ok(bytes) = settings_store::load_raw_settings() {
            return Ok((bytes.len() as u64, 0o100644));
        } else {
            return Ok((0, 0o100644));
        }
    }

    if p.is_empty() || p == "EOS SHARE" || p == "RootFS" || p == "Initrd" || p == "/" || p == "." {
        return Ok((0, 0o040755));
    }
    if p.starts_with("EOS SHARE/") || p.starts_with("EOS SHARE") {
        let clean = p.strip_prefix("EOS SHARE").unwrap_or(&p).trim_matches('/');
        if let Ok(res) = FAT_FS.stat(clean) {
            return Ok(res);
        }
    }
    if let Ok(b) = vfs_read_bytes(&p) {
        return Ok((b.len() as u64, 0o100644));
    }
    if vfs_list_dir(&p).is_ok() {
        return Ok((0, 0o040755));
    }
    Err("Not found")
}

pub fn vfs_read_bytes(path: &str) -> Result<Vec<u8>, &'static str> {
    let p = resolve_path(path);
    let clean = p.trim_matches('/');

    if clean == "home/march/.config/settings.ini" || is_settings_file(&p) {
        unsafe {
            if let Some(fs) = &*core::ptr::addr_of_mut!(crate::fs::ext2::EXT2_FS) {
                if let Ok(inode) = fs.read_inode(19) {
                    if let Ok(bytes) = fs.read_file_data(&inode) { return Ok(bytes); }
                }
                if let Ok(inode) = fs.read_inode(11) {
                    if let Ok(bytes) = fs.read_file_data(&inode) { return Ok(bytes); }
                }
            }
        }
    }

    if clean == "home/march/.bash_history" || clean.ends_with(".bash_history") {
        unsafe {
            if let Some(fs) = &*core::ptr::addr_of_mut!(crate::fs::ext2::EXT2_FS) {
                if let Ok(inode) = fs.read_inode(18) {
                    if let Ok(bytes) = fs.read_file_data(&inode) { return Ok(bytes); }
                }
                if let Ok(inode) = fs.read_inode(12) {
                    if let Ok(bytes) = fs.read_file_data(&inode) { return Ok(bytes); }
                }
            }
        }
    }

    if clean == "etc/os-release" {
        unsafe {
            if let Some(fs) = &*core::ptr::addr_of_mut!(crate::fs::ext2::EXT2_FS) {
                if let Ok(inode) = fs.read_inode(20) {
                    if let Ok(bytes) = fs.read_file_data(&inode) { return Ok(bytes); }
                }
            }
        }
    }
    if clean == "etc/heroers" || clean == "etc/sudoers" {
        unsafe {
            if let Some(fs) = &*core::ptr::addr_of_mut!(crate::fs::ext2::EXT2_FS) {
                if let Ok(inode) = fs.read_inode(21) {
                    if let Ok(bytes) = fs.read_file_data(&inode) { return Ok(bytes); }
                }
            }
        }
    }

    unsafe {
        if let Some(archive) = &*core::ptr::addr_of_mut!(crate::fs::tar::INITRD) {
            let bare_name = clean.split('/').last().unwrap_or(clean);
            for f in &archive.files {
                let f_bare = f.name.split('/').last().unwrap_or(&f.name);
                if f.name.eq_ignore_ascii_case(clean) || f_bare.eq_ignore_ascii_case(bare_name) {
                    return Ok(core::slice::from_raw_parts(f.data_ptr, f.size).to_vec());
                }
            }
        }
    }

    unsafe {
        if let Some(fs) = &*core::ptr::addr_of_mut!(crate::fs::ext2::EXT2_FS) {
            if let Ok(entries) = fs.list_directory(2) {
                let check_name = clean.strip_prefix("RootFS/").unwrap_or(clean);
                if let Some((_, ino)) = entries.iter().find(|(n, _)| n.eq_ignore_ascii_case(check_name)) {
                    if let Ok(inode) = fs.read_inode(*ino) {
                        return fs.read_file_data(&inode);
                    }
                }
            }
        }
    }

    if clean.to_ascii_uppercase().starts_with("EOS SHARE/") || clean.eq_ignore_ascii_case("EOS SHARE") || !clean.contains('/') {
        let fat_path = clean.strip_prefix("EOS SHARE/").unwrap_or(clean);
        if let Ok(bytes) = FAT_FS.read_bytes(fat_path) {
            return Ok(bytes);
        }
    }

    Err("File not found")
}

pub fn vfs_save_text_file(filename: &str, content: &[u8]) -> Result<(), &'static str> {
    let p = resolve_path(filename);
    let clean = p.trim_matches('/');
    crate::log_info!("VFS", "vfs_save_text_file invoked for '{}' ({} bytes)", p, content.len());

    if clean == "home/march/.config/settings.ini" || is_settings_file(&p) {
        unsafe {
            if let Some(fs) = &*core::ptr::addr_of_mut!(crate::fs::ext2::EXT2_FS) {
                let _ = fs.write_file_data(19, content);
                let _ = fs.write_file_data(11, content);
                return Ok(());
            }
        }
    }

    if clean == "home/march/.bash_history" || clean.ends_with(".bash_history") {
        unsafe {
            if let Some(fs) = &*core::ptr::addr_of_mut!(crate::fs::ext2::EXT2_FS) {
                let _ = fs.write_file_data(18, content);
                let _ = fs.write_file_data(12, content);
                return Ok(());
            }
        }
    }

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

pub mod settings_store;

pub fn is_settings_file(path: &str) -> bool {
    let p = resolve_path(path);
    let clean = p.trim_matches('/');
    clean.eq_ignore_ascii_case("settings.ini")
        || clean.eq_ignore_ascii_case("RootFS/settings.ini")
        || clean.eq_ignore_ascii_case("EOS SHARE/settings.ini")
        || clean.ends_with("/settings.ini")
        || clean.ends_with("settings.ini")
}

pub fn is_protected_system_path(path: &str) -> bool {
    let clean = path.trim_matches('/');
    if clean.starts_with("home/march") || clean.starts_with("tmp") || clean.starts_with("var/log") {
        return false;
    }
    true
}

pub fn detect_file_media_type(name: &str) -> MediaType {
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".png") || lower.ends_with(".bmp") || lower.ends_with(".vec") {
        MediaType::Image
    } else if lower.ends_with(".txt") || lower.ends_with(".ini") || lower.ends_with(".log") || lower.ends_with(".conf") {
        MediaType::Text
    } else if lower.ends_with(".elf") || lower.ends_with(".bin") {
        MediaType::Executable
    } else {
        MediaType::Unknown
    }
}
