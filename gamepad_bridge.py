import ctypes
import socket
import time
import sys

class XINPUT_GAMEPAD(ctypes.Structure):
    _fields_ = [
        ("wButtons", ctypes.c_ushort),
        ("bLeftTrigger", ctypes.c_ubyte),
        ("bRightTrigger", ctypes.c_ubyte),
        ("sThumbLX", ctypes.c_short),
        ("sThumbLY", ctypes.c_short),
        ("sThumbRX", ctypes.c_short),
        ("sThumbRY", ctypes.c_short),
    ]

class XINPUT_STATE(ctypes.Structure):
    _fields_ = [("dwPacketNumber", ctypes.c_ulong), ("Gamepad", XINPUT_GAMEPAD)]

xinput_dlls = ["xinput1_4.dll", "xinput1_3.dll", "xinput9_1_0.dll"]
xinput = None

for dll in xinput_dlls:
    try:
        xinput = ctypes.windll.LoadLibrary(dll)
        print(f"[✓] Loaded {dll}")
        break
    except OSError:
        pass

if not xinput:
    print("[ERROR] No XInput DLL found!")
    sys.exit(1)

def get_controller_state():
    for cid in range(4):
        state = XINPUT_STATE()
        res = xinput.XInputGetState(cid, ctypes.byref(state))
        if res == 0:
            return cid, state.Gamepad
    return None, None

def main():
    print("===================================================")
    print("🎮 GAMEPAD SERVER BRIDGE (127.0.0.1:4444)")
    print("===================================================")

    server = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    server.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    server.bind(("127.0.0.1", 4444))
    server.listen(1)
    print("[*] Waiting for QEMU connection on port 4444...")

    conn, addr = server.accept()
    print(f"[✓] QEMU connected from {addr}!")

    last_reported_cid = None
    packet_counter = 0

    while True:
        cid, pad = get_controller_state()

        if pad:
            if cid != last_reported_cid:
                print(f"[✓] Active Gamepad on Controller #{cid} (GameSir Nova Lite)")
                last_reported_cid = cid

            btns = 0
            if pad.wButtons & 0x1000: btns |= 0x01 # A
            if pad.wButtons & 0x2000: btns |= 0x02 # B
            if pad.wButtons & 0x4000: btns |= 0x04 # X
            if pad.wButtons & 0x8000: btns |= 0x08 # Y
            if pad.wButtons & 0x0001: btns |= 0x10 # Up
            if pad.wButtons & 0x0002: btns |= 0x20 # Down
            if pad.wButtons & 0x0004: btns |= 0x40 # Left
            if pad.wButtons & 0x0008: btns |= 0x80 # Right

            # معايرة المحاور لتصل إلى كامل المدى (Full Dynamic Range)
            lx = pad.sThumbLX
            ly = pad.sThumbLY

            # إزالة المنطقة الميتة وتوسيع المدى للوصول إلى 0 و 255 بسلاسة
            if abs(lx) < 4000:
                x = 128
            else:
                x = int(((lx + 32768) * 255) / 65535)

            if abs(ly) < 4000:
                y = 128
            else:
                y = int(((ly + 32768) * 255) / 65535)

            packet = bytearray([0xAA, btns, x, y])
            try:
                conn.sendall(packet)
                packet_counter += 1
            except Exception:
                break
        else:
            if last_reported_cid is not None:
                print("[-] Gamepad disconnected.")
                last_reported_cid = None

            if packet_counter % 30 == 0:
                try:
                    conn.sendall(bytearray([0xAA, 0, 128, 128]))
                except Exception:
                    break

        time.sleep(0.016)

    conn.close()
    server.close()

if __name__ == "__main__":
    main()
