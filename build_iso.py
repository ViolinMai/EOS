import os
import shutil
import subprocess
import sys

def main():
    print("[1/3] Building EOS Kernel & Userspace...")
    res = subprocess.run([sys.executable, "run_qemu.py", "--build-only"])
    if res.returncode != 0:
        sys.exit(1)

    limine_dir = os.path.abspath("target/limine_bin")
    if not os.path.exists(limine_dir):
        print("[*] Cloning Limine v10.x-binary for Hybrid BIOS+UEFI ISO...")
        subprocess.run([
            "git", "clone", "--depth=1", "--branch=v10.x-binary",
            "https://github.com/limine-bootloader/limine.git", limine_dir
        ], check=True)
        subprocess.run(["make", "-C", limine_dir], check=True)

    iso_root = os.path.abspath("target/iso_root")
    if os.path.exists(iso_root):
        shutil.rmtree(iso_root)
    os.makedirs(os.path.join(iso_root, "boot", "limine"), exist_ok=True)
    os.makedirs(os.path.join(iso_root, "EFI", "BOOT"), exist_ok=True)

    kernel_src = os.path.abspath("target/x86_64-unknown-none/release/EOS")
    initrd_src = os.path.abspath("/mnt/storage/delete/EOS_SHARE/INITRD.TAR")
    if not os.path.exists(initrd_src):
        initrd_src = os.path.abspath("target/EOS_SHARE/INITRD.TAR")

    shutil.copy(kernel_src, os.path.join(iso_root, "EOS"))
    shutil.copy(kernel_src, os.path.join(iso_root, "EOS.BAK"))
    shutil.copy(initrd_src, os.path.join(iso_root, "INITRD.TAR"))
    shutil.copy(initrd_src, os.path.join(iso_root, "INITRD.BAK"))
    shutil.copy("limine.conf", os.path.join(iso_root, "boot", "limine", "limine.conf"))
    shutil.copy("limine.conf", os.path.join(iso_root, "limine.conf"))

    for f in ["limine-bios.sys", "limine-bios-cd.bin", "limine-uefi-cd.bin"]:
        shutil.copy(os.path.join(limine_dir, f), os.path.join(iso_root, "boot", "limine", f))
    shutil.copy(os.path.join(limine_dir, "BOOTX64.EFI"), os.path.join(iso_root, "EFI", "BOOT", "BOOTX64.EFI"))

    iso_out = os.path.abspath("target/EOS_Hybrid.iso")
    print(f"[2/3] Packaging Hybrid Legacy BIOS + UEFI ISO -> {iso_out}...")
    subprocess.run([
        "xorriso", "-as", "mkisofs",
        "-R", "-r", "-J",
        "-b", "boot/limine/limine-bios-cd.bin",
        "-no-emul-boot", "-boot-load-size", "4", "-boot-info-table",
        "-hfsplus", "-apm-block-size", "2048",
        "--efi-boot", "boot/limine/limine-uefi-cd.bin",
        "-efi-boot-part", "--efi-boot-image", "--protective-msdos-label",
        iso_root, "-o", iso_out
    ], check=True)

    print("[3/3] Installing Limine BIOS stage onto Hybrid ISO...")
    subprocess.run([os.path.join(limine_dir, "limine"), "bios-install", iso_out], check=True)
    print(f"\n[+] DONE! Hybrid ISO ready: {iso_out}")
    print("    Write to USB with: sudo dd if=target/EOS_Hybrid.iso of=/dev/sdX bs=4M status=progress oflag=sync")

if __name__ == "__main__":
    main()
