import os
import subprocess
import urllib.request
import shutil
import struct
import tarfile
import io
import sys
import time

LIMINE_BOOTX64_URL = "https://raw.githubusercontent.com/limine-bootloader/limine/v5.x-branch-binary/BOOTX64.EFI"

USER_CARGO_DIR = os.path.expanduser(r"~\.cargo\bin")
if USER_CARGO_DIR not in os.environ.get("PATH", ""):
    os.environ["PATH"] = USER_CARGO_DIR + os.pathsep + os.environ.get("PATH", "")

CARGO_BIN = shutil.which("cargo") or os.path.join(USER_CARGO_DIR, "cargo.exe")
QEMU_DEFAULT_DIR = r"C:\Program Files\qemu"
if os.path.exists(QEMU_DEFAULT_DIR) and QEMU_DEFAULT_DIR not in os.environ.get("PATH", ""):
    os.environ["PATH"] = QEMU_DEFAULT_DIR + os.pathsep + os.environ.get("PATH", "")

QEMU_EXE = shutil.which("qemu-system-x86_64") or os.path.join(QEMU_DEFAULT_DIR, "qemu-system-x86_64.exe")

def lfn_checksum(short_name_bytes):
    chk = 0
    for b in short_name_bytes: chk = (((chk & 1) << 7) + (chk >> 1) + b) & 0xFF
    return chk

def make_lfn_entry(order, long_name_slice, checksum, is_last=False):
    chars = [ord(c) for c in long_name_slice]
    while len(chars) < 13: chars.append(0xFFFF if len(chars) > len(long_name_slice) else 0x0000)
    entry = bytearray(32)
    entry[0] = (order | 0x40) if is_last else order
    struct.pack_into("<5H", entry, 1, *chars[0:5])
    entry[11] = 0x0F
    entry[12] = 0x00
    entry[13] = checksum
    struct.pack_into("<6H", entry, 14, *chars[5:11])
    struct.pack_into("<H", entry, 26, 0)
    struct.pack_into("<2H", entry, 28, *chars[11:13])
    return entry

def build_userspace_app():
    userspace_dir = os.path.abspath("userspace")
    cmd = [CARGO_BIN, "build", "--release", "--manifest-path", os.path.join(userspace_dir, "Cargo.toml"), "--target", "x86_64-unknown-linux-musl"]
    env = os.environ.copy()
    env.pop("RUSTFLAGS", None)
    print(f"[*] Executing Cargo Build for Userspace: {' '.join(cmd)}")
    res = subprocess.run(cmd, shell=True, env=env)
    if res.returncode != 0: return None
    bin_path = os.path.join(userspace_dir, "target", "x86_64-unknown-linux-musl", "release", "user_app")
    if os.path.exists(bin_path):
        with open(bin_path, "rb") as f: return f.read()
    return None

def create_demo_tar(user_elf_data):
    buf = io.BytesIO()
    with tarfile.open(fileobj=buf, mode="w") as tar:
        def add_file(name, data):
            ti = tarfile.TarInfo(name=name)
            ti.size = len(data)
            tar.addfile(ti, io.BytesIO(data))

        add_file("readme.txt", b"Welcome to EOS Kernel!\r\nThis file is read from TarFS.\r\n")
        if user_elf_data: add_file("user_app.elf", user_elf_data)

        share_dir = r"C:\EOS_SHARE"
        settings_file = os.path.join(share_dir, "settings.ini")
        if os.path.exists(settings_file):
            try:
                with open(settings_file, "rb") as sf:
                    sdata = sf.read()
                    add_file("settings.ini", sdata)
            except Exception:
                pass

        share_fonts_dir = r"C:\EOS_SHARE\fonts"
        scan_dirs = [share_fonts_dir, share_dir]
        for sdir in scan_dirs:
            if os.path.exists(sdir):
                for fname in os.listdir(sdir):
                    lower = fname.lower()
                    if lower.endswith(".ttf") or lower.endswith(".otf"):
                        fpath = os.path.join(sdir, fname)
                        if os.path.isfile(fpath):
                            try:
                                with open(fpath, "rb") as ff: fb = ff.read()
                                add_file(fname, fb)
                                add_file("fonts/" + fname, fb)
                            except Exception:
                                pass

        png_magic = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR\x00\x00\x00\x01\x00\x00\x00\x01\x08\x06\x00\x00\x00\x1f\x15\xc4\x89\x00\x00\x00\rIDAT\x08\x1d\x01\x05\x00\xfa\xff\x89P\x4E\x47\x00\x00\x00\x00IEND\xaeB`\x82"
        add_file("sample.png", png_magic)

    return buf.getvalue()

def make_uefi_fat32_disk(img_path, files):
    SECTOR_SIZE = 512
    SECTORS_PER_CLUSTER = 2
    CLUSTER_SIZE = SECTORS_PER_CLUSTER * SECTOR_SIZE
    RESERVED_SECTORS = 32
    NUM_FATS = 2
    PART_START_SECTOR = 2048
    PART_SECTORS = 262144
    TOTAL_SECTORS = PART_START_SECTOR + PART_SECTORS
    FAT_SIZE_SECTORS = 2048
    ROOT_CLUSTER = 2
    disk = bytearray(TOTAL_SECTORS * SECTOR_SIZE)
    disk[510:512] = b"\x55\xAA"
    disk[446:462] = struct.pack("<BBBBBBBBII", 0x80, 0x00, 0x02, 0x00, 0xEF, 0xFF, 0xFF, 0xFF, PART_START_SECTOR, PART_SECTORS)

    vbr_offset = PART_START_SECTOR * SECTOR_SIZE
    vbr = bytearray(SECTOR_SIZE)
    vbr[0:3] = b"\xEB\x58\x90"
    vbr[3:11] = b"MSWIN4.1"
    struct.pack_into("<H", vbr, 11, SECTOR_SIZE)
    vbr[13] = SECTORS_PER_CLUSTER
    struct.pack_into("<H", vbr, 14, RESERVED_SECTORS)
    vbr[16] = NUM_FATS
    struct.pack_into("<H", vbr, 17, 0)
    struct.pack_into("<H", vbr, 19, 0)
    vbr[21] = 0xF8
    struct.pack_into("<H", vbr, 22, 0)
    struct.pack_into("<H", vbr, 24, 63)
    struct.pack_into("<H", vbr, 26, 255)
    struct.pack_into("<I", vbr, 28, PART_START_SECTOR)
    struct.pack_into("<I", vbr, 32, PART_SECTORS)
    struct.pack_into("<I", vbr, 36, FAT_SIZE_SECTORS)
    struct.pack_into("<H", vbr, 40, 0)
    struct.pack_into("<H", vbr, 42, 0)
    struct.pack_into("<I", vbr, 44, ROOT_CLUSTER)
    struct.pack_into("<H", vbr, 48, 1)
    struct.pack_into("<H", vbr, 50, 6)
    vbr[64] = 0x80
    vbr[66] = 0x29
    struct.pack_into("<I", vbr, 67, 0x12345678)
    vbr[71:82] = b"EOS_BOOT   "
    vbr[82:90] = b"FAT32   "
    vbr[510:512] = b"\x55\xAA"
    disk[vbr_offset : vbr_offset + SECTOR_SIZE] = vbr
    disk[vbr_offset + (6 * SECTOR_SIZE) : vbr_offset + (7 * SECTOR_SIZE)] = vbr

    fsinfo = bytearray(SECTOR_SIZE)
    struct.pack_into("<I", fsinfo, 0, 0x41615252)
    struct.pack_into("<I", fsinfo, 484, 0x61417272)
    struct.pack_into("<I", fsinfo, 488, 0xFFFFFFFF)
    struct.pack_into("<I", fsinfo, 492, 0x00000003)
    struct.pack_into("<I", fsinfo, 508, 0xAA550000)
    disk[vbr_offset + SECTOR_SIZE : vbr_offset + (2 * SECTOR_SIZE)] = fsinfo
    disk[vbr_offset + (7 * SECTOR_SIZE) : vbr_offset + (8 * SECTOR_SIZE)] = fsinfo

    fat1_offset = vbr_offset + (RESERVED_SECTORS * SECTOR_SIZE)
    data_start = fat1_offset + (NUM_FATS * FAT_SIZE_SECTORS * SECTOR_SIZE)

    def set_fat(cluster, val):
        struct.pack_into("<I", disk, fat1_offset + (cluster * 4), val & 0x0FFFFFFF)

    set_fat(0, 0x0FFFFFF8)
    set_fat(1, 0x0FFFFFFF)
    set_fat(ROOT_CLUSTER, 0x0FFFFFFF)
    current_cluster = 3

    def allocate_clusters(num_clusters):
        nonlocal current_cluster
        start = current_cluster
        for i in range(num_clusters):
            set_fat(start + i, 0x0FFFFFFF if i == num_clusters - 1 else start + i + 1)
        current_cluster += num_clusters
        return start

    def write_data(data):
        if len(data) == 0:
            return 0
        needed = (len(data) + CLUSTER_SIZE - 1) // CLUSTER_SIZE
        start_c = allocate_clusters(needed)
        for i in range(needed):
            c_offset = data_start + ((start_c + i - 2) * CLUSTER_SIZE)
            chunk = data[i * CLUSTER_SIZE : (i + 1) * CLUSTER_SIZE]
            disk[c_offset : c_offset + len(chunk)] = chunk
        return start_c

    def make_entry(short_name, attr, cluster, size):
        entry = bytearray(32)
        entry[0:11] = short_name.encode("ascii")
        entry[11] = attr
        struct.pack_into("<H", entry, 20, (cluster >> 16) & 0xFFFF)
        struct.pack_into("<H", entry, 26, cluster & 0xFFFF)
        struct.pack_into("<I", entry, 28, size)
        return entry

    bootx64_cluster = write_data(files["BOOTX64.EFI"])
    conf_cluster = write_data(files["limine.conf"])
    k_cluster = write_data(files["EOS"])
    tar_cluster = write_data(files["initrd.tar"])
    startup_data = b"\\EFI\\BOOT\\BOOTX64.EFI\r\n"
    startup_cluster = write_data(startup_data)
    boot_dir_cluster = allocate_clusters(1)
    efi_dir_cluster = allocate_clusters(1)

    lfn_conf = make_lfn_entry(1, "limine.conf", lfn_checksum(b"LIMINE  CFG"), is_last=True)
    short_conf_entry = make_entry("LIMINE  CFG", 0x20, conf_cluster, len(files["limine.conf"]))

    boot_dir = bytearray(CLUSTER_SIZE)
    for i, ent in enumerate([
        make_entry(".          ", 0x10, boot_dir_cluster, 0),
        make_entry("..         ", 0x10, efi_dir_cluster, 0),
        make_entry("BOOTX64 EFI", 0x20, bootx64_cluster, len(files["BOOTX64.EFI"])),
        lfn_conf,
        short_conf_entry
    ]):
        boot_dir[i * 32 : (i + 1) * 32] = ent
    disk[data_start + ((boot_dir_cluster - 2) * CLUSTER_SIZE) : data_start + ((boot_dir_cluster - 1) * CLUSTER_SIZE)] = boot_dir

    efi_dir = bytearray(CLUSTER_SIZE)
    for i, ent in enumerate([
        make_entry(".          ", 0x10, efi_dir_cluster, 0),
        make_entry("..         ", 0x10, 0, 0),
        make_entry("BOOT       ", 0x10, boot_dir_cluster, 0)
    ]):
        efi_dir[i * 32 : (i + 1) * 32] = ent
    disk[data_start + ((efi_dir_cluster - 2) * CLUSTER_SIZE) : data_start + ((efi_dir_cluster - 1) * CLUSTER_SIZE)] = efi_dir

    root_dir = bytearray(CLUSTER_SIZE)
    for i, ent in enumerate([
        make_entry("EFI        ", 0x10, efi_dir_cluster, 0),
        make_entry("EOS        ", 0x20, k_cluster, len(files["EOS"])),
        make_entry("INITRD  TAR", 0x20, tar_cluster, len(files["initrd.tar"])),
        make_entry("STARTUP NSH", 0x20, startup_cluster, len(startup_data)),
        lfn_conf,
        short_conf_entry
    ]):
        root_dir[i * 32 : (i + 1) * 32] = ent
    disk[data_start + ((ROOT_CLUSTER - 2) * CLUSTER_SIZE) : data_start + ((ROOT_CLUSTER - 1) * CLUSTER_SIZE)] = root_dir

    fat2_offset = fat1_offset + (FAT_SIZE_SECTORS * SECTOR_SIZE)
    disk[fat2_offset : fat2_offset + (FAT_SIZE_SECTORS * SECTOR_SIZE)] = disk[fat1_offset : fat1_offset + (FAT_SIZE_SECTORS * SECTOR_SIZE)]
    with open(img_path, "wb") as f:
        f.write(disk)

def make_ext2_disk(img_path):
    if os.path.exists(img_path) and os.path.getsize(img_path) >= (10 * 1024 * 1024):
        return

    BLOCK_SIZE = 1024
    TOTAL_BLOCKS = 10240  # 10 MB
    INODES_COUNT = 256
    BLOCKS_PER_GROUP = 8192
    INODES_PER_GROUP = 256
    INODE_SIZE = 128

    disk = bytearray(TOTAL_BLOCKS * BLOCK_SIZE)

    # 1. Superblock (Block 1, offset 1024)
    sb_off = 1024
    struct.pack_into("<I", disk, sb_off + 0, INODES_COUNT)      # s_inodes_count
    struct.pack_into("<I", disk, sb_off + 4, TOTAL_BLOCKS)      # s_blocks_count
    struct.pack_into("<I", disk, sb_off + 8, 0)                 # s_r_blocks_count
    struct.pack_into("<I", disk, sb_off + 12, TOTAL_BLOCKS - 35) # s_free_blocks_count
    struct.pack_into("<I", disk, sb_off + 16, INODES_COUNT - 12) # s_free_inodes_count
    struct.pack_into("<I", disk, sb_off + 20, 1)                # s_first_data_block (1 for 1KB blocks)
    struct.pack_into("<I", disk, sb_off + 24, 0)                # s_log_block_size (0 = 1024)
    struct.pack_into("<I", disk, sb_off + 28, 0)                # s_log_frag_size
    struct.pack_into("<I", disk, sb_off + 32, BLOCKS_PER_GROUP) # s_blocks_per_group
    struct.pack_into("<I", disk, sb_off + 36, BLOCKS_PER_GROUP) # s_frags_per_group
    struct.pack_into("<I", disk, sb_off + 40, INODES_PER_GROUP) # s_inodes_per_group
    struct.pack_into("<H", disk, sb_off + 56, 0xEF53)           # s_magic
    struct.pack_into("<H", disk, sb_off + 58, 1)                # s_state (Clean)
    struct.pack_into("<H", disk, sb_off + 62, 0)                # s_minor_rev_level
    struct.pack_into("<I", disk, sb_off + 76, 1)                # s_rev_level (Dynamic rev)
    struct.pack_into("<H", disk, sb_off + 84, 11)               # s_first_ino
    struct.pack_into("<H", disk, sb_off + 88, INODE_SIZE)       # s_inode_size

    # 2. Block Group Descriptor (Block 2, offset 2048)
    bgd_off = 2048
    struct.pack_into("<I", disk, bgd_off + 0, 3)                # bg_block_bitmap
    struct.pack_into("<I", disk, bgd_off + 4, 4)                # bg_inode_bitmap
    struct.pack_into("<I", disk, bgd_off + 8, 5)                # bg_inode_table (Blocks 5..36)
    struct.pack_into("<H", disk, bgd_off + 12, TOTAL_BLOCKS - 35) # bg_free_blocks_count
    struct.pack_into("<H", disk, bgd_off + 14, INODES_COUNT - 12) # bg_free_inodes_count
    struct.pack_into("<H", disk, bgd_off + 16, 2)               # bg_used_dirs_count

    # 3. Bitmaps
    # Block bitmap (Block 3): Mark blocks 0..36 as used
    for b in range(37):
        disk[(3 * BLOCK_SIZE) + (b // 8)] |= (1 << (b % 8))

    # Inode bitmap (Block 4): Mark Inodes 1..12 as used
    for ino in range(1, 13):
        disk[(4 * BLOCK_SIZE) + ((ino - 1) // 8)] |= (1 << ((ino - 1) % 8))

    # Helper: Inode Table offset
    def get_inode_offset(ino):
        return (5 * BLOCK_SIZE) + ((ino - 1) * INODE_SIZE)

    # 4. Inode 2: Root Directory "/"
    root_ino_off = get_inode_offset(2)
    struct.pack_into("<H", disk, root_ino_off + 0, 0o040755)    # i_mode (Directory)
    struct.pack_into("<I", disk, root_ino_off + 4, BLOCK_SIZE)  # i_size
    struct.pack_into("<H", disk, root_ino_off + 26, 3)          # i_links_count
    struct.pack_into("<I", disk, root_ino_off + 28, 2)          # i_blocks (512-byte units)
    struct.pack_into("<I", disk, root_ino_off + 40, 37)         # i_block[0] = Block 37 (Dir contents)
    disk[(3 * BLOCK_SIZE) + (37 // 8)] |= (1 << (37 % 8))

    # 5. Inode 11: settings.ini (Pre-allocated regular file)
    init_settings = b"dark_mode=false\nui_scale=2\nwallpaper=\n"
    set_ino_off = get_inode_offset(11)
    struct.pack_into("<H", disk, set_ino_off + 0, 0o100644)     # i_mode (Regular file)
    struct.pack_into("<I", disk, set_ino_off + 4, len(init_settings)) # i_size
    struct.pack_into("<H", disk, set_ino_off + 26, 1)          # i_links_count
    struct.pack_into("<I", disk, set_ino_off + 28, 8)          # i_blocks (4 blocks allocated)
    for b_idx in range(4):
        struct.pack_into("<I", disk, set_ino_off + 40 + (b_idx * 4), 38 + b_idx)
        disk[(3 * BLOCK_SIZE) + ((38 + b_idx) // 8)] |= (1 << ((38 + b_idx) % 8))
    # Write initial settings payload
    disk[38 * BLOCK_SIZE : (38 * BLOCK_SIZE) + len(init_settings)] = init_settings

    # 6. Root Directory Data (Block 37)
    # Entries: "." (ino 2), ".." (ino 2), "settings.ini" (ino 11)
    dir_blk_off = 37 * BLOCK_SIZE
    cur = dir_blk_off

    # "."
    struct.pack_into("<I", disk, cur + 0, 2)
    struct.pack_into("<H", disk, cur + 4, 12)
    disk[cur + 6] = 1
    disk[cur + 7] = 2
    disk[cur + 8 : cur + 9] = b"."
    cur += 12

    # ".."
    struct.pack_into("<I", disk, cur + 0, 2)
    struct.pack_into("<H", disk, cur + 4, 12)
    disk[cur + 6] = 2
    disk[cur + 7] = 2
    disk[cur + 8 : cur + 10] = b".."
    cur += 12

    # "settings.ini"
    name_bytes = b"settings.ini"
    rec_len = BLOCK_SIZE - (cur - dir_blk_off)
    struct.pack_into("<I", disk, cur + 0, 11)
    struct.pack_into("<H", disk, cur + 4, rec_len)
    disk[cur + 6] = len(name_bytes)
    disk[cur + 7] = 1
    disk[cur + 8 : cur + 8 + len(name_bytes)] = name_bytes

    with open(img_path, "wb") as f:
        f.write(disk)

def prepare_and_run():
    share_dir = r"C:\EOS_SHARE"
    os.makedirs(share_dir, exist_ok=True); os.makedirs(os.path.join(share_dir, "fonts"), exist_ok=True)

    print("[*] Building Userspace App...")
    user_elf = build_userspace_app()
    if not user_elf:
        print("\n[!] ❌ ABORTING: user_app.elf could not be built successfully.")
        return

    print("[*] Building EOS Kernel (Release, LTO)...")
    env = os.environ.copy(); env.pop("RUSTFLAGS", None)
    res = subprocess.run([CARGO_BIN, "build", "--release", "-Z", "build-std=core,alloc"], shell=True, env=env)
    if res.returncode != 0: return

    target_dir = "target"; os.makedirs(target_dir, exist_ok=True)
    kernel_src = os.path.join(target_dir, "x86_64-unknown-none", "release", "EOS")
    bootx64_path = os.path.join(target_dir, "BOOTX64.EFI")

    if not os.path.exists(bootx64_path):
        print("[*] Downloading Limine BOOTX64.EFI v5...")
        urllib.request.urlretrieve(LIMINE_BOOTX64_URL, bootx64_path)

    with open(kernel_src, "rb") as f: kernel_data = f.read()
    with open("limine.conf", "rb") as f: conf_data = f.read()
    with open(bootx64_path, "rb") as f: bootx64_data = f.read()

    tar_data = create_demo_tar(user_elf)
    img_path = os.path.join(target_dir, "uefi_hdd.img")
    make_uefi_fat32_disk(img_path, { "EOS": kernel_data, "limine.conf": conf_data, "BOOTX64.EFI": bootx64_data, "initrd.tar": tar_data })

    ext2_img_path = os.path.join(target_dir, "rootfs.ext2"); make_ext2_disk(ext2_img_path)

    settings_img_path = os.path.join(target_dir, "settings.img")
    if not os.path.exists(settings_img_path) or os.path.getsize(settings_img_path) == 0:
        default_payload = b"dark_mode=false\nui_scale=2\nwallpaper=\n"
        crc = 0xFFFFFFFF
        for b in default_payload:
            crc ^= b
            for _ in range(8):
                crc = (crc >> 1) ^ 0xEDB88320 if (crc & 1) else (crc >> 1)
        crc = (~crc) & 0xFFFFFFFF

        sec0 = bytearray(512)
        sec0[0:4] = b"EOSS"
        struct.pack_into("<H", sec0, 4, 1) # version
        struct.pack_into("<H", sec0, 6, len(default_payload)) # length
        struct.pack_into("<I", sec0, 8, crc) # checksum
        sec0[20:20 + len(default_payload)] = default_payload

        with open(settings_img_path, "wb") as f:
            f.write(sec0)
            f.write(bytearray((1024 * 1024) - 512))

    qemu_share = os.path.join(os.path.dirname(QEMU_EXE), "share")
    if not os.path.exists(qemu_share): qemu_share = r"C:\Program Files\qemu\share"
    code_fd = os.path.join(qemu_share, "edk2-x86_64-code.fd")

    vars_dst = os.path.join(target_dir, "vars.fd")
    if os.path.exists(vars_dst):
        try: os.remove(vars_dst)
        except Exception: pass

    orig_vars = os.path.join(qemu_share, "edk2-i386-vars.fd")
    pflash_vars_args = []
    if os.path.exists(orig_vars):
        shutil.copyfile(orig_vars, vars_dst)
        pflash_vars_args = ["-drive", f"if=pflash,format=raw,file={vars_dst}"]

    print("[*] Starting Gamepad Bridge Server...")
    bridge_proc = subprocess.Popen([sys.executable, "gamepad_bridge.py"]); time.sleep(0.3)

    qemu_cmd = [
        QEMU_EXE, "-M", "q35", "-m", "8192M", "-smp", "5",
        "-accel", "tcg,thread=multi",
        "-drive", f"if=pflash,format=raw,readonly=on,file={code_fd}",
    ] + pflash_vars_args + [
        "-device", "piix3-ide,id=ide",
        "-drive", f"id=disk0,file={img_path},format=raw,if=none", "-device", "ide-hd,bus=ide.0,unit=0,drive=disk0",
        "-drive", f"id=disk1,file=fat:rw:{share_dir},format=raw,if=none", "-device", "ide-hd,bus=ide.0,unit=1,drive=disk1",
        "-drive", f"id=disk2,file={ext2_img_path},format=raw,if=none", "-device", "ide-hd,bus=ide.1,unit=0,drive=disk2",
        "-drive", f"id=disk3,file={settings_img_path},format=raw,if=none", "-device", "ide-hd,bus=ide.1,unit=1,drive=disk3",
        "-netdev", "user,id=n0,hostfwd=udp::68-:68", "-device", "e1000,netdev=n0",
        "-serial", "stdio", "-d", "guest_errors", "-no-reboot", "-no-shutdown"
    ]
    try: subprocess.run(qemu_cmd)
    finally: bridge_proc.terminate()

if __name__ == "__main__": prepare_and_run()
