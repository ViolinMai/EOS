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

try:
    xinput = ctypes.windll.xinput1_4
except OSError:
    try:
        xinput = ctypes.windll.xinput1_3
    except OSError:
        sys.exit(1)

def get_state(controller_id=0):
    state = XINPUT_STATE()
    res = xinput.XInputGetState(controller_id, ctypes.byref(state))
    if res == 0:
        return state.Gamepad
    return None

def main():
    sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    while True:
        try:
            sock.connect(("127.0.0.1", 4444))
            break
        except ConnectionRefusedError:
            time.sleep(0.5)

    while True:
        pad = get_state(0)
        if pad:
            btns = 0
            if pad.wButtons & 0x1000: btns |= 0x01 # A
            if pad.wButtons & 0x2000: btns |= 0x02 # B
            if pad.wButtons & 0x4000: btns |= 0x04 # X
            if pad.wButtons & 0x8000: btns |= 0x08 # Y
            if pad.wButtons & 0x0001: btns |= 0x10 # Up
            if pad.wButtons & 0x0002: btns |= 0x20 # Down
            if pad.wButtons & 0x0004: btns |= 0x40 # Left
            if pad.wButtons & 0x0008: btns |= 0x80 # Right

            x = int((pad.sThumbLX + 32768) / 256)
            y = int((pad.sThumbRY + 32768) / 256)

            packet = bytearray([0xAA, btns, x, y])
            try:
                sock.send(packet)
            except (ConnectionResetError, BrokenPipeError):
                break
        
        time.sleep(0.016)

if __name__ == "__main__":
    main()
