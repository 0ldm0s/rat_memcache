#!/usr/bin/env python3
"""
测试Python socket缓冲区设置
"""
import socket
import time

def test_socket_buffer():
    # 创建大值测试数据 (50KB)
    test_value = b'SOCKET_BUFFER_TEST_' * 2500  # 50KB
    host = '127.0.0.1'
    port = 11211

    print("🔧 测试socket缓冲区设置...")
    print(f"   - 数据大小: {len(test_value)} bytes")

    try:
        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)

        # 获取默认缓冲区大小
        default_sndbuf = sock.getsockopt(socket.SOL_SOCKET, socket.SO_SNDBUF)
        default_rcvbuf = sock.getsockopt(socket.SOL_SOCKET, socket.SO_RCVBUF)
        print(f"   - 默认发送缓冲区: {default_sndbuf} bytes")
        print(f"   - 默认接收缓冲区: {default_rcvbuf} bytes")

        # 设置更大的缓冲区
        sock.setsockopt(socket.SOL_SOCKET, socket.SO_SNDBUF, 1024 * 1024)  # 1MB发送缓冲区
        sock.setsockopt(socket.SOL_SOCKET, socket.SO_RCVBUF, 1024 * 1024)  # 1MB接收缓冲区

        # 获取设置后的缓冲区大小
        new_sndbuf = sock.getsockopt(socket.SOL_SOCKET, socket.SO_SNDBUF)
        new_rcvbuf = sock.getsockopt(socket.SOL_SOCKET, socket.SO_RCVBUF)
        print(f"   - 设置后发送缓冲区: {new_sndbuf} bytes")
        print(f"   - 设置后接收缓冲区: {new_rcvbuf} bytes")

        sock.settimeout(30)  # 30秒超时
        sock.connect((host, port))
        print("✅ 连接成功")

        # 发送SET命令
        set_cmd = f"set buffer_test_key 0 300 {len(test_value)}\r\n".encode()
        print(f"📤 发送SET命令: {len(set_cmd)} bytes")
        sock.send(set_cmd)

        # 发送数据
        print(f"📤 发送数据: {len(test_value)} bytes")
        sock.send(test_value + b'\r\n')

        # 等待响应
        response = sock.recv(1024).decode().strip()
        print(f"📥 SET响应: {response}")

        # 短暂暂停
        time.sleep(1)

        # 关闭连接
        sock.close()
        print("📤 关闭连接")

        # 重新连接获取数据
        print("📤 重新连接获取数据...")
        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        sock.setsockopt(socket.SOL_SOCKET, socket.SO_SNDBUF, 1024 * 1024)
        sock.setsockopt(socket.SOL_SOCKET, socket.SO_RCVBUF, 1024 * 1024)
        sock.settimeout(30)
        sock.connect((host, port))

        # 发送GET命令
        get_cmd = b"get buffer_test_key\r\n"
        print(f"📤 发送GET命令: {len(get_cmd)} bytes")
        sock.send(get_cmd)

        # 接收响应头
        header = sock.recv(1024).decode()
        print(f"📥 GET响应头长度: {len(header)} chars")
        print(f"📥 GET响应头: {header[:100]}...")

        # 计算并接收剩余数据
        if "VALUE buffer_test_key" in header:
            # 解析数据长度
            parts = header.split()
            if len(parts) >= 4:
                data_length = int(parts[3])
                print(f"📥 预期数据长度: {data_length} bytes")

                # 接收数据
                received_data = b''
                remaining = data_length
                chunk_size = 4096

                while remaining > 0:
                    chunk = sock.recv(min(chunk_size, remaining))
                    if not chunk:
                        break
                    received_data += chunk
                    remaining -= len(chunk)
                    print(f"   已接收: {len(received_data)}/{data_length} bytes")

                # 接收结束标记
                sock.recv(2)  # \r\n
                sock.recv(5)  # END\r\n

                print(f"✅ 数据接收完成: {len(received_data)} bytes")
                print(f"✅ 数据完整性: {'通过' if len(received_data) == data_length and received_data == test_value else '失败'}")

        sock.close()
        print("✅ Socket缓冲区测试完成")

    except Exception as e:
        print(f"❌ 测试失败: {e}")
        import traceback
        traceback.print_exc()

if __name__ == "__main__":
    test_socket_buffer()