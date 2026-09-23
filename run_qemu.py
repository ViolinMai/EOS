import os
import subprocess
import urllib.request
import shutil
import struct
import tarfile
import io
import sys
import time

LIMINE_BOOTX64_URL = "https://raw.githubusercontent.com/limine-bootloader/limine/v8.x-binary/BOOTX64.EFI"

def make_lfn_entry(order, long_name_slice, checksum, is_last=False):
    chars = [ord(c) for c in long_name_slice]
    while len(chars) < 13:
        chars.append(0xFFFF if len(chars) > len(long_name_slice) else 0x0000)

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

def lfn_checksum(short_name_bytes):
    chk = 0
    for b in short_name_bytes:
        chk = (((chk & 1) << 7) + (chk >> 1) + b) & 0xFF
    return chk

def build_userspace_app():
    userspace_dir = os.path.abspath("userspace")
    cmd = [
        "cargo", "build",
        "--release",
        "--manifest-path", os.path.join(userspace_dir, "Cargo.toml"),
        "--target", "x86_64-unknown-none"
    ]
    res = subprocess.run(cmd, shell=True)
    if res.returncode != 0:
        return None

    bin_path = os.path.join(userspace_dir, "target", "x86_64-unknown-none", "release", "user_app")
    if os.path.exists(bin_path):
        with open(bin_path, "rb") as f:
            return f.read()
    return None

def create_demo_tar(user_elf_data):
    buf = io.BytesIO()
    with tarfile.open(fileobj=buf, mode="w") as tar:
        f1_data = b"Welcome to EOS Kernel!\r\n"
        ti1 = tarfile.TarInfo(name="readme.txt")
        ti1.size = len(f1_data)
        tar.addfile(ti1, io.BytesIO(f1_data))

        if user_elf_data:
            ti3 = tarfile.TarInfo(name="app.elf")
            ti3.size = len(user_elf_data)
            tar.addfile(ti3, io.BytesIO(user_elf_data))

        icon_path = os.path.abspath("icon.png")
        if os.path.exists(icon_path):
            with open(icon_path, "rb") as f:
                icon_data = f.read()
            ti_icon = tarfile.TarInfo(name="icon.png")
            ti_icon.size = len(icon_data)
            tar.addfile(ti_icon, io.BytesIO(icon_data))

    return buf.getvalue()

def make_uefi_fat32_disk(img_path, files):
    SECTOR_SIZE = 512
    SECTORS_PER_CLUSTER = 8
    CLUSTER_SIZE = SECTORS_PER_CLUSTER * SECTOR_SIZE
    RESERVED_SECTORS = 32
    NUM_FATS = 2

    PART_START_SECTOR = 2048
    PART_SECTORS = 2097152
    TOTAL_SECTORS = PART_START_SECTOR + PART_SECTORS
    FAT_SIZE_SECTORS = 2048
    ROOT_CLUSTER = 2

    disk = bytearray(TOTAL_SECTORS * SECTOR_SIZE)
    disk[510:512] = b"\x55\xAA"
    disk[446:462] = struct.pack("<BBBBBBBBII", 0x80, 0x00, 0x02, 0x00, 0x0C, 0xFF, 0xFF, 0xFF, PART_START_SECTOR, PART_SECTORS)

    vbr_offset = PART_START_SECTOR * SECTOR_SIZE
    vbr = bytearray(SECTOR_SIZE)
    vbr[0:3] = b"\xEB\x58\x90"
    vbr[3:11] = b"MSWIN4.1"
    struct.pack_into("<H", vbr, 11, SECTOR_SIZE)
    vbr[13] = SECTORS_PER_CLUSTER
    struct.pack_into("<H", vbr, 14, RESERVED_SECTORS)
    vbr[16] = NUM_FATS
    vbr[21] = 0xF8
    struct.pack_into("<H", vbr, 24, 63)
    struct.pack_into("<H", vbr, 26, 255)
    struct.pack_into("<I", vbr, 28, PART_START_SECTOR)
    struct.pack_into("<I", vbr, 32, PART_SECTORS)
    struct.pack_into("<I", vbr, 36, FAT_SIZE_SECTORS)
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

    fat1_offset = vbr_offset + (RESERVED_SECTORS * SECTOR_SIZE)
    data_start = fat1_offset + (NUM_FATS * FAT_SIZE_SECTORS * SECTOR_SIZE)

    def set_fat32_entry(cluster, val):
        offset = fat1_offset + (cluster * 4)
        struct.pack_into("<I", disk, offset, val & 0x0FFFFFFF)

    set_fat32_entry(0, 0x0FFFFFF8)
    set_fat32_entry(1, 0x0FFFFFFF)
    set_fat32_entry(ROOT_CLUSTER, 0x0FFFFFFF)

    current_cluster = 3

    def write_file(data):
        nonlocal current_cluster
        if len(data) == 0: return 0
        needed = (len(data) + CLUSTER_SIZE - 1) // CLUSTER_SIZE
        start_c = current_cluster
        for i in range(needed):
            c = start_c + i
            nxt = (c + 1) if (i < needed - 1) else 0x0FFFFFFF
            set_fat32_entry(c, nxt)
            c_offset = data_start + ((c - 2) * CLUSTER_SIZE)
            chunk = data[i * CLUSTER_SIZE : (i + 1) * CLUSTER_SIZE]
            disk[c_offset : c_offset + len(chunk)] = chunk
        current_cluster += needed
        return start_c

    def make_entry(short_name, attr, cluster, size):
        entry = bytearray(32)
        entry[0:11] = short_name.encode("ascii")
        entry[11] = attr
        struct.pack_into("<H", entry, 20, (cluster >> 16) & 0xFFFF)
        struct.pack_into("<H", entry, 26, cluster & 0xFFFF)
        struct.pack_into("<I", entry, 28, size)
        return entry

    efi_dir_cluster = current_cluster
    current_cluster += 1
    set_fat32_entry(efi_dir_cluster, 0x0FFFFFFF)

    boot_dir_cluster = current_cluster
    current_cluster += 1
    set_fat32_entry(boot_dir_cluster, 0x0FFFFFFF)

    bootx64_cluster = write_file(files["BOOTX64.EFI"])
    conf_cluster = write_file(files["limine.conf"])
    k_cluster = write_file(files["EOS"])
    tar_cluster = write_file(files["initrd.tar"])
    startup_bytes = b"@echo -off\r\nFS0:\r\ncd \\EFI\\BOOT\r\nBOOTX64.EFI\r\n"
    startup_cluster = write_file(startup_bytes)

    short_conf = b"LIMINE~1CON"
    chk_c = lfn_checksum(short_conf)
    lfn_conf_entry = make_lfn_entry(1, "limine.conf", chk_c, is_last=True)
    short_conf_entry = make_entry("LIMINE~1CON", 0x20, conf_cluster, len(files["limine.conf"]))
    short_tar_entry = make_entry("INITRD  TAR", 0x20, tar_cluster, len(files["initrd.tar"]))

    boot_dir = bytearray(CLUSTER_SIZE)
    b_entries = [
        make_entry(".          ", 0x10, boot_dir_cluster, 0),
        make_entry("..         ", 0x10, efi_dir_cluster, 0),
        make_entry("BOOTX64 EFI", 0x20, bootx64_cluster, len(files["BOOTX64.EFI"])),
        lfn_conf_entry,
        short_conf_entry
    ]
    for i, ent in enumerate(b_entries):
        boot_dir[i * 32 : (i + 1) * 32] = ent
    c_off_boot = data_start + ((boot_dir_cluster - 2) * CLUSTER_SIZE)
    disk[c_off_boot : c_off_boot + CLUSTER_SIZE] = boot_dir

    efi_dir = bytearray(CLUSTER_SIZE)
    e_entries = [
        make_entry(".          ", 0x10, efi_dir_cluster, 0),
        make_entry("..         ", 0x10, ROOT_CLUSTER, 0),
        make_entry("BOOT       ", 0x10, boot_dir_cluster, 0)
    ]
    for i, ent in enumerate(e_entries):
        efi_dir[i * 32 : (i + 1) * 32] = ent
    c_off_efi = data_start + ((efi_dir_cluster - 2) * CLUSTER_SIZE)
    disk[c_off_efi : c_off_efi + CLUSTER_SIZE] = efi_dir

    root_dir = bytearray(CLUSTER_SIZE)
    root_entries = [
        make_entry("EOS        ", 0x20, k_cluster, len(files["EOS"])),
        short_tar_entry,
        make_entry("STARTUP NSH", 0x20, startup_cluster, len(startup_bytes)),
        lfn_conf_entry,
        short_conf_entry,
        make_entry("EFI        ", 0x10, efi_dir_cluster, 0)
    ]
    for i, ent in enumerate(root_entries):
        root_dir[i * 32 : (i + 1) * 32] = ent
    c_off_root = data_start + ((ROOT_CLUSTER - 2) * CLUSTER_SIZE)
    disk[c_off_root : c_off_root + CLUSTER_SIZE] = root_dir

    fat2_offset = fat1_offset + (FAT_SIZE_SECTORS * SECTOR_SIZE)
    disk[fat2_offset : fat2_offset + (FAT_SIZE_SECTORS * SECTOR_SIZE)] = disk[fat1_offset : fat1_offset + (FAT_SIZE_SECTORS * SECTOR_SIZE)]

    with open(img_path, "wb") as f:
        f.write(disk)

def make_ext2_disk(img_path):
    # لا نعيد إنشاء قرص ext2 إذا كان موجوداً للحفاظ على الإعدادات والملفات المخزنة
    if os.path.exists(img_path) and os.path.getsize(img_path) > 0:
        return

    BLOCK_SIZE = 1024
    BLOCKS_COUNT = 8192
    INODES_COUNT = 256
    BLOCKS_PER_GROUP = 8192
    INODES_PER_GROUP = 256
    
    disk = bytearray(BLOCKS_COUNT * BLOCK_SIZE)
    sb_offset = 1024
    struct.pack_into("<I", disk, sb_offset + 0, INODES_COUNT)
    struct.pack_into("<I", disk, sb_offset + 4, BLOCKS_COUNT)
    struct.pack_into("<I", disk, sb_offset + 12, BLOCKS_COUNT - 30)
    struct.pack_into("<I", disk, sb_offset + 16, INODES_COUNT - 11)
    struct.pack_into("<I", disk, sb_offset + 20, 1)
    struct.pack_into("<I", disk, sb_offset + 32, BLOCKS_PER_GROUP)
    struct.pack_into("<I", disk, sb_offset + 36, BLOCKS_PER_GROUP)
    struct.pack_into("<I", disk, sb_offset + 40, INODES_PER_GROUP)
    struct.pack_into("<H", disk, sb_offset + 56, 0xEF53)
    struct.pack_into("<H", disk, sb_offset + 58, 1)
    struct.pack_into("<I", disk, sb_offset + 76, 1)
    struct.pack_into("<I", disk, sb_offset + 84, 11)
    struct.pack_into("<H", disk, sb_offset + 88, 128)
    
    bgd_offset = 2048
    struct.pack_into("<I", disk, bgd_offset + 0, 3)
    struct.pack_into("<I", disk, bgd_offset + 4, 4)
    struct.pack_into("<I", disk, bgd_offset + 8, 5)
    struct.pack_into("<H", disk, bgd_offset + 12, BLOCKS_COUNT - 30)
    struct.pack_into("<H", disk, bgd_offset + 14, INODES_COUNT - 11)
    struct.pack_into("<H", disk, bgd_offset + 16, 2)
    
    disk[3 * BLOCK_SIZE : 3 * BLOCK_SIZE + 4] = b"\xFF\xFF\xFF\x03"
    disk[4 * BLOCK_SIZE : 4 * BLOCK_SIZE + 2] = b"\xFF\x07"
    
    inode2_offset = (5 * BLOCK_SIZE) + (1 * 128)
    struct.pack_into("<H", disk, inode2_offset + 0, 0x41ED)
    struct.pack_into("<I", disk, inode2_offset + 4, 1024)
    struct.pack_into("<H", disk, inode2_offset + 26, 2)
    struct.pack_into("<I", disk, inode2_offset + 28, 2)
    struct.pack_into("<I", disk, inode2_offset + 40, 26)
    
    r_off = 26 * BLOCK_SIZE
    struct.pack_into("<I", disk, r_off + 0, 2)
    struct.pack_into("<H", disk, r_off + 4, 12)
    struct.pack_into("<B", disk, r_off + 6, 1)
    struct.pack_into("<B", disk, r_off + 7, 2)
    disk[r_off + 8 : r_off + 9] = b"."
    
    r_off += 12
    struct.pack_into("<I", disk, r_off + 0, 2)
    struct.pack_into("<H", disk, r_off + 4, 12)
    struct.pack_into("<B", disk, r_off + 6, 2)
    struct.pack_into("<B", disk, r_off + 7, 2)
    disk[r_off + 8 : r_off + 10] = b".."
    
    with open(img_path, "wb") as f:
        f.write(disk)

def prepare_and_run():
    user_elf = build_userspace_app()
    res = subprocess.run(["cargo", "build", "--release"], shell=True)
    if res.returncode != 0: return

    target_dir = "target"
    kernel_src = os.path.join(target_dir, "x86_64-unknown-none", "release", "EOS")
    bootx64_path = os.path.join(target_dir, "BOOTX64.EFI")

    if not os.path.exists(bootx64_path):
        urllib.request.urlretrieve(LIMINE_BOOTX64_URL, bootx64_path)

    with open(kernel_src, "rb") as f: kernel_data = f.read()
    with open("limine.conf", "rb") as f: conf_data = f.read()
    with open(bootx64_path, "rb") as f: bootx64_data = f.read()

    tar_data = create_demo_tar(user_elf)

    img_path = os.path.join(target_dir, "uefi_hdd.img")
    make_uefi_fat32_disk(img_path, {
        "EOS": kernel_data,
        "limine.conf": conf_data,
        "BOOTX64.EFI": bootx64_data,
        "initrd.tar": tar_data
    })

    ext2_img_path = os.path.join(target_dir, "rootfs.ext2")
    make_ext2_disk(ext2_img_path)

    # تجهيز مجلد المشاركة من ويندوز وإنشاء ملف eos.cfg إذا لم يكن موجوداً
    share_dir = r"C:\EOS_SHARE"
    try:
        os.makedirs(share_dir, exist_ok=True)
        cfg_file = os.path.join(share_dir, "eos.cfg")
        if not os.path.exists(cfg_file):
            with open(cfg_file, "w", encoding="utf-8") as f:
                f.write("THEME=0\nWALLPAPER=\n")
    except Exception:
        pass

    qemu_share = r"C:\Program Files\qemu\share"
    code_fd = os.path.join(qemu_share, "edk2-x86_64-code.fd")
    vars_src = os.path.join(qemu_share, "edk2-i386-vars.fd")
    vars_dst = os.path.join(target_dir, "vars.fd")

    if os.path.exists(vars_src):
        shutil.copy(vars_src, vars_dst)
    else:
        with open(vars_dst, "wb") as f:
            f.write(b"\x00" * (128 * 1024))

    log_path = os.path.join(target_dir, "qemu.log")

    print("[*] Starting Gamepad Bridge Server...")
    bridge_proc = subprocess.Popen([sys.executable, "gamepad_bridge.py"])
    time.sleep(0.5)

    qemu_cmd = [
        "qemu-system-x86_64",
        "-M", "q35",
        "-m", "2048M",
        "-smp", "8",
        "-drive", f"if=pflash,format=raw,readonly=on,file={code_fd}",
        "-drive", f"if=pflash,format=raw,file={vars_dst}",
        "-device", "piix3-ide,id=ide",
        "-drive", f"id=disk0,file={img_path},format=raw,if=none",
        "-device", "ide-hd,bus=ide.0,unit=0,drive=disk0",
        "-drive", f"id=disk1,file=fat:rw:{share_dir},format=raw,if=none",
        "-device", "ide-hd,bus=ide.0,unit=1,drive=disk1",
        "-drive", f"id=disk2,file={ext2_img_path},format=raw,if=none",
        "-device", "ide-hd,bus=ide.1,unit=0,drive=disk2",
        "-serial", "stdio",
        "-serial", "tcp:127.0.0.1:4444",
        "-d", "int,cpu_reset,guest_errors",
        "-D", log_path,
        "-no-reboot",
        "-no-shutdown"
    ]
    
    print("[*] Launching QEMU...")
    try:
        subprocess.run(qemu_cmd)
    finally:
        bridge_proc.terminate()

if __name__ == "__main__":
    prepare_and_run()
