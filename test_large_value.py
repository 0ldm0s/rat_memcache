#!/usr/bin/env python3
"""
测试大值处理功能的简单脚本
"""
import socket
import time

def test_large_value():
    # 创建小值 (512 bytes)
    small_value = b'a' * 512

    # 创建大值 (12KB - 超过10KB阈值)
    large_value = b'b' * 12 * 1024

    # 创建超大值 (50KB)
    huge_value = b'c' * 50 * 1024

    host = '127.0.0.1'
    port = 11211

    print("🧪 开始测试大值处理功能...")
    print(f"   - 小值: {len(small_value)} bytes")
    print(f"   - 大值: {len(large_value)} bytes (超过10KB阈值)")
    print(f"   - 超大值: {len(huge_value)} bytes")

    try:
        # 连接到服务器
        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        sock.connect((host, port))
        print("✅ 成功连接到服务器")

        # 测试1: 小值应该正常工作
        print("\n📝 测试1: 小值 (512B)")
        cmd = f"set small_key 0 60 {len(small_value)}\r\n".encode()
        sock.send(cmd)
        sock.send(small_value + b'\r\n')
        response = sock.recv(1024).decode().strip()
        print(f"   SET结果: {response}")

        # 获取小值
        sock.send(b"get small_key\r\n")
        response = sock.recv(2048).decode()
        print(f"   GET结果: {'成功' if 'VALUE small_key' in response else '失败'}")

        # 测试2: 大值应该直接下沉到L2
        print("\n📝 测试2: 大值 (12KB)")
        cmd = f"set large_key 0 60 {len(large_value)}\r\n".encode()
        sock.send(cmd)
        sock.send(large_value + b'\r\n')
        response = sock.recv(1024).decode().strip()
        print(f"   SET结果: {response}")

        # 获取大值
        sock.send(b"get large_key\r\n")
        response = sock.recv(2048).decode()
        print(f"   GET结果: {'成功' if 'VALUE large_key' in response else '失败'}")

        # 测试3: 超大值
        print("\n📝 测试3: 超大值 (50KB)")
        cmd = f"set huge_key 0 60 {len(huge_value)}\r\n".encode()
        sock.send(cmd)
        sock.send(huge_value + b'\r\n')
        response = sock.recv(1024).decode().strip()
        print(f"   SET结果: {response}")

        # 获取超大值
        sock.send(b"get huge_key\r\n")
        response = sock.recv(2048).decode()
        print(f"   GET结果: {'成功' if 'VALUE huge_key' in response else '失败'}")

        sock.close()
        print("\n✅ 测试完成！")

    except Exception as e:
        print(f"❌ 测试失败: {e}")

if __name__ == "__main__":
    test_large_value()