#!/usr/bin/env python3
"""
简单测试大值处理功能
"""
import socket
import time

def test_simple_large_value():
    # 创建测试值 (15KB - 超过10KB阈值)
    test_value = b'X' * 15 * 1024

    host = '127.0.0.1'
    port = 11211

    print("🧪 测试大值存储和获取...")
    print(f"   - 测试值大小: {len(test_value)} bytes")

    try:
        # 连接到服务器
        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        sock.settimeout(10)  # 设置10秒超时
        sock.connect((host, port))
        print("✅ 成功连接到服务器")

        # 存储大值
        print("📝 存储大值...")
        cmd = f"set test_large 0 300 {len(test_value)}\r\n".encode()
        sock.send(cmd)
        sock.send(test_value + b'\r\n')

        # 等待响应
        response = sock.recv(1024).decode().strip()
        print(f"   SET结果: {response}")

        if response != "STORED":
            print("❌ 存储失败")
            return False

        # 关闭连接，重新连接以获取数据
        sock.close()
        time.sleep(0.1)

        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        sock.settimeout(10)
        sock.connect((host, port))

        # 获取大值
        print("📝 获取大值...")
        sock.send(b"get test_large\r\n")

        # 读取响应头
        header = sock.recv(1024).decode()
        if "VALUE test_large" not in header:
            print(f"❌ 获取失败: {header}")
            return False

        # 解析数据长度
        parts = header.split()
        if len(parts) >= 4:
            data_length = int(parts[3])
            print(f"   数据长度: {data_length} bytes")

            # 读取数据
            received_data = b''
            remaining = data_length
            while remaining > 0:
                chunk = sock.recv(min(4096, remaining))
                if not chunk:
                    break
                received_data += chunk
                remaining -= len(chunk)

            # 读取结束标记
            sock.recv(2)  # \r\n

            print(f"   接收数据长度: {len(received_data)} bytes")
            print(f"   数据匹配: {'✅ 是' if received_data == test_value else '❌ 否'}")

            # 验证数据完整性
            if len(received_data) == len(test_value) and received_data == test_value:
                print("✅ 大值处理功能正常工作！")
                return True
            else:
                print("❌ 数据不匹配")
                return False
        else:
            print(f"❌ 响应格式错误: {header}")
            return False

    except Exception as e:
        print(f"❌ 测试失败: {e}")
        return False
    finally:
        try:
            sock.close()
        except:
            pass

if __name__ == "__main__":
    success = test_simple_large_value()
    if success:
        print("\n🎉 大值处理功能验证成功！")
    else:
        print("\n💥 大值处理功能验证失败！")