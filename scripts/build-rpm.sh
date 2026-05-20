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
cd "${PROJECT_ROOT}/src-tauri"
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

# 提取结果（spec 中的 _rpmdir 默认指向 %_topdir/RPMS）
cp "${RPM_TOPDIR}/RPMS/x86_64/"*.rpm "${PROJECT_ROOT}/"
echo "=== RPM built successfully ==="
ls -lh "${PROJECT_ROOT}/cc-switch-"*.rpm

# 清理
rm -rf "${PKG_DIR}" "${RPM_TOPDIR}"
