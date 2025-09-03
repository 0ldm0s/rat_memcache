#!/usr/bin/env python3
"""
Memcached 协议测试脚本
用于测试 rat_memcached 服务器的基本功能
"""

import socket
import time
import sys

def send_command(sock, command):
    """发送命令并接收响应"""
    sock.sendall(command.encode('utf-8'))
    response = sock.recv(4096)
    return response.decode('utf-8')

def test_basic_operations(host='127.0.0.1', port=11211):
    """测试基本操作"""
    print(f"测试服务器: {host}:{port}")
    
    try:
        # 连接服务器
        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        sock.connect((host, port))
        print("✓ 连接服务器成功")
        
        # 测试 SET 操作
        set_cmd = "set test_key 0 60 11\r\nhello world\r\n"
        response = send_command(sock, set_cmd)
        print(f"SET 响应: {response.strip()}")
        
        # 测试 GET 操作
        get_cmd = "get test_key\r\n"
        response = send_command(sock, get_cmd)
        print(f"GET 响应: {response.strip()}")
        
        # 测试 DELETE 操作
        delete_cmd = "delete test_key\r\n"
        response = send_command(sock, delete_cmd)
        print(f"DELETE 响应: {response.strip()}")
        
        # 测试不存在的键
        get_cmd = "get nonexistent\r\n"
        response = send_command(sock, get_cmd)
        print(f"GET 不存在的键: {response.strip()}")
        
        sock.close()
        print("✓ 基本操作测试完成")
        
    except Exception as e:
        print(f"✗ 测试失败: {e}")
        return False
    
    return True

def test_multiple_operations(host='127.0.0.1', port=11211):
    """测试多个操作"""
    print("\n测试多个操作...")
    
    try:
        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        sock.connect((host, port))
        
        # 设置多个键值对
        keys = ['key1', 'key2', 'key3']
        values = ['value1', 'value2', 'value3']
        
        for i, (key, value) in enumerate(zip(keys, values)):
            set_cmd = f"set {key} 0 300 {len(value)}\r\n{value}\r\n"
            response = send_command(sock, set_cmd)
            print(f"SET {key}: {response.strip()}")
        
        # 获取所有键
        for key in keys:
            get_cmd = f"get {key}\r\n"
            response = send_command(sock, get_cmd)
            print(f"GET {key}: {response.strip()}")
        
        # 清理
        for key in keys:
            delete_cmd = f"delete {key}\r\n"
            response = send_command(sock, delete_cmd)
            print(f"DELETE {key}: {response.strip()}")
        
        sock.close()
        print("✓ 多个操作测试完成")
        
    except Exception as e:
        print(f"✗ 多个操作测试失败: {e}")
        return False
    
    return True

def test_ttl_expiration(host='127.0.0.1', port=11211):
    """测试 TTL 过期"""
    print("\n测试 TTL 过期...")
    
    try:
        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        sock.connect((host, port))
        
        # 设置一个 2 秒后过期的键
        set_cmd = "set temp_key 0 2 11\r\ntemp_value\r\n"
        response = send_command(sock, set_cmd)
        print(f"SET 临时键: {response.strip()}")
        
        # 立即获取应该存在
        get_cmd = "get temp_key\r\n"
        response = send_command(sock, get_cmd)
        print(f"GET 立即获取: {'找到' if 'temp_value' in response else '未找到'}")
        
        # 等待 3 秒让键过期
        print("等待 3 秒让键过期...")
        time.sleep(3)
        
        # 再次获取应该不存在
        response = send_command(sock, get_cmd)
        print(f"GET 过期后获取: {'找到' if 'temp_value' in response else '未找到'}")
        
        sock.close()
        print("✓ TTL 过期测试完成")
        
    except Exception as e:
        print(f"✗ TTL 测试失败: {e}")
        return False
    
    return True

def test_server_info(host='127.0.0.1', port=11211):
    """测试服务器信息命令"""
    print("\n测试服务器信息...")
    
    try:
        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        sock.connect((host, port))
        
        # 测试 stats 命令
        stats_cmd = "stats\r\n"
        response = send_command(sock, stats_cmd)
        print("STATS 响应:")
        for line in response.split('\r\n'):
            if line and not line.startswith('END'):
                print(f"  {line}")
        
        # 测试 version 命令
        version_cmd = "version\r\n"
        response = send_command(sock, version_cmd)
        print(f"VERSION: {response.strip()}")
        
        sock.close()
        print("✓ 服务器信息测试完成")
        
    except Exception as e:
        print(f"✗ 服务器信息测试失败: {e}")
        return False
    
    return True

def main():
    """主函数"""
    # 解析命令行参数
    host = '127.0.0.1'
    port = 11211
    
    if len(sys.argv) > 1:
        host = sys.argv[1]
    if len(sys.argv) > 2:
        port = int(sys.argv[2])
    
    print("=" * 50)
    print("Memcached 协议测试脚本")
    print("=" * 50)
    
    # 运行所有测试
    tests = [
        test_basic_operations,
        test_multiple_operations,
        test_ttl_expiration,
        test_server_info
    ]
    
    results = []
    for test in tests:
        results.append(test(host, port))
    
    print("\n" + "=" * 50)
    print("测试结果汇总:")
    print(f"总测试数: {len(tests)}")
    print(f"成功数: {sum(results)}")
    print(f"失败数: {len(results) - sum(results)}")
    
    if all(results):
        print("✓ 所有测试通过!")
        return 0
    else:
        print("✗ 部分测试失败!")
        return 1

if __name__ == "__main__":
    sys.exit(main())