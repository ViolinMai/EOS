import socket
import time

def main():
    print("[*] Gamepad Server listening on 127.0.0.1:4444")
    server = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    server.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    server.settimeout(2.0)
    try:
        server.bind(("127.0.0.1", 4444))
        server.listen(1)
    except Exception as e:
        print(f"[!] Gamepad socket bind skipped: {e}")
        return

    try:
        while True:
            try:
                conn, _ = server.accept()
                conn.settimeout(1.0)
                seq = 0
                while True:
                    packet = bytearray([0xAA, 0x55, seq, 0x00, 128, 128, seq ^ 0x00 ^ 128 ^ 128])
                    try:
                        conn.sendall(packet)
                        seq = (seq + 1) % 256
                    except Exception:
                        break
                    time.sleep(0.016)
            except socket.timeout:
                continue
    except KeyboardInterrupt:
        pass
    finally:
        server.close()

if __name__ == "__main__":
    main()
