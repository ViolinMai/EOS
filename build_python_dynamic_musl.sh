#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")" && pwd)"
SHARE_DIR="/mnt/storage/delete/EOS_SHARE"
[ -d "$SHARE_DIR" ] || SHARE_DIR="$ROOT_DIR/target/EOS_SHARE"

mkdir -p "$SHARE_DIR/lib" "$SHARE_DIR/bin" "$SHARE_DIR/python_libs"

echo "[1/3] Provisioning ld-musl-x86_64.so.1 into $SHARE_DIR/lib/..."
MUSL_LD=""
for cand in \
    /usr/lib/ld-musl-x86_64.so.1 \
    /lib/ld-musl-x86_64.so.1 \
    /usr/lib/musl/lib/libc.so \
    /usr/x86_64-linux-musl/lib/libc.so
do
    if [ -f "$cand" ]; then
        MUSL_LD="$cand"
        break
    fi
done

if [ -z "$MUSL_LD" ]; then
    echo "[!] ERROR: Could not find ld-musl-x86_64.so.1 / musl libc.so on host."
    exit 1
fi

cp -fL "$MUSL_LD" "$SHARE_DIR/lib/ld-musl-x86_64.so.1"
cp -fL "$MUSL_LD" "$SHARE_DIR/lib/libc.so"
echo "[+] Copied $MUSL_LD -> $SHARE_DIR/lib/ld-musl-x86_64.so.1"

echo "[2/3] Building dynamic dlopen() verification suite (libeostest.so + dlopen_test.elf)..."
TMP_TEST="$ROOT_DIR/target/dyn_test"
mkdir -p "$TMP_TEST"

cat > "$TMP_TEST/libeostest.c" << 'EOF'
int eos_dyn_add(int a, int b) {
    return a + b + 100;
}
EOF

cat > "$TMP_TEST/dlopen_test.c" << 'EOF'
#include <stdio.h>
#include <dlfcn.h>

int main(void) {
    void *h = dlopen("/EOS SHARE/lib/libeostest.so", RTLD_NOW);
    if (!h) {
        printf("[DYN_TEST] dlopen failed: %s\n", dlerror());
        return 1;
    }
    int (*fn)(int, int) = (int (*)(int, int))dlsym(h, "eos_dyn_add");
    if (!fn) {
        printf("[DYN_TEST] dlsym failed: %s\n", dlerror());
        return 2;
    }
    int r = fn(20, 22);
    printf("[DYN_TEST] dlopen + dlsym OK -> eos_dyn_add(20, 22) = %d\n", r);
    dlclose(h);
    return (r == 142) ? 0 : 3;
}
EOF

musl-gcc -shared -fPIC -O2 "$TMP_TEST/libeostest.c" -o "$SHARE_DIR/lib/libeostest.so"
musl-gcc -O2 "$TMP_TEST/dlopen_test.c" -Wl,-rpath,"/EOS SHARE/lib" -o "$SHARE_DIR/bin/dlopen_test.elf"
echo "[+] Built $SHARE_DIR/lib/libeostest.so and $SHARE_DIR/bin/dlopen_test.elf"

echo "[3/3] Building Dynamic CPython 3.12 (--enable-shared) for full .so extension support..."
SRC_DIR="$ROOT_DIR/userspace/vendor/cpython_src"
DYN_BUILD="$ROOT_DIR/userspace/vendor/cpython_dyn_build"
if [ ! -d "$SRC_DIR" ]; then
    echo "[!] CPython source not found at $SRC_DIR. Run ./build_cpython_musl.sh first."
    exit 1
fi

if [ -f "$SRC_DIR/Makefile" ]; then
    make -C "$SRC_DIR" distclean >/dev/null 2>&1 || true
fi

# Build static musl zlib so CPython gets full zlib/gzip/zipimport support
ZLIB_DIR="$ROOT_DIR/userspace/vendor/zlib_musl"
if [ ! -f "$ZLIB_DIR/lib/libz.a" ]; then
    echo "[*] Building zlib for musl..."
    rm -rf "$ZLIB_DIR"
    mkdir -p "$ZLIB_DIR/src" "$ZLIB_DIR/lib" "$ZLIB_DIR/include"
    curl -fsSL "https://github.com/madler/zlib/releases/download/v1.3.1/zlib-1.3.1.tar.gz" -o "$ZLIB_DIR/zlib.tar.gz"
    tar -xzf "$ZLIB_DIR/zlib.tar.gz" -C "$ZLIB_DIR/src" --strip-components=1
    (
        cd "$ZLIB_DIR/src"
        CC=musl-gcc CFLAGS="-O2 -fPIC" ./configure --static --prefix="$ZLIB_DIR"
        make -j"$(nproc)" install
    )
fi

rm -rf "$DYN_BUILD"
mkdir -p "$DYN_BUILD"
cd "$DYN_BUILD"

CC=musl-gcc \
PKG_CONFIG=false \
CFLAGS="-O2 -fPIC -D_GNU_SOURCE -I$ZLIB_DIR/include" \
LDFLAGS="-Wl,-rpath,/lib -L$ZLIB_DIR/lib" \
ZLIB_CFLAGS="-I$ZLIB_DIR/include" \
ZLIB_LIBS="-L$ZLIB_DIR/lib -lz" \
py_cv_module__bz2=n/a \
py_cv_module__lzma=n/a \
py_cv_module__sqlite3=n/a \
py_cv_module__ssl=n/a \
py_cv_module__hashlib=n/a \
py_cv_module__ctypes=n/a \
"$SRC_DIR/configure" \
    --prefix="/usr" \
    --enable-shared \
    --without-ensurepip \
    --disable-test-modules \
    --with-builtin-hashlib-hashes=md5,sha1,sha2,sha3

make -j"$(nproc)" python sharedmods
cp -f python "$SHARE_DIR/bin/python3"
for sofile in libpython3.12.so*; do
    [ -f "$sofile" ] && cp -f "$sofile" "$SHARE_DIR/lib/$sofile"
done

mkdir -p "$SHARE_DIR/lib/python3.12/lib-dynload"
find build -name "*.so" -exec cp -f {} "$SHARE_DIR/lib/python3.12/lib-dynload/" \; 2>/dev/null || true

echo "[+] Dynamic CPython 3.12 + dlopen() ready in $SHARE_DIR!"
echo "    -> Dynamic Linker : $SHARE_DIR/lib/ld-musl-x86_64.so.1"
echo "    -> Dynamic Test   : /EOS SHARE/bin/dlopen_test.elf"
echo "    -> Dynamic Python : /EOS SHARE/bin/python3"
