#!/usr/bin/env python3
"""
测试压缩阈值功能的Python脚本
使用telnet协议连接memcached服务器进行测试
"""

import telnetlib
import time
import sys

def test_compression_thresholds():
    """测试不同大小的数据是否按预期进行压缩"""

    host = '127.0.0.1'
    port = 11211

    try:
        # 连接服务器
        tn = telnetlib.Telnet(host, port, timeout=10)
        print(f"✅ 成功连接到 {host}:{port}")

        # 测试数据集 - 不同大小的数据
        test_cases = [
            # (key, data_size, description)
            ("small_50", 50, "小于最小阈值(128bytes) - 不应该压缩"),
            ("small_100", 100, "小于最小阈值(128bytes) - 不应该压缩"),
            ("medium_200", 200, "在阈值范围内(128-1048576) - 应该压缩"),
            ("medium_1024", 1024, "在阈值范围内(128-1048576) - 应该压缩"),
            ("medium_8192", 8192, "在阈值范围内(128-1048576) - 应该压缩"),
            ("large_15K", 15 * 1024, "超过大值阈值(10KB)但在压缩范围内 - 应该压缩"),
            ("large_2M", 2 * 1024 * 1024, "大于最大阈值(1MB) - 不应该压缩"),
        ]

        for key, data_size, description in test_cases:
            print(f"\n🧪 测试: {description}")
            print(f"   数据大小: {data_size} bytes")

            # 生成测试数据 - 使用高度重复的模式确保良好的压缩率
            if data_size <= 1024:
                # 小数据使用重复字符，确保高压缩率
                test_data = b'A' * data_size
            else:
                # 大数据使用重复的长模式，确保高压缩率
                repeat_pattern = b'AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA'  # 32个A
                test_data = repeat_pattern * (data_size // 32 + 1)
                test_data = test_data[:data_size]

            # 设置数据
            set_cmd = f"set {key} 0 0 {len(test_data)}\r\n"
            tn.write(set_cmd.encode())
            tn.write(test_data + b"\r\n")

            # 获取响应
            response = tn.read_until(b"\r\n").decode().strip()
            if response == "STORED":
                print(f"   ✅ 数据设置成功")
            else:
                print(f"   ❌ 数据设置失败: {response}")
                continue

            # 获取数据
            get_cmd = f"get {key}\r\n"
            tn.write(get_cmd.encode())

            # 读取响应
            response = tn.read_until(b"END\r\n").decode()

            if "VALUE" in response:
                # 计算实际传输的数据大小
                lines = response.split('\r\n')
                if len(lines) >= 3:
                    actual_data = lines[1]  # 数据行
                    actual_size = len(actual_data.encode())

                    print(f"   📊 原始大小: {data_size} bytes")
                    print(f"   📦 传输大小: {actual_size} bytes")

                    # 判断是否被压缩
                    if actual_size < data_size:
                        ratio = actual_size / data_size
                        print(f"   🗜️  已压缩，压缩率: {ratio:.2%}")
                    elif actual_size == data_size:
                        print(f"   📦 未压缩")
                    else:
                        print(f"   ⚠️  传输数据大于原始数据（异常）")
            else:
                print(f"   ❌ 获取数据失败")

            # 删除测试数据
            delete_cmd = f"delete {key}\r\n"
            tn.write(delete_cmd.encode())
            tn.read_until(b"\r\n")  # 读取响应

        print("\n🎉 压缩阈值测试完成！")

    except Exception as e:
        print(f"❌ 测试失败: {e}")
        return False
    finally:
        try:
            tn.close()
        except:
            pass

    return True

if __name__ == "__main__":
    print("🚀 开始压缩阈值功能测试")
    print("=" * 50)

    success = test_compression_thresholds()

    if success:
        print("\n✅ 所有测试完成")
        sys.exit(0)
    else:
        print("\n❌ 测试失败")
        sys.exit(1)