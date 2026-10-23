use crate::serial_println;
use core::sync::atomic::Ordering;

/// Escapes a string for embedding inside a JSON string literal, so that a
/// message containing quotes, backslashes or newlines can never break the
/// single-line [EOS_AUDIT] record that the host harness parses.
struct JsonEsc<'a>(&'a str);

impl core::fmt::Display for JsonEsc<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        use core::fmt::Write;
        for c in self.0.chars() {
            match c {
                '"' => f.write_str("\\\"")?,
                '\\' => f.write_str("\\\\")?,
                '\n' => f.write_str("\\n")?,
                '\r' => f.write_str("\\r")?,
                '\t' => f.write_str("\\t")?,
                c if (c as u32) < 0x20 => write!(f, "\\u{:04x}", c as u32)?,
                c => f.write_char(c)?,
            }
        }
        Ok(())
    }
}

pub fn emit_raw_json(subsys: &str, event: &str, status: &str, details: &str) {
    serial_println!(
        "[EOS_AUDIT] {{\"ts\":{},\"subsys\":\"{}\",\"event\":\"{}\",\"status\":\"{}\",\"details\":\"{}\"}}",
        crate::arch::x86_64::pit::get_ticks(),
        JsonEsc(subsys),
        JsonEsc(event),
        JsonEsc(status),
        JsonEsc(details)
    );
}

pub fn run_kernel_self_audit() -> usize {
    emit_raw_json("AUDIT", "SELF_TEST_START", "INFO", "Starting comprehensive kernel subsystem audit");
    let mut passed = 0usize;

    // 1. Physical Memory Manager (PMM)
    if let Some(frame) = crate::mm::frame::allocate_frame_safe() {
        crate::mm::frame::free_frame_safe(frame);
        emit_raw_json("MM_PMM", "FRAME_ALLOC_CYCLE", "PASS", "Allocated and freed 4KB physical frame");
        passed += 1;
    } else {
        emit_raw_json("MM_PMM", "FRAME_ALLOC_CYCLE", "FAIL", "OOM during frame allocation test");
    }

    // 2. Kernel Heap
    {
        let mut test_vec = alloc::vec::Vec::with_capacity(64);
        for i in 0..64 {
            test_vec.push(i as u64);
        }
        let used_kb = crate::mm::heap::HEAP_ALLOCATOR.used() / 1024;
        if test_vec.len() == 64 && test_vec[63] == 63 {
            let msg = alloc::format!("Heap functional, current usage: {} KB", used_kb);
            emit_raw_json("MM_HEAP", "ALLOC_VECTOR", "PASS", &msg);
            passed += 1;
        } else {
            emit_raw_json("MM_HEAP", "ALLOC_VECTOR", "FAIL", "Heap vector verification failed");
        }
    }

    // 3. SMP Cores Online
    let cores = crate::CORES_ONLINE.load(Ordering::SeqCst);
    if cores >= 1 {
        let msg = alloc::format!("{} hardware cores active", cores);
        emit_raw_json("SMP", "CORES_ONLINE", "PASS", &msg);
        passed += 1;
    } else {
        emit_raw_json("SMP", "CORES_ONLINE", "FAIL", "No SMP cores reported online");
    }

    // 4. VFS & TarFS Initrd
    match crate::fs::vfs_read_bytes("readme.txt") {
        Ok(bytes) if !bytes.is_empty() => {
            let msg = alloc::format!("Read readme.txt ({} bytes)", bytes.len());
            emit_raw_json("VFS_TAR", "READ_INITRD", "PASS", &msg);
            passed += 1;
        }
        _ => emit_raw_json("VFS_TAR", "READ_INITRD", "FAIL", "Could not read readme.txt from TarFS"),
    }

    // 5. EXT2 Persistent RootFS
    match crate::fs::vfs_read_bytes("etc/os-release") {
        Ok(bytes) if !bytes.is_empty() => {
            let msg = alloc::format!("EXT2 mounted, os-release size: {} bytes", bytes.len());
            emit_raw_json("VFS_EXT2", "READ_ROOTFS", "PASS", &msg);
            passed += 1;
        }
        _ => emit_raw_json("VFS_EXT2", "READ_ROOTFS", "FAIL", "Failed reading /etc/os-release from EXT2"),
    }

    // 6. Network E1000 Interface
    let net_mgr = crate::net::get_mgr();
    if net_mgr.active {
        let m = net_mgr.mac.0;
        let msg = alloc::format!(
            "E1000 bound, MAC {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
            m[0], m[1], m[2], m[3], m[4], m[5]
        );
        emit_raw_json("NET_E1000", "NIC_STATUS", "PASS", &msg);
        passed += 1;
    } else {
        emit_raw_json("NET_E1000", "NIC_STATUS", "WARN", "NIC inactive or not present");
    }

    let summary = alloc::format!("Completed kernel audit: {}/6 core checks passed", passed);
    emit_raw_json("AUDIT", "SELF_TEST_DONE", if passed >= 5 { "PASS" } else { "WARN" }, &summary);
    passed
}
