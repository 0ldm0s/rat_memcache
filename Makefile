# Makefile for RatMemCache (MSYS2 compatible)

# 检测操作系统
ifeq ($(OS),Windows_NT)
    OS_DETECTED := Windows
    DIST_DIR := dist-mingw64
else
    OS_DETECTED := $(shell uname -s)
    DIST_DIR := dist-$(shell uname -s | tr '[:upper:]' '[:lower:]')
endif

# 项目配置
PROJECT := rat_memcache
BIN_NAME := rat_memcached
TARGET := release
BUILD_DIR := target/$(TARGET)
EXE_FILE := $(BUILD_DIR)/$(BIN_NAME).exe

# MSYS2路径配置
MSYS2_PATH := /c/msys64
MINGW64_BIN := $(MSYS2_PATH)/mingw64/bin

# 默认目标
all: build

# 构建项目
build:
	@echo "Building $(PROJECT) for $(OS_DETECTED)..."
	cargo build --$(TARGET)

# Windows mingw64专用分发包
dist-mingw64: build copy-mingw64-deps
	@echo "Creating mingw64 distribution package..."
	@mkdir -p $(DIST_DIR)
	@cp "$(EXE_FILE)" "$(DIST_DIR)/"
	@echo "Mingw64 distribution package created in $(DIST_DIR)"

# Linux分发包
dist-linux: build
	@echo "Creating Linux distribution package..."
	@mkdir -p $(DIST_DIR)
	@cp "$(EXE_FILE)" "$(DIST_DIR)/"
	@echo "Linux distribution package created in $(DIST_DIR)"

# 复制mingw64依赖的DLL文件
copy-mingw64-deps:
	@echo "Copying mingw64 dependencies..."
	@mkdir -p $(DIST_DIR)
	@if [ -f "$(MINGW64_BIN)/libmelange_db.dll" ]; then cp "$(MINGW64_BIN)/libmelange_db.dll" "$(DIST_DIR)/" && echo "Copied: libmelange_db.dll"; fi
	@if [ -f "$(MINGW64_BIN)/libgcc_s_seh-1.dll" ]; then cp "$(MINGW64_BIN)/libgcc_s_seh-1.dll" "$(DIST_DIR)/" && echo "Copied: libgcc_s_seh-1.dll"; fi
	@if [ -f "$(MINGW64_BIN)/libstdc++-6.dll" ]; then cp "$(MINGW64_BIN)/libstdc++-6.dll" "$(DIST_DIR)/" && echo "Copied: libstdc++-6.dll"; fi
	@if [ -f "$(MINGW64_BIN)/libwinpthread-1.dll" ]; then cp "$(MINGW64_BIN)/libwinpthread-1.dll" "$(DIST_DIR)/" && echo "Copied: libwinpthread-1.dll"; fi
	@if [ -f "$(MINGW64_BIN)/zlib1.dll" ]; then cp "$(MINGW64_BIN)/zlib1.dll" "$(DIST_DIR)/" && echo "Copied: zlib1.dll"; fi
	@if [ -f "$(MINGW64_BIN)/liblz4.dll" ]; then cp "$(MINGW64_BIN)/liblz4.dll" "$(DIST_DIR)/" && echo "Copied: liblz4.dll"; fi
	@if [ -f "$(MINGW64_BIN)/libbz2-1.dll" ]; then cp "$(MINGW64_BIN)/libbz2-1.dll" "$(DIST_DIR)/" && echo "Copied: libbz2-1.dll"; fi
	@if [ -f "$(MINGW64_BIN)/libzstd.dll" ]; then cp "$(MINGW64_BIN)/libzstd.dll" "$(DIST_DIR)/" && echo "Copied: libzstd.dll"; fi

# 安装mingw64依赖
install-mingw64-deps:
	@echo "Installing mingw64 dependencies..."
	pacman -S --noconfirm --needed mingw-w64-x86_64-gcc mingw-w64-x86_64-zlib mingw-w64-x86_64-lz4 mingw-w64-x86_64-bzip2 mingw-w64-x86_64-zstd

# 清理构建文件
clean:
	@echo "Cleaning build artifacts..."
	cargo clean
	@rm -rf $(DIST_DIR)
	@echo "Clean complete"

# 运行测试
test:
	@echo "Running tests..."
	cargo test

# 显示帮助信息
help:
	@echo "Available targets:"
	@echo "  build               - Build the project"
	@echo "  dist-mingw64        - Build and create mingw64 distribution package"
	@echo "  dist-linux          - Build and create Linux distribution package"
	@echo "  copy-mingw64-deps   - Copy mingw64 dependent DLLs"
	@echo "  install-mingw64-deps - Install mingw64 dependencies"
	@echo "  clean               - Clean build artifacts"
	@echo "  test                - Run tests"
	@echo "  help                - Show this help message"

.PHONY: all build dist-mingw64 dist-linux copy-mingw64-deps install-mingw64-deps clean test help