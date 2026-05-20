# CC Switch RPM 打包实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 构建 CC Switch 的 RPM 包，支持一键安装和桌面图标启动

**Architecture:** 预构建制品打包策略 — 先编译 `cc-switch-server`（server_only）和前端静态文件，再通过 RPM spec 搬运到 `/opt/cc-switch/`。启动器 Shell 脚本处理服务生命周期和浏览器打开逻辑。

**Tech Stack:** Rust (cc-switch-server), React/Vite (frontend), RPM spec, Shell

---

### 任务 1: 创建启动器脚本 `scripts/cc-switch`

**Files:**
- Create: `scripts/cc-switch`

- [ ] **Step 1: 编写启动器脚本**

```bash
#!/usr/bin/env bash
set -euo pipefail

LOCKFILE="/tmp/cc-switch.lock"
PORT=10245
URL="http://127.0.0.1:${PORT}"
PIDFILE="/tmp/cc-switch.pid"
BINARY="/opt/cc-switch/cc-switch-server"
FRONTEND_DIR="/opt/cc-switch/frontend"

# 唯一锁：防止重复启动
exec 100>"${LOCKFILE}"
flock -n 100 || {
    # 已有实例，检查端口是否存活
    if curl -sf "${URL}" >/dev/null 2>&1; then
        exec xdg-open "${URL}"
    else
        notify-send "CC Switch" "已有实例但服务未响应，请稍后重试"
    fi
    exit 0
}

# 取锁成功，确保 PIDFILE 由我们管理

# 先杀旧进程
if [[ -f "${PIDFILE}" ]] && kill -0 "$(cat "${PIDFILE}")" 2>/dev/null; then
    kill "$(cat "${PIDFILE}")" 2>/dev/null || true
    sleep 1
fi

# 清理旧端口
fuser -k "${PORT}/tcp" 2>/dev/null || true

# 右下角弹窗提示启动中
notify-send "CC Switch" "正在启动服务…"

# 设置环境变量，让 server 能找到前端文件
export CC_SWITCH_DIST_DIR="${FRONTEND_DIR}"

# 后台启动服务
nohup "${BINARY}" >/tmp/cc-switch.log 2>&1 &
echo $! > "${PIDFILE}"

# 等待服务就绪（最多等 15 秒）
for i in $(seq 1 30); do
    if curl -sf "${URL}" >/dev/null 2>&1; then
        break
    fi
    sleep 0.5
done

# 打开浏览器
xdg-open "${URL}" || true
```

注意点：
- `CC_SWITCH_DIST_DIR` 环境变量告诉 server 去哪里找前端静态文件
- `set -euo pipefail` 严格模式
- flock 的文件描述符 `100` 在整个脚本生命周期持有锁

- [ ] **Step 2: 添加执行权限**

```bash
chmod +x scripts/cc-switch
```

- [ ] **Step 3: 提交**

```bash
git add scripts/cc-switch
git commit -m "feat: add RPM launcher script with flock and notify-send"
```

---

### 任务 2: 创建桌面文件

**Files:**
- Create: `scripts/cc-switch.desktop`

- [ ] **Step 1: 编写 .desktop 文件**

```desktop
[Desktop Entry]
Type=Application
Name=CC Switch
Comment=All-in-One Assistant for Claude Code, Codex & Gemini CLI
Exec=/opt/cc-switch/cc-switch
Icon=cc-switch
Terminal=false
Categories=Utility;Development;
StartupNotify=true
```

- [ ] **Step 2: 提交**

```bash
git add scripts/cc-switch.desktop
git commit -m "feat: add desktop entry for RPM launcher"
```

---

### 任务 3: 准备图标

**Files:**
- Modify: `scripts/build-rpm.sh`（在后续任务中会用到）

目前项目中有 `src/assets/icons/app-icon.png`（32x32），需要生成 128x128 的桌面图标。

- [ ] **Step 1: 生成 128x128 图标**

```bash
mkdir -p scripts/rpm-assets
convert src/assets/icons/app-icon.png -resize 128x128 scripts/rpm-assets/cc-switch.png
```

如果没有 `convert`（ImageMagick），也可直接用 `pnpm` 或 `npx sharp-cli`。构建脚本会处理这个步骤。

- [ ] **Step 2: 提交**

```bash
git add scripts/rpm-assets/cc-switch.png
git commit -m "chore: add 128x128 desktop icon for RPM"
```

---

### 任务 4: 编写 RPM spec 文件

**Files:**
- Create: `cc-switch.spec`

- [ ] **Step 1: 编写 spec 文件**

```spec
%define _rpmdir %{_topdir}
%define _srcrpmdir %{_topdir}

Name:       cc-switch
Version:    3.15.0
Release:    1%{?dist}
Summary:    All-in-One Assistant for Claude Code, Codex & Gemini CLI
License:    MIT
URL:        https://github.com/lzyyzznl/cc-switch
Source0:    cc-switch-%{version}.tar.gz
BuildArch:  x86_64

Requires:   libnotify, xdg-utils, curl

%description
CC Switch is an all-in-one assistant for Claude Code, Codex & Gemini CLI.
This package provides the web-based UI mode (server-only) with a desktop
launcher that starts the service and opens the browser.

%prep
%setup -q -n cc-switch-%{version}

%install
# 主程序目录
mkdir -p %{buildroot}/opt/cc-switch/frontend/assets
mkdir -p %{buildroot}/usr/share/applications
mkdir -p %{buildroot}/usr/share/icons/hicolor/128x128/apps

# 安装二进制
install -m 755 cc-switch-server %{buildroot}/opt/cc-switch/

# 安装启动器脚本
install -m 755 cc-switch %{buildroot}/opt/cc-switch/

# 安装前端文件
install -m 644 frontend/index.html %{buildroot}/opt/cc-switch/frontend/
install -m 644 frontend/assets/* %{buildroot}/opt/cc-switch/frontend/assets/

# 安装版本文件
echo "%{version}-%{release}" > %{buildroot}/opt/cc-switch/VERSION

# 安装桌面文件和图标
install -m 644 cc-switch.desktop %{buildroot}/usr/share/applications/
install -m 644 cc-switch.png %{buildroot}/usr/share/icons/hicolor/128x128/apps/

%post
# 安装/升级时：杀掉旧进程、清锁
if [ -f /tmp/cc-switch.pid ]; then
    kill "$(cat /tmp/cc-switch.pid)" 2>/dev/null || true
fi
fuser -k 10245/tcp 2>/dev/null || true
rm -f /tmp/cc-switch.lock /tmp/cc-switch.pid

# 刷新桌面数据库
gtk-update-icon-cache /usr/share/icons/hicolor 2>/dev/null || true
update-desktop-database 2>/dev/null || true

%preun
if [ "$1" -eq 0 ]; then
    # 真正卸载（非升级）时清理进程
    if [ -f /tmp/cc-switch.pid ]; then
        kill "$(cat /tmp/cc-switch.pid)" 2>/dev/null || true
    fi
    fuser -k 10245/tcp 2>/dev/null || true
    rm -f /tmp/cc-switch.lock /tmp/cc-switch.pid
fi

%files
%dir /opt/cc-switch/
/opt/cc-switch/cc-switch-server
/opt/cc-switch/cc-switch
%dir /opt/cc-switch/frontend/
/opt/cc-switch/frontend/index.html
/opt/cc-switch/frontend/assets/*
/opt/cc-switch/VERSION
/usr/share/applications/cc-switch.desktop
/usr/share/icons/hicolor/128x128/apps/cc-switch.png

%changelog
* Wed May 20 2026 李泽宇 <lzy@zluck.com> - 3.15.0-1
- Initial RPM package for CC Switch server-only mode
```

- [ ] **Step 2: 提交**

```bash
git add cc-switch.spec
git commit -m "feat: add RPM spec for server-only packaging"
```

---

### 任务 5: 编写构建脚本 `scripts/build-rpm.sh`

**Files:**
- Create: `scripts/build-rpm.sh`

这个脚本封装从源码到 RPM 的完整流程。

- [ ] **Step 1: 编写构建脚本**

```bash
#!/usr/bin/env bash
# scripts/build-rpm.sh — 构建 CC Switch RPM 包
set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
VERSION="3.15.0"
RELEASE="1"
PKG_DIR="$(mktemp -d)"
PKG_NAME="cc-switch-${VERSION}"
PKG_PATH="${PKG_DIR}/${PKG_NAME}"

echo "=== CC Switch RPM Builder ==="

# 1. 构建后端
echo "[1/4] Building cc-switch-server binary..."
cd "${PROJECT_ROOT}"
cargo build --release --features server_only
BINARY="${PROJECT_ROOT}/src-tauri/target/release/cc-switch-server"

# 2. 构建前端
echo "[2/4] Building frontend..."
pnpm install --frozen-lockfile
pnpm build:renderer

# 3. 组装 RPM 目录结构
echo "[3/4] Assembling package directory..."
mkdir -p "${PKG_PATH}/frontend/assets"
cp "${BINARY}" "${PKG_PATH}/cc-switch-server"
cp "${PROJECT_ROOT}/scripts/cc-switch" "${PKG_PATH}/"
cp "${PROJECT_ROOT}/dist/index.html" "${PKG_PATH}/frontend/"
cp "${PROJECT_ROOT}/dist/assets/"* "${PKG_PATH}/frontend/assets/"
cp "${PROJECT_ROOT}/scripts/rpm-assets/cc-switch.png" "${PKG_PATH}/"
cp "${PROJECT_ROOT}/scripts/cc-switch.desktop" "${PKG_PATH}/"
echo "${VERSION}-${RELEASE}" > "${PKG_PATH}/VERSION"

# 4. 打包为 tar.gz（RPM Source0）
echo "[4/4] Building RPM..."
tar czf "${PKG_DIR}/${PKG_NAME}.tar.gz" -C "${PKG_DIR}" "${PKG_NAME}"

# 准备 RPM 构建环境
RPM_TOPDIR="$(mktemp -d)"
mkdir -p "${RPM_TOPDIR}"/{SOURCES,SPECS,RPMS,SRPMS,BUILD}
cp "${PKG_DIR}/${PKG_NAME}.tar.gz" "${RPM_TOPDIR}/SOURCES/"
cp "${PROJECT_ROOT}/cc-switch.spec" "${RPM_TOPDIR}/SPECS/"

# 构建 RPM
rpmbuild --define "_topdir ${RPM_TOPDIR}" -bb "${RPM_TOPDIR}/SPECS/cc-switch.spec"
# 或者用 --define "_rpmdir <path>" 控制输出目录

# 提取结果
cp "${RPM_TOPDIR}/RPMS/x86_64/"*.rpm "${PROJECT_ROOT}/"
echo "=== RPM built successfully ==="
ls -lh "${PROJECT_ROOT}/cc-switch-"*.rpm

# 清理
rm -rf "${PKG_DIR}" "${RPM_TOPDIR}"
```

- [ ] **Step 2: 添加执行权限**

```bash
chmod +x scripts/build-rpm.sh
```

- [ ] **Step 3: 提交**

```bash
git add scripts/build-rpm.sh
git commit -m "feat: add RPM build script"
```

---

### 任务 6: 自测

- [ ] **Step 1: 验证脚本语法**

```bash
bash -n scripts/cc-switch
bash -n scripts/build-rpm.sh
```

- [ ] **Step 2: 检查 spec 语法**

```bash
rpmspec --parse cc-switch.spec  >/dev/null || echo "spec error"
```

- [ ] **Step 3: 完整构建测试（可选，需要 build 环境）**

```bash
# 这个步骤需要 Rust + Node.js + rpmbuild 环境
cd /home/0668001050/workspace/cc-switch
bash scripts/build-rpm.sh
```
