#!/usr/bin/env python3
"""
RatMemcached服务器全面功能测试脚本
"""

import socket
import time
import sys

def test_basic_memcached_protocol():
    """使用原始socket测试Memcached协议"""
    print("=== 原始Memcached协议测试 ===")

    try:
        # 连接到服务器
        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        sock.connect(('127.0.0.1', 11211))
        print("✓ 连接成功")

        # 测试set命令
        set_cmd = b"set test_key 0 60 11\r\nhello world\r\n"
        sock.send(set_cmd)
        response = sock.recv(1024)
        if b'STORED' in response:
            print("✓ SET命令成功")
        else:
            print(f"❌ SET命令失败: {response}")
            return False

        # 测试get命令
        get_cmd = b"get test_key\r\n"
        sock.send(get_cmd)
        response = sock.recv(1024)
        if b'hello world' in response:
            print("✓ GET命令成功")
        else:
            print(f"❌ GET命令失败: {response}")
            return False

        # 测试delete命令
        del_cmd = b"delete test_key\r\n"
        sock.send(del_cmd)
        response = sock.recv(1024)
        if b'DELETED' in response:
            print("✓ DELETE命令成功")
        else:
            print(f"❌ DELETE命令失败: {response}")
            return False

        # 测试stats命令
        stats_cmd = b"stats\r\n"
        sock.send(stats_cmd)
        response = sock.recv(4096)  # stats可能返回较多数据
        if b'STATS' in response or b'curr_items' in response:
            print("✓ STATS命令成功")
        else:
            print(f"❌ STATS命令可能失败: {response[:200]}...")

        # 测试version命令
        version_cmd = b"version\r\n"
        sock.send(version_cmd)
        response = sock.recv(1024)
        if b'VERSION' in response:
            print("✓ VERSION命令成功")
        else:
            print(f"❌ VERSION命令失败: {response}")

        sock.close()
        return True

    except Exception as e:
        print(f"❌ 原始协议测试失败: {e}")
        return False

def test_multiple_clients():
    """测试多客户端并发访问"""
    print("\n=== 多客户端并发测试 ===")

    import threading

    def client_worker(client_id):
        try:
            sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
            sock.connect(('127.0.0.1', 11211))

            # 每个客户端执行10次set/get操作
            for i in range(10):
                key = f"client_{client_id}_key_{i}"
                value = f"client_{client_id}_value_{i}"

                # set
                set_cmd = f"set {key} 0 60 {len(value)}\r\n{value}\r\n".encode()
                sock.send(set_cmd)
                response = sock.recv(1024)

                # get
                get_cmd = f"get {key}\r\n".encode()
                sock.send(get_cmd)
                response = sock.recv(1024)

                if value.encode() not in response:
                    print(f"❌ 客户端{client_id}: 数据验证失败")
                    return False

            sock.close()
            return True

        except Exception as e:
            print(f"❌ 客户端{client_id}测试失败: {e}")
            return False

    # 创建5个客户端线程
    threads = []
    for i in range(5):
        thread = threading.Thread(target=client_worker, args=(i,))
        threads.append(thread)
        thread.start()

    # 等待所有线程完成
    for thread in threads:
        thread.join()

    print("✓ 多客户端并发测试完成")
    return True

def test_large_value():
    """测试大值存储"""
    print("\n=== 大值存储测试 ===")

    try:
        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        sock.connect(('127.0.0.1', 11211))

        # 测试不同大小的数据
        sizes = [100, 1000, 10000, 50000]  # 100B, 1KB, 10KB, 50KB

        for size in sizes:
            value = 'x' * size
            key = f"large_key_{size}"

            # set
            set_cmd = f"set {key} 0 60 {len(value)}\r\n{value}\r\n".encode()
            sock.send(set_cmd)
            response = sock.recv(1024)

            if b'STORED' not in response:
                print(f"❌ 大值SET失败 ({size}B): {response}")
                return False

            # get
            get_cmd = f"get {key}\r\n".encode()
            sock.send(get_cmd)
            response = sock.recv(size + 200)  # 预留足够空间

            if value.encode() not in response:
                print(f"❌ 大值GET失败 ({size}B)")
                return False

            print(f"✓ {size}B数据处理成功")

            # delete
            del_cmd = f"delete {key}\r\n".encode()
            sock.send(del_cmd)
            sock.recv(1024)  # 读取响应

        sock.close()
        return True

    except Exception as e:
        print(f"❌ 大值测试失败: {e}")
        return False

def test_performance():
    """简单的性能测试"""
    print("\n=== 性能测试 ===")

    try:
        sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        sock.connect(('127.0.0.1', 11211))

        # 测试1000次set操作
        start_time = time.time()

        for i in range(1000):
            key = f"perf_key_{i}"
            value = f"perf_value_{i}"
            set_cmd = f"set {key} 0 60 {len(value)}\r\n{value}\r\n".encode()
            sock.send(set_cmd)
            response = sock.recv(1024)  # 读取STORED响应

        set_time = time.time() - start_time

        # 测试1000次get操作
        start_time = time.time()

        for i in range(1000):
            key = f"perf_key_{i}"
            get_cmd = f"get {key}\r\n".encode()
            sock.send(get_cmd)
            response = sock.recv(1024)  # 读取值和END

        get_time = time.time() - start_time

        print(f"✓ SET操作: 1000次耗时 {set_time:.3f}秒, QPS: {1000/set_time:.0f}")
        print(f"✓ GET操作: 1000次耗时 {get_time:.3f}秒, QPS: {1000/get_time:.0f}")

        # 清理数据
        for i in range(1000):
            key = f"perf_key_{i}"
            del_cmd = f"delete {key}\r\n".encode()
            sock.send(del_cmd)
            sock.recv(1024)

        sock.close()
        return True

    except Exception as e:
        print(f"❌ 性能测试失败: {e}")
        return False

def main():
    print("🧪 RatMemcached 服务器全面功能测试")
    print("=" * 60)
    print(f"测试时间: {time.strftime('%Y-%m-%d %H:%M:%S')}")
    print(f"服务器地址: 127.0.0.1:11211")
    print("=" * 60)

    tests = [
        ("基本Memcached协议", test_basic_memcached_protocol),
        ("大值存储", test_large_value),
        ("多客户端并发", test_multiple_clients),
        ("性能测试", test_performance),
    ]

    passed = 0
    failed = 0

    for test_name, test_func in tests:
        print(f"\n🔍 {test_name}")
        try:
            if test_func():
                print(f"✅ {test_name} 通过")
                passed += 1
            else:
                print(f"❌ {test_name} 失败")
                failed += 1
        except Exception as e:
            print(f"❌ {test_name} 异常: {e}")
            failed += 1

    print("\n" + "=" * 60)
    print("📊 测试结果:")
    print(f"✅ 通过: {passed}")
    print(f"❌ 失败: {failed}")
    print(f"📈 成功率: {passed/(passed+failed)*100:.1f}%")

    if failed == 0:
        print("\n🎉 所有测试通过！RatMemcached服务器运行完美！")
        return True
    else:
        print(f"\n⚠️  有{failed}个测试失败")
        return False

if __name__ == "__main__":
    success = main()
    sys.exit(0 if success else 1)