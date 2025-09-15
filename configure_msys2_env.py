#!/usr/bin/env python3
"""
MSYS2环境变量配置脚本
用于配置RocksDB构建所需的环境变量
"""

import os
import sys
from pathlib import Path

def get_msys2_path():
    """获取MSYS2安装路径"""
    default_path = "C:\\msys64"

    while True:
        path = input(f"请输入MSYS2安装路径 (默认: {default_path}): ").strip()
        if not path:
            path = default_path

        if os.path.exists(path):
            return path
        else:
            print(f"错误: 路径 {path} 不存在")
            retry = input("是否重试? (y/n): ").strip().lower()
            if retry != 'y':
                return None

def check_rocksdb_installation(msys2_path):
    """检查RocksDB是否已安装"""
    lib_path = Path(msys2_path) / "mingw64" / "lib"
    include_path = Path(msys2_path) / "mingw64" / "include"

    # 检查头文件
    rocksdb_header = include_path / "rocksdb" / "c.h"
    if not rocksdb_header.exists():
        print(f"错误: RocksDB头文件不存在: {rocksdb_header}")
        print("请在MSYS2中运行以下命令安装RocksDB:")
        print("  pacman -S mingw-w64-x86_64-rocksdb")
        return False

    # 检查库文件
    lib_files = ["librocksdb.dll.a", "librocksdb.a"]
    lib_found = False
    for lib_file in lib_files:
        lib_path_full = lib_path / lib_file
        if lib_path_full.exists():
            print(f"✅ 找到RocksDB库文件: {lib_path_full}")
            lib_found = True
            break

    if not lib_found:
        print(f"错误: 未找到RocksDB库文件，预期在: {lib_path}")
        return False

    return True

def check_bash_profile(msys2_path):
    """检查.bash_profile文件是否包含所需的环境变量"""
    bash_profile = Path(msys2_path) / "home" / os.environ.get("USERNAME", os.environ.get("USER", "default")) / ".bash_profile"

    # 如果.bash_profile不存在，检查其他可能的配置文件
    if not bash_profile.exists():
        print(f".bash_profile 不存在: {bash_profile}")
        return False

    required_vars = [
        "LIBROCKSDB_STATIC",
        "ROCKSDB_LIB_DIR",
        "ROCKSDB_INCLUDE_DIR"
    ]

    with open(bash_profile, 'r', encoding='utf-8') as f:
        content = f.read()

    missing_vars = []
    for var in required_vars:
        if var not in content:
            missing_vars.append(var)

    if not missing_vars:
        print("✅ .bash_profile 已包含所有必需的环境变量")
        return True
    else:
        print(f"❌ .bash_profile 缺少以下环境变量: {', '.join(missing_vars)}")
        return False

def add_env_to_bash_profile(msys2_path):
    """将环境变量添加到.bash_profile文件"""
    bash_profile = Path(msys2_path) / "home" / os.environ.get("USERNAME", os.environ.get("USER", "default")) / ".bash_profile"

    # 创建备份
    if bash_profile.exists():
        backup_path = bash_profile.with_suffix('.backup')
        print(f"创建备份: {backup_path}")
        import shutil
        shutil.copy2(bash_profile, backup_path)

    mingw64_path = Path(msys2_path) / "mingw64"
    env_vars = f"""
# RocksDB环境变量配置 - 由RatMemCache项目添加
export LIBROCKSDB_STATIC="1"
export ROCKSDB_LIB_DIR="{mingw64_path}\\lib"
export ROCKSDB_INCLUDE_DIR="{mingw64_path}\\include"

"""

    with open(bash_profile, 'a', encoding='utf-8') as f:
        f.write(env_vars)

    print(f"✅ 环境变量已添加到: {bash_profile}")
    print("\n请重新启动MSYS2终端或运行以下命令使配置生效:")
    print("  source ~/.bash_profile")

def main():
    """主函数"""
    print("MSYS2环境变量配置脚本")
    print("=" * 50)

    # 获取MSYS2路径
    msys2_path = get_msys2_path()
    if not msys2_path:
        print("错误: 未找到有效的MSYS2路径")
        return 1

    print(f"使用MSYS2路径: {msys2_path}")

    # 检查RocksDB安装
    if not check_rocksdb_installation(msys2_path):
        return 1

    # 检查.bash_profile
    if check_bash_profile(msys2_path):
        print("\n配置已完成，无需修改")
        return 0

    # 询问是否添加环境变量
    print("\n.bash_profile需要添加环境变量配置")
    choice = input("是否现在添加这些环境变量? (y/n): ").strip().lower()
    if choice == 'y':
        add_env_to_bash_profile(msys2_path)
        print("\n配置完成！")
    else:
        print("取消配置")
        return 1

    return 0

if __name__ == "__main__":
    sys.exit(main())