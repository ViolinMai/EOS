import os
import sys
import time
import json
import socket
import struct
import subprocess
import argparse

class QmpClient:
    def __init__(self, host="127.0.0.1", port=4445):
        self.host = host
        self.port = port
        self.sock = None

    def connect(self, timeout=15.0):
        deadline = time.time() + timeout
        while time.time() < deadline:
            try:
                s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
                s.settimeout(2.0)
                s.connect((self.host, self.port))
                self.sock = s
                greeting = self._recv_json()
                if greeting and "QMP" in greeting:
                    self.execute("qmp_capabilities")
                    return True
            except Exception:
                time.sleep(0.25)
        return False

    def _recv_json(self):
        buf = b""
        while b"\n" not in buf:
            chunk = self.sock.recv(4096)
            if not chunk:
                return None
            buf += chunk
        line = buf.split(b"\n", 1)[0]
        return json.loads(line.decode("utf-8", errors="ignore"))

    def execute(self, cmd, arguments=None):
        payload = {"execute": cmd}
        if arguments:
            payload["arguments"] = arguments
        self.sock.sendall((json.dumps(payload) + "\r\n").encode("utf-8"))
        while True:
            resp = self._recv_json()
            if resp is None:
                return None
            if "return" in resp or "error" in resp:
                return resp

    def screendump(self, filepath):
        abs_path = os.path.abspath(filepath)
        return self.execute("screendump", {"filename": abs_path})

    def send_key(self, key_name):
        return self.execute("send-key", {"keys": [{"type": "qcode", "data": key_name}]})

    def mouse_move(self, dx, dy):
        return self.execute("input-send-event", {
            "events": [
                {"type": "rel", "data": {"axis": "x", "value": int(dx)}},
                {"type": "rel", "data": {"axis": "y", "value": int(dy)}},
            ]
        })

    def mouse_click(self):
        self.execute("input-send-event", {
            "events": [{"type": "btn", "data": {"down": True, "button": "left"}}]
        })
        time.sleep(0.08)
        self.execute("input-send-event", {
            "events": [{"type": "btn", "data": {"down": False, "button": "left"}}]
        })

    def quit(self):
        try:
            self.execute("quit")
            self.sock.close()
        except Exception:
            pass


def verify_ppm_image(ppm_path):
    """Parses a QEMU raw PPM (P6) screendump and verifies resolution and non-blank visual content."""
    if not os.path.exists(ppm_path):
        return False, "Screendump file not created"
    with open(ppm_path, "rb") as f:
        data = f.read()
    if not data.startswith(b"P6"):
        return False, "Invalid PPM magic header"

    lines = []
    idx = 0
    while len(lines) < 3 and idx < len(data):
        nl = data.find(b"\n", idx)
        if nl == -1:
            break
        line = data[idx:nl].strip()
        if not line.startswith(b"#"):
            lines.append(line.decode("ascii", errors="ignore"))
        idx = nl + 1

    if len(lines) < 3:
        return False, "Incomplete PPM header"

    dims = lines[1].split()
    w, h = int(dims[0]), int(dims[1])
    pixels = data[idx:]
    expected_len = w * h * 3
    if len(pixels) < expected_len:
        return False, f"Truncated pixel buffer ({len(pixels)} < {expected_len})"

    unique_colors = set()
    step = max(3, (expected_len // 1500) * 3)
    for offset in range(0, expected_len - 3, step):
        rgb = (pixels[offset], pixels[offset + 1], pixels[offset + 2])
        unique_colors.add(rgb)

    if len(unique_colors) < 5:
        return False, f"Screen appears blank or solid-colored (only {len(unique_colors)} unique colors)"

    return True, f"Valid {w}x{h} frame with {len(unique_colors)} sampled distinct colors"


def run_steps(qmp, steps):
    """Runs a small scripted input scenario through QMP.

    Each step is a dict: {"wait": seconds} | {"move": [dx, dy]} | {"click": true} | {"key": "ret"}
    """
    for step in steps:
        if "wait" in step:
            time.sleep(float(step["wait"]))
        elif "move" in step:
            qmp.mouse_move(step["move"][0], step["move"][1])
        elif step.get("click"):
            qmp.mouse_click()
        elif "key" in step:
            qmp.send_key(step["key"])
        time.sleep(0.15)


def compare_test_subject_outputs(require_output=False):
    """Compares `/EOS SHARE/TEST_SUBJECT/eos_output.json` against `expected_manifest.json`.
    Returns (status_str, details_str, diffs_list) where status_str is 'PASS', 'SKIP', or 'FAIL'.
    """
    test_dir = r"C:\EOS_SHARE\TEST_SUBJECT"
    expected_path = os.path.join(test_dir, "expected_manifest.json")
    actual_path = os.path.join(test_dir, "eos_output.json")

    if not os.path.exists(expected_path):
        return "FAIL", "Missing expected_manifest.json in C:\\EOS_SHARE\\TEST_SUBJECT", ["expected_manifest.json missing"]

    with open(expected_path, "r", encoding="utf-8") as f:
        expected = json.load(f)

    if not os.path.exists(actual_path):
        if require_output:
            return "FAIL", "Required eos_output.json was not generated by Userspace", ["eos_output.json missing"]
        return "SKIP", "eos_output.json not yet generated (pre-M05 milestone)", []

    diffs = []
    try:
        with open(actual_path, "r", encoding="utf-8") as f:
            actual = json.load(f)
    except Exception as e:
        return "FAIL", f"Malformed eos_output.json: {e}", [str(e)]

    # Compare any overlapping keys between actual["files"] and expected["files"]
    exp_files = expected.get("files", {})
    act_files = actual.get("files", {})
    for fname, act_meta in act_files.items():
        if fname in exp_files:
            exp_meta = exp_files[fname]
            if "size" in act_meta and act_meta["size"] != exp_meta.get("size"):
                diffs.append(f"{fname}: size mismatch (got {act_meta['size']}, expected {exp_meta.get('size')})")
            if "crc32" in act_meta and str(act_meta["crc32"]).lower() != str(exp_meta.get("crc32")).lower():
                diffs.append(f"{fname}: crc32 mismatch (got {act_meta['crc32']}, expected {exp_meta.get('crc32')})")

    # Compare golden image matrices if present
    exp_mat = expected.get("golden_matrices", {})
    act_mat = actual.get("golden_matrices", {})
    for mname, act_m in act_mat.items():
        if mname in exp_mat:
            exp_m = exp_mat[mname]
            for k in ("width", "height", "top_left_argb", "center_argb", "bottom_right_argb"):
                if k in act_m and str(act_m[k]).upper() != str(exp_m.get(k)).upper():
                    diffs.append(f"Matrix {mname}.{k}: got {act_m[k]}, expected {exp_m.get(k)}")

    if diffs:
        return "FAIL", f"TEST_SUBJECT matrix comparison found {len(diffs)} discrepancy(s)", diffs
    return "PASS", "All reported TEST_SUBJECT matrices match golden expectations", []


def run_automated_audit(timeout_sec=35, headless=True, expect=None, steps=None, verify_test_subject=True, require_test_output=False):
    expect = list(expect or [])
    steps = list(steps or [])
    print("=" * 72)
    print("[*] EOS AUTOMATED SYSTEM, UX & TEST_SUBJECT AUDIT HARNESS")
    print("=" * 72)

    if verify_test_subject and os.path.exists("setup_test_subject.py"):
        subprocess.run([sys.executable, "setup_test_subject.py"], check=False)

    print("[1/4] Building Userspace & Kernel images...")
    build_res = subprocess.run([sys.executable, "run_qemu.py", "--build-only"])
    if build_res.returncode != 0:
        print("[!] FAIL: Compilation failed.")
        return False

    os.makedirs("target/audit", exist_ok=True)
    serial_log = os.path.abspath("target/audit/serial.log")
    screenshot_ppm = os.path.abspath("target/audit/desktop_frame.ppm")
    report_json = os.path.abspath("target/audit/audit_report.json")

    for f in (serial_log, screenshot_ppm, report_json):
        if os.path.exists(f):
            os.remove(f)

    qmp_port = 4445
    qemu_args = [
        sys.executable, "run_qemu.py",
        "--skip-build",
        "--qmp", f"tcp:127.0.0.1:{qmp_port},server,nowait",
        "--serial-file", serial_log,
    ]
    if headless:
        qemu_args.append("--headless")

    print("[2/4] Launching EOS inside QEMU harness...")
    qemu_proc = subprocess.Popen(qemu_args)
    qmp = QmpClient("127.0.0.1", qmp_port)

    audit_events = []
    panicked = False
    desktop_ready = False
    steps_done = False

    def pending_markers():
        return [
            e for e in expect
            if not any(e in str(ev.get("details", "")) for ev in audit_events)
        ]

    try:
        if not qmp.connect(timeout=12.0):
            print("[!] FAIL: Could not connect to QEMU QMP control socket.")
            return False

        print("[3/4] Monitoring [EOS_AUDIT] telemetry stream...")
        start_time = time.time()
        seen_lines = 0

        while time.time() - start_time < timeout_sec:
            if os.path.exists(serial_log):
                with open(serial_log, "r", encoding="utf-8", errors="ignore") as f:
                    lines = f.readlines()
                for line in lines[seen_lines:]:
                    clean = line.strip()
                    if "[KERNEL PANIC]" in clean:
                        panicked = True
                        print(f"    [!] {clean}")
                    if "[EOS_AUDIT]" in clean:
                        json_part = clean.split("[EOS_AUDIT]", 1)[1].strip()
                        try:
                            ev = json.loads(json_part)
                            audit_events.append(ev)
                            print(f"    -> [{ev.get('status')}] {ev.get('subsys')}:{ev.get('event')} - {ev.get('details')}")
                            if "FIRST_FRAME:PASS" in ev.get("details", ""):
                                desktop_ready = True
                        except Exception:
                            pass
                seen_lines = len(lines)

            if desktop_ready and not steps_done:
                steps_done = True
                if steps:
                    print("    -> Running scripted input steps...")
                    run_steps(qmp, steps)

            if panicked or (desktop_ready and not pending_markers()):
                break
            time.sleep(0.4)

        print("[4/4] Injecting synthetic input & capturing framebuffer...")
        qmp.mouse_move(40, 30)
        time.sleep(0.3)
        qmp.screendump(screenshot_ppm)
        time.sleep(0.5)

    finally:
        qmp.quit()
        qemu_proc.terminate()
        try:
            qemu_proc.wait(timeout=3.0)
        except Exception:
            qemu_proc.kill()

    screen_ok, screen_msg = verify_ppm_image(screenshot_ppm)
    print(f"    -> Framebuffer Check: {'PASS' if screen_ok else 'FAIL'} ({screen_msg})")

    required_subsystems = {"MM_PMM", "MM_HEAP", "SMP", "VFS_TAR", "VFS_EXT2"}
    passed_subsystems = {
        e["subsys"] for e in audit_events if e.get("status") == "PASS"
    }
    missing = required_subsystems - passed_subsystems

    missing_expected = pending_markers()
    if missing_expected:
        print(f"    -> Missing expected markers: {missing_expected}")

    ts_status, ts_msg, ts_diffs = (
        compare_test_subject_outputs(require_output=require_test_output)
        if verify_test_subject
        else ("SKIP", "Verification disabled", [])
    )
    print(f"    -> TEST_SUBJECT Matrix Check: {ts_status} ({ts_msg})")
    for d in ts_diffs:
        print(f"       [DIFF] {d}")

    overall_pass = (
        (not panicked)
        and (len(missing) == 0)
        and (len(missing_expected) == 0)
        and desktop_ready
        and screen_ok
        and (ts_status in ("PASS", "SKIP"))
    )

    report = {
        "overall_status": "PASS" if overall_pass else "FAIL",
        "panicked": panicked,
        "desktop_first_frame": desktop_ready,
        "framebuffer_verified": screen_ok,
        "framebuffer_details": screen_msg,
        "test_subject_verdict": ts_status,
        "test_subject_details": ts_msg,
        "test_subject_diffs": ts_diffs,
        "missing_subsystems": list(missing),
        "expected_markers": expect,
        "missing_markers": missing_expected,
        "audit_events": audit_events,
    }
    with open(report_json, "w", encoding="utf-8") as f:
        json.dump(report, f, indent=2)

    print("=" * 72)
    print(f"[*] AUDIT VERDICT: {'PASS' if overall_pass else 'FAIL'} (Report saved to {report_json})")
    print("=" * 72)
    return overall_pass


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--gui", action="store_true", help="Show QEMU window during audit")
    parser.add_argument("--timeout", type=int, default=35, help="Audit timeout in seconds")
    parser.add_argument("--expect", action="append", default=[], help="Marker substring that must appear in an [EOS_AUDIT] details field (repeatable)")
    parser.add_argument("--steps", type=str, default=None, help='JSON list of input steps, e.g. [{"move":[10,0]},{"click":true},{"wait":1}]')
    parser.add_argument("--verify-test-subject", action="store_true", default=True, help="Seed and verify C:\\EOS_SHARE\\TEST_SUBJECT golden matrices")
    parser.add_argument("--require-test-output", action="store_true", default=False, help="Fail audit if eos_output.json is not generated by Userspace")
    cli = parser.parse_args()
    ok = run_automated_audit(
        timeout_sec=cli.timeout,
        headless=not cli.gui,
        expect=cli.expect,
        steps=json.loads(cli.steps) if cli.steps else [],
        verify_test_subject=cli.verify_test_subject,
        require_test_output=cli.require_test_output,
    )
    sys.exit(0 if ok else 1)
