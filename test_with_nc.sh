#!/bin/bash

echo "🧪 使用nc测试大值处理..."

# 设置测试数据 (15KB)
DATA_SIZE=15360
KEY="test_nc_large"

echo "📝 1. 存储大值 ($DATA_SIZE bytes)..."
echo "set $KEY 0 300 $DATA_SIZE" | nc localhost 11211 &
NC_PID=$!

# 等待一下让nc准备好
sleep 0.1

# 发送数据 (使用X字符填充)
printf "%${DATA_SIZE}s" | tr ' ' 'X' | nc localhost 11211
wait $NC_PID

echo ""
echo "📝 2. 获取大值..."
echo "get $KEY" | nc localhost 11211 | head -3

echo ""
echo "📝 3. 验证数据大小..."
echo "get $KEY" | nc localhost 11211 | wc -c

echo ""
echo "✅ 测试完成！"