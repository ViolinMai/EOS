#!/usr/bin/env bash
set -euo pipefail

PY_VER="3.12.7"
ROOT_DIR="$(cd "$(dirname "$0")" && pwd)"
VENDOR_DIR="$ROOT_DIR/userspace/vendor/python"
SRC_DIR="$ROOT_DIR/userspace/vendor/cpython_src"
TARBALL="$ROOT_DIR/userspace/vendor/Python-${PY_VER}.tgz"

mkdir -p "$VENDOR_DIR/lib" "$VENDOR_DIR/include" "$ROOT_DIR/userspace/vendor"

if [ ! -f "$TARBALL" ]; then
    echo "[*] Downloading CPython ${PY_VER}..."
    curl -L "https://www.python.org/ftp/python/${PY_VER}/Python-${PY_VER}.tgz" -o "$TARBALL"
fi

if [ ! -d "$SRC_DIR" ]; then
    echo "[*] Extracting CPython ${PY_VER}..."
    mkdir -p "$SRC_DIR"
    tar -xzf "$TARBALL" -C "$SRC_DIR" --strip-components=1
fi

cd "$SRC_DIR"

echo "[*] Configuring static CPython ${PY_VER} with musl-gcc..."
CC=musl-gcc \
CFLAGS="-O2 -fPIC -D_GNU_SOURCE" \
LDFLAGS="-static" \
./configure \
    --prefix="$VENDOR_DIR" \
    --disable-shared \
    --without-ensurepip \
    --without-pymalloc \
    --disable-test-modules \
    --with-system-ffi=no

echo "[*] Compiling libpython3.12.a..."
make -j"$(nproc)" libpython3.12.a

cp -f libpython3.12.a "$VENDOR_DIR/lib/"
cp -rf Include/* "$VENDOR_DIR/include/"
cp -f pyconfig.h "$VENDOR_DIR/include/"

echo "[*] Packaging uncompressed stdlib into python312.zip..."
cd "$SRC_DIR/Lib"
zip -0 -q -r "$VENDOR_DIR/python312.zip" . -x "test/*" "idlelib/*" "tkinter/*" "turtledemo/*" "ensurepip/*"

# Provision python_stdlib and python_libs into EOS_SHARE if available
if [ -d "/mnt/storage/delete/EOS_SHARE" ]; then
    mkdir -p "/mnt/storage/delete/EOS_SHARE/python_libs" "/mnt/storage/delete/EOS_SHARE/python_stdlib"
    cp -f "$VENDOR_DIR/python312.zip" "/mnt/storage/delete/EOS_SHARE/python312.zip"
    rsync -a --exclude='test' --exclude='idlelib' --exclude='tkinter' --exclude='turtledemo' --exclude='ensurepip' --exclude='__pycache__' "$SRC_DIR/Lib/" "/mnt/storage/delete/EOS_SHARE/python_stdlib/"
    echo "[+] Provisioned python_stdlib and python312.zip to /mnt/storage/delete/EOS_SHARE/"
fi

echo "[+] CPython ${PY_VER} static build complete!"
echo "    -> Static lib : $VENDOR_DIR/lib/libpython3.12.a"
echo "    -> Stdlib zip : $VENDOR_DIR/python312.zip"
echo "    -> External packages dir: /mnt/storage/delete/EOS_SHARE/python_libs"
