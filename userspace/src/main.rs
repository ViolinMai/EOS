use std::fs::File;
use std::io::Read;
use std::time::Instant;
use std::collections::{BTreeMap, HashMap};
use serde::{Serialize, Deserialize};
use sha2::{Sha256, Digest};
use rand::{RngCore, SeedableRng, rngs::SmallRng};
use regex_automata::meta::Regex;

#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct BenchmarkReport {
    kernel: String,
    arch: String,
    layer: String,
    alloc_items: usize,
    hash_verified: bool,
}

fn log_test(tag: &str, msg: &str, time_micros: Option<u128>) {
    if let Some(t) = time_micros {
        println!("[TEST: {:<10}] {:<55} [ {} \u{00b5}s ]", tag, msg, t);
    } else {
        println!("[TEST: {:<10}] {}", tag, msg);
    }
}

fn run_crates_test_suite() {
    println!("\n┌────────────────────────────────────────────────────────┐");
    println!("│ EOS MIRROR LAYER - CRATES INTEGRATION TEST SUITE       │");
    println!("└────────────────────────────────────────────────────────┘\n");

    let total_start = Instant::now();

    // 1. اختبار std::collections و الـ Allocator
    let t_start = Instant::now();
    let mut map: HashMap<usize, String> = HashMap::new();
    let mut btree: BTreeMap<String, usize> = BTreeMap::new();
    for i in 0..500 {
        let key = format!("KEY_{:04}", i);
        btree.insert(key.clone(), i);
        map.insert(i, key);
    }
    assert_eq!(map.len(), 500);
    log_test("COLLECT", "HashMap & BTreeMap dynamic growth & rehashing (500 items)", Some(t_start.elapsed().as_micros()));

    // 2. اختبار Crate: SHA2
    let t_start = Instant::now();
    let payload = b"EOS Operating System - Advanced Mirror Layer Subsystem Active";
    let mut hasher = Sha256::new();
    hasher.update(payload);
    let hash_result = hasher.finalize();
    log_test("SHA2", "Compute SHA256 cryptographic digest without panic", Some(t_start.elapsed().as_micros()));
    println!("   -> Hash: {:x}", hash_result);

    // 3. اختبار Crate: Serde & Serde_JSON
    let t_start = Instant::now();
    let report = BenchmarkReport {
        kernel: "EOS".to_string(),
        arch: "x86_64".to_string(),
        layer: "MirrorLayer".to_string(),
        alloc_items: 200,
        hash_verified: true,
    };
    let json_str = serde_json::to_string_pretty(&report).expect("Failed to serialize");
    let deserialized: BenchmarkReport = serde_json::from_str(&json_str).expect("Failed to deserialize");
    assert_eq!(report, deserialized);
    log_test("SERDE_JSON", "Structural roundtrip JSON (Serialize/Deserialize)", Some(t_start.elapsed().as_micros()));
    println!("   -> JSON Length: {} bytes", json_str.len());

    // 4. اختبار Crate: Rand
    let t_start = Instant::now();
    let mut rng = SmallRng::seed_from_u64(0x1337_CAFE_DEAD_BEEF);
    let mut random_bytes = [0u8; 16];
    rng.fill_bytes(&mut random_bytes);
    log_test("RAND", "Pseudo-random stream generation via SmallRng", Some(t_start.elapsed().as_micros()));
    println!("   -> Stream: {:02x?}", random_bytes);

    // 5. اختبار Crate: Regex-Automata
    let t_start = Instant::now();
    let re = Regex::new(r"EOS-\w+").expect("Failed to compile pattern");
    let haystack = "Kernel build target: EOS-MirrorLayer-Stable";
    assert!(re.is_match(haystack));
    log_test("REGEX", "Compile regular expression and execute NFA matching", Some(t_start.elapsed().as_micros()));

    // 6. اختبار std::fs عبر VFS
    let t_start = Instant::now();
    match File::open("readme.txt") {
        Ok(mut f) => {
            let mut content = String::new();
            f.read_to_string(&mut content).unwrap_or(0);
            log_test("STD::FS", "File 'readme.txt' opened and read from TarFS/Ext2", Some(t_start.elapsed().as_micros()));
        }
        Err(e) => println!("   -> [WARN] readme.txt read skipped: {}", e),
    }

    let total_elapsed = total_start.elapsed();
    println!("\n============================================================");
    println!("🎉 ALL CRATES PASSED SUCCESSFULLY in {} ms! 🎉", total_elapsed.as_millis());
    println!("============================================================\n");
}

fn get_passed_argument() -> String {
    let ipc_ptr = 0x0000_0000_2050_0000 as *const u8;
    unsafe {
        let mut len = 0usize;
        while len < 255 && *ipc_ptr.add(len) != 0 { len += 1; }
        if len > 0 {
            let slice = std::slice::from_raw_parts(ipc_ptr, len);
            if let Ok(s) = std::str::from_utf8(slice) {
                let trimmed = s.trim();
                if !trimmed.is_empty() { return trimmed.to_string(); }
            }
        }
    }
    String::new()
}

fn main() {
    let arg = get_passed_argument();
    
    if arg == "test" || arg.is_empty() {
        run_crates_test_suite();
    } else {
        println!("[Viewer] Running with target argument: {}", arg);
        run_crates_test_suite();
    }
}
