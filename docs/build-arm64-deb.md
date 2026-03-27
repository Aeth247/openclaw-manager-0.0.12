# 构建 ARM64 Linux deb 包

本文介绍如何为 ARM64（aarch64）Linux 设备构建 OpenClaw Manager 的 `.deb` 安装包。

## 方式一：本地 Docker 构建（推荐）

适用于 macOS / Linux / Windows（需安装 Docker Desktop）。

### 前置条件

- [Docker Desktop](https://www.docker.com/products/docker-desktop/) 已安装并运行
- 确保 Docker 已启用多架构支持（Docker Desktop 默认支持）

### 构建步骤

1. 在项目根目录执行：

```bash
docker build --platform linux/arm64 -f build-arm64.Dockerfile -t openclaw-arm64-builder .
```

> **国内网络提示**：`build-arm64.Dockerfile` 已配置清华镜像源（ubuntu-ports + npmmirror），如果你的网络可以直连，可自行修改为官方源。

2. 构建完成后，从镜像中提取 `.deb` 文件：

```bash
# 创建临时容器
docker create --platform linux/arm64 --name openclaw-extract openclaw-arm64-builder

# 拷贝 deb 包到本地
docker cp openclaw-extract:/app/src-tauri/target/aarch64-unknown-linux-gnu/release/bundle/deb/ ./dist-arm64/

# 清理临时容器
docker rm openclaw-extract
```

3. 构建产物位于 `dist-arm64/` 目录下。

### 构建耗时

首次构建约 5-10 分钟（取决于网络和机器性能），Docker 会缓存中间层，后续构建会快很多。

## 方式二：GitHub Actions 自动构建

项目的 CI 已配置 ARM64 Linux 构建（`.github/workflows/build.yml`），会在以下情况自动触发：

- 推送到 `main` 分支
- 创建 `v*` 格式的 tag（同时会创建 GitHub Release）
- 提交 Pull Request 到 `main`

构建完成后，可在 GitHub Actions 的 Artifacts 中下载 `artifacts-Linux-ARM64`，包含 `.deb` 和 `.AppImage` 文件。

### 手动触发发布

```bash
git tag v0.0.7
git push origin v0.0.7
```

推送 tag 后，CI 会自动构建所有平台的安装包并创建 Draft Release。

## 方式三：在 ARM64 设备上原生构建

如果你有 ARM64 Linux 设备（如树莓派 4/5、RK3588 等），可以直接在设备上构建。

### 安装依赖

```bash
# 系统依赖
sudo apt-get update
sudo apt-get install -y \
    curl wget build-essential pkg-config \
    libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev \
    patchelf libgtk-3-dev libsoup-3.0-dev libjavascriptcoregtk-4.1-dev file

# 安装 Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source ~/.cargo/env

# 安装 Node.js 22
curl -fsSL https://deb.nodesource.com/setup_22.x | sudo bash -
sudo apt-get install -y nodejs
```

### 构建

```bash
npm ci
npm run tauri build
```

构建产物在 `src-tauri/target/release/bundle/deb/` 目录下。

## 安装 deb 包

将 `.deb` 文件传到目标 ARM64 Linux 设备后：

```bash
sudo dpkg -i "OpenClaw Manager_0.0.7_arm64.deb"

# 如有依赖缺失，执行：
sudo apt-get install -f
```
