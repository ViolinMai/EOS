import os
import json
import zlib
import struct
import wave
import math

SHARE_DIR = r"C:\EOS_SHARE"
TEST_SUBJECT_DIR = os.path.join(SHARE_DIR, "TEST_SUBJECT")


def crc32_hex(data: bytes) -> str:
    return f"{zlib.crc32(data) & 0xFFFFFFFF:08x}"


def make_png_rgba(width: int, height: int, pixels_argb: list[int]) -> bytes:
    """Generates a valid RGBA PNG file from an ARGB8888 pixel list."""
    raw_rows = bytearray()
    for y in range(height):
        raw_rows.append(0)  # Filter type 0 (None)
        for x in range(width):
            argb = pixels_argb[y * width + x]
            a = (argb >> 24) & 0xFF
            r = (argb >> 16) & 0xFF
            g = (argb >> 8) & 0xFF
            b = argb & 0xFF
            raw_rows.extend((r, g, b, a))

    def png_chunk(chunk_type: bytes, data: bytes) -> bytes:
        c = chunk_type + data
        return struct.pack(">I", len(data)) + c + struct.pack(">I", zlib.crc32(c) & 0xFFFFFFFF)

    ihdr = struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)
    idat = zlib.compress(bytes(raw_rows), 9)
    return (
        b"\x89PNG\r\n\x1a\n"
        + png_chunk(b"IHDR", ihdr)
        + png_chunk(b"IDAT", idat)
        + png_chunk(b"IEND", b"")
    )


def make_wav_sine(freq_hz=440.0, duration_sec=0.25, sample_rate=22050) -> tuple[bytes, list[int]]:
    """Generates a mono 16-bit PCM WAV file and returns (bytes, first_16_samples)."""
    num_samples = int(duration_sec * sample_rate)
    samples = []
    for i in range(num_samples):
        t = i / sample_rate
        val = int(16000.0 * math.sin(2.0 * math.pi * freq_hz * t))
        samples.append(val)

    import io
    buf = io.BytesIO()
    with wave.open(buf, "wb") as wf:
        wf.setnchannels(1)
        wf.setsampwidth(2)
        wf.setframerate(sample_rate)
        wf.writeframes(struct.pack(f"<{len(samples)}h", *samples))
    return buf.getvalue(), samples[:16]


def ensure_test_subject():
    os.makedirs(TEST_SUBJECT_DIR, exist_ok=True)
    manifest = {
        "schema_version": 1,
        "description": "EOS TEST_SUBJECT Golden Reference Matrices for Autonomous Debugging & Tuning",
        "files": {},
        "golden_matrices": {},
    }

    # 1. 16x16 Golden Color Bars PNG
    w, h = 16, 16
    bars = [
        0xFFFF0000,  # Red
        0xFF00FF00,  # Green
        0xFF0000FF,  # Blue
        0xFFFFFF00,  # Yellow
        0xFF00FFFF,  # Cyan
        0xFFFF00FF,  # Magenta
        0xFFFFFFFF,  # White
        0xFF1E293B,  # Slate Dark
    ]
    pixels_16x16 = []
    for y in range(h):
        for x in range(w):
            bar_idx = (x * len(bars)) // w
            pixels_16x16.append(bars[bar_idx])

    png_path = os.path.join(TEST_SUBJECT_DIR, "color_bars_16x16.png")
    png_bytes = make_png_rgba(w, h, pixels_16x16)
    with open(png_path, "wb") as f:
        f.write(png_bytes)

    manifest["files"]["color_bars_16x16.png"] = {
        "size": len(png_bytes),
        "crc32": crc32_hex(png_bytes),
        "type": "image/png",
    }
    manifest["golden_matrices"]["color_bars_16x16.png"] = {
        "width": w,
        "height": h,
        "top_left_argb": f"0x{pixels_16x16[0]:08X}",
        "center_argb": f"0x{pixels_16x16[(h // 2) * w + (w // 2)]:08X}",
        "bottom_right_argb": f"0x{pixels_16x16[-1]:08X}",
        "row_0_argb": [f"0x{p:08X}" for p in pixels_16x16[:w]],
        "max_decode_ms": 25,
    }

    # 2. Pangram & Typography Reference
    pangram_text = (
        "The quick brown fox jumps over the lazy dog.\n"
        "0123456789 !@#$%^&*()_+-=[]{};':\",./<>?\n"
        "EOS Windows 7 Aero Glass Typography Benchmark\n"
    ).encode("utf-8")
    pangram_path = os.path.join(TEST_SUBJECT_DIR, "test_pangram.txt")
    with open(pangram_path, "wb") as f:
        f.write(pangram_text)

    manifest["files"]["test_pangram.txt"] = {
        "size": len(pangram_text),
        "crc32": crc32_hex(pangram_text),
        "lines": 3,
    }

    # 3. Structured CSV Dataset for Explorer / Python Automation
    csv_text = (
        "id,component,target_fps,memory_kb,status\n"
        "1,Compositor,60,4096,ACTIVE\n"
        "2,Explorer,60,2048,ACTIVE\n"
        "3,PythonVM,60,1024,ACTIVE\n"
        "4,MediaPlayer,60,3072,READY\n"
    ).encode("utf-8")
    csv_path = os.path.join(TEST_SUBJECT_DIR, "sample_table.csv")
    with open(csv_path, "wb") as f:
        f.write(csv_text)

    manifest["files"]["sample_table.csv"] = {
        "size": len(csv_text),
        "crc32": crc32_hex(csv_text),
        "expected_rows": 4,
        "expected_memory_kb_sum": 10240,
    }

    # 4. Reference WAV Audio Clip (440Hz Sine)
    wav_bytes, first_16_pcm = make_wav_sine()
    wav_path = os.path.join(TEST_SUBJECT_DIR, "tone_440hz.wav")
    with open(wav_path, "wb") as f:
        f.write(wav_bytes)

    manifest["files"]["tone_440hz.wav"] = {
        "size": len(wav_bytes),
        "crc32": crc32_hex(wav_bytes),
        "sample_rate": 22050,
        "channels": 1,
        "first_16_pcm_samples": first_16_pcm,
    }

    # 5. Python Automation Benchmark Script
    py_script = (
        "# EOS Userspace Python Golden Test Script\n"
        "def compute_matrix_trace(n):\n"
        "    total = 0\n"
        "    for i in range(n):\n"
        "        total = total + (i * i)\n"
        "    return total\n\n"
        "result = compute_matrix_trace(10)\n"
        "print('TRACE_10=' + str(result))\n"
    ).encode("utf-8")
    py_path = os.path.join(TEST_SUBJECT_DIR, "01_math_matrix.py")
    with open(py_path, "wb") as f:
        f.write(py_script)

    manifest["files"]["01_math_matrix.py"] = {
        "size": len(py_script),
        "crc32": crc32_hex(py_script),
        "expected_stdout": "TRACE_10=285",
    }

    manifest_path = os.path.join(TEST_SUBJECT_DIR, "expected_manifest.json")
    with open(manifest_path, "w", encoding="utf-8") as f:
        json.dump(manifest, f, indent=2)

    print(f"[+] TEST_SUBJECT ready at {TEST_SUBJECT_DIR} ({len(manifest['files'])} golden test assets).")
    return manifest


if __name__ == "__main__":
    ensure_test_subject()
