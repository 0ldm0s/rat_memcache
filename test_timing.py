#!/usr/bin/env python3
import socket
import time

def test_timing():
    """测试时间单位改进"""
    try:
        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        sock.settimeout(5)
        sock.connect(('127.0.0.1', 11211))

        # 存储测试数据
        test_data = b'X' * 1024
        set_cmd = f"set test_timing 0 0 {len(test_data)}\r\n".encode()
        sock.send(set_cmd)
        sock.send(test_data)
        sock.send(b'\r\n')
        response = sock.recv(1024)
        print(f"SET响应: {response.decode().strip()}")

        # 测试传统GET
        start_time = time.time()
        get_cmd = b"get test_timing\r\n"
        sock.send(get_cmd)

        # 接收响应
        full_response = sock.recv(65536)
        elapsed_ms = (time.time() - start_time) * 1000

        print(f"传统GET耗时: {elapsed_ms:.2f}毫秒")
        print(f"接收到的响应长度: {len(full_response)} bytes")

        # 测试流式GET
        start_time = time.time()
        streaming_get_cmd = b"streaming_get test_timing 16384\r\n"
        sock.send(streaming_get_cmd)
        response = sock.recv(1024).decode().strip()
        elapsed_ms = (time.time() - start_time) * 1000

        print(f"流式GET耗时: {elapsed_ms:.2f}毫秒")
        print(f"流式GET响应: {response}")

        sock.close()

    except Exception as e:
        print(f"测试失败: {e}")

if __name__ == "__main__":
    test_timing()