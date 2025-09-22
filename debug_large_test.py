#!/usr/bin/env python3
"""
大值调试测试 - 超过10KB阈值
"""
import socket
import time

def debug_large_test():
    # 创建12KB的测试值 (超过10KB阈值)
    test_value = b'LARGE_DEBUG_DATA_' * 1200  # 大约12KB

    host = '127.0.0.1'
    port = 11211

    print("🔧 大值调试测试开始...")
    print(f"   - 数据大小: {len(test_value)} bytes (超过10KB阈值)")

    try:
        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        sock.settimeout(15)  # 15秒超时
        sock.connect((host, port))
        print("✅ 连接成功")

        # 发送SET命令
        set_cmd = f"set large_debug_key 0 300 {len(test_value)}\r\n".encode()
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

        # 关闭连接
        sock.close()
        print("📤 关闭连接")

        # 重新连接获取数据
        print("📤 重新连接获取数据...")
        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        sock.settimeout(15)
        sock.connect((host, port))

        # 发送GET命令
        get_cmd = b"get large_debug_key\r\n"
        print(f"📤 发送GET命令: {len(get_cmd)} bytes")
        sock.send(get_cmd)

        # 接收响应头
        header = sock.recv(1024).decode()
        print(f"📥 GET响应头: {header[:100]}...")  # 只显示前100字符

        sock.close()
        print("✅ 大值测试完成")

    except Exception as e:
        print(f"❌ 大值测试失败: {e}")
        import traceback
        traceback.print_exc()

if __name__ == "__main__":
    debug_large_test()