#!/usr/bin/env python3
"""
RatMemcached缓存功能测试
"""

import socket
import sys

def test_cache_operations(host='127.0.0.1', port=11211):
    """测试基本的缓存操作"""

    def send_command(sock, command, expect_data=False):
        """发送命令并接收响应"""
        try:
            sock.sendall(command.encode() + b'\r\n')
            response = b''

            if expect_data:
                # 对于GET命令，需要读取多行响应
                while True:
                    chunk = sock.recv(1024)
                    if not chunk:
                        break
                    response += chunk
                    if b'END\r\n' in response:
                        break
            else:
                response = sock.recv(1024)

            return response.decode().strip()
        except Exception as e:
            print(f"命令执行失败: {e}")
            return ""

    print("测试缓存功能...")

    try:
        # 创建连接
        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        sock.connect((host, port))
        print("连接服务器成功")

        # 测试SET命令
        set_cmd = "set test_key 0 60 11\r\nhello world"
        response = send_command(sock, set_cmd)
        if response == "STORED":
            print("SET命令成功")
        else:
            print(f"SET命令失败: {response}")
            return False

        # 测试GET命令
        get_cmd = "get test_key"
        response = send_command(sock, get_cmd, expect_data=True)
        if "hello world" in response and "VALUE" in response:
            print("GET命令成功")
            print(f"   获取的值: hello world")
        else:
            print(f"GET命令失败: {response}")
            return False

        # 测试DELETE命令
        delete_cmd = "delete test_key"
        response = send_command(sock, delete_cmd)
        if response == "DELETED":
            print("DELETE命令成功")
        else:
            print(f"DELETE命令失败: {response}")
            return False

        # 验证删除后GET应该返回空
        response = send_command(sock, get_cmd, expect_data=True)
        if "END" in response and "VALUE" not in response:
            print("删除验证成功")
        else:
            print(f"删除验证失败: {response}")
            return False

        sock.close()
        return True

    except Exception as e:
        print(f"测试过程中出错: {e}")
        return False

def main():
    """主函数"""
    print("缓存功能测试")
    print("=" * 40)

    # 直接连接已运行的服务器进行测试
    success = test_cache_operations()

    if success:
        print("\n缓存功能测试通过！")
        return 0
    else:
        print("\n缓存功能测试失败")
        return 1

if __name__ == "__main__":
    sys.exit(main())