#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")" && pwd)"
SHARE_DIR="/mnt/storage/delete/EOS_SHARE"
[ -d "$SHARE_DIR" ] || SHARE_DIR="$ROOT_DIR/target/EOS_SHARE"
BB_DIR="$ROOT_DIR/userspace/vendor/busybox_musl"
BB_VER="1.36.1"

mkdir -p "$SHARE_DIR/bin" "$BB_DIR"
if [ ! -d "$BB_DIR/src" ]; then
    echo "[*] Downloading BusyBox $BB_VER..."
    mkdir -p "$BB_DIR/src"
    curl -fsSL "https://busybox.net/downloads/busybox-${BB_VER}.tar.bz2" -o "$BB_DIR/busybox.tar.bz2"
    tar -xjf "$BB_DIR/busybox.tar.bz2" -C "$BB_DIR/src" --strip-components=1
fi

cd "$BB_DIR/src"
make defconfig
sed -i 's/# CONFIG_STATIC is not set/CONFIG_STATIC=y/' .config
sed -i 's/CONFIG_TC=y/# CONFIG_TC is not set/' .config
make -j"$(nproc)" CC=musl-gcc busybox
cp -f busybox "$SHARE_DIR/bin/busybox"
echo "[+] Static musl BusyBox installed to $SHARE_DIR/bin/busybox"
