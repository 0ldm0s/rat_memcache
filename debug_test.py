#!/usr/bin/env python3
"""
简单的大值调试测试
"""
import socket
import time

def debug_test():
    # 创建较小的测试值 (8KB - 超过10KB阈值)
    test_value = b'DEBUG_DATA_' * 160  # 大约8KB

    host = '127.0.0.1'
    port = 11211

    print("🔧 调试测试开始...")
    print(f"   - 数据大小: {len(test_value)} bytes")

    try:
        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        sock.settimeout(15)  # 15秒超时
        sock.connect((host, port))
        print("✅ 连接成功")

        # 发送SET命令
        set_cmd = f"set debug_key 0 300 {len(test_value)}\r\n".encode()
        print(f"📤 发送SET命令: {len(set_cmd)} bytes")
        sock.send(set_cmd)

        # 发送数据
        print(f"📤 发送数据: {len(test_value)} bytes")
        sock.send(test_value + b'\r\n')

        # 等待响应
        response = sock.recv(1024).decode().strip()
        print(f"📥 SET响应: {response}")

        # 短暂暂停
        time.sleep(0.5)

        # 发送GET命令
        get_cmd = b"get debug_key\r\n"
        print(f"📤 发送GET命令: {len(get_cmd)} bytes")
        sock.send(get_cmd)

        # 接收响应头
        header = sock.recv(1024).decode()
        print(f"📥 GET响应头: {header[:100]}...")  # 只显示前100字符

        sock.close()
        print("✅ 测试完成")

    except Exception as e:
        print(f"❌ 测试失败: {e}")
        import traceback
        traceback.print_exc()

if __name__ == "__main__":
    debug_test()