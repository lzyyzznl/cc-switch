# CC Switch RPM 打包设计文档

## 概述

将 cc-switch 的前端（Vite 构建产物）和后端（cc-switch-server binary）打包为 RPM，
实现一键安装、桌面图标启动体验。

## 运行模式

使用 `server_only` 模式（非 Tauri 桌面模式）。安装后点击图标：
启动器脚本检查服务状态 → 未运行则启动后台服务 → 打开浏览器访问 WebUI。

## 安装目录结构

```
/opt/cc-switch/
├── cc-switch-server        # Rust 二进制 (server_only 模式)
├── cc-switch               # 启动器 Shell 脚本
├── frontend/
│   ├── index.html
│   └── assets/
└── VERSION
```

## 启动器脚本 (`/opt/cc-switch/cc-switch`)

Shell 脚本，实现以下功能：

1. **唯一锁**：使用 `flock /tmp/cc-switch.lock` 防止重复实例
   - 加锁失败 → 检查端口存活 → 存活则直接 `xdg-open`
   - 加锁失败且服务不存活 → `notify-send` 提示用户稍后重试

2. **旧进程清理**：读取 `/tmp/cc-switch.pid` 杀旧进程，`fuser -k 10245/tcp` 清理端口

3. **启动通知**：`notify-send "CC Switch" "正在启动服务…"`

4. **后台启动**：`nohup cc-switch-server > /tmp/cc-switch.log 2>&1 &`

5. **等待就绪**：轮询 `127.0.0.1:10245` 最多 15 秒（30 次 × 0.5 秒间隔）

6. **打开浏览器**：`xdg-open http://127.0.0.1:10245`

## 桌面文件 (`/usr/share/applications/cc-switch.desktop`)

```desktop
[Desktop Entry]
Type=Application
Name=CC Switch
Exec=/opt/cc-switch/cc-switch
Icon=cc-switch
Terminal=false
Categories=Utility;Development;
StartupNotify=true
```

## RPM spec 设计

### 构建策略：预构建制品打包（方案 B）

构建和打包分离：
- 构建阶段：`cargo build --release --features server_only` + `pnpm build:renderer`
- 打包阶段：RPM spec 只负责搬运和注册

### spec 核心架构

- **Name**: cc-switch
- **Source0**: 预构建的 tar.gz（包含二进制 + 前端产物 + 启动脚本 + 图标）
- **Requires**: libnotify, xdg-utils, curl
- **%post**: 杀旧进程、清锁文件、更新桌面数据库
- **%preun**: 卸载时清理进程

## RPM 生命周期事件

| 事件 | 动作 |
|------|------|
| `%post` (安装/升级) | 杀旧进程 (`kill $(cat /tmp/cc-switch.pid)` + `fuser -k 10245/tcp`)、清锁、刷新桌面数据库 |
| `%preun` (卸载) | 仅当 `$1 -eq 0`（真正卸载）时杀进程并清除锁和 PID 文件 |

## 依赖

- `libnotify` — `notify-send` 系统通知
- `xdg-utils` — `xdg-open` 打开浏览器
- `curl` — 健康检查端口存活

## 构建流程

```
# 1. 构建后端
cargo build --release --features server_only

# 2. 构建前端
pnpm build:renderer

# 3. 组装目录结构
mkdir -p pkg/opt/cc-switch/frontend
cp src-tauri/target/release/cc-switch-server pkg/opt/cc-switch/
cp scripts/cc-switch pkg/opt/cc-switch/
cp dist/index.html pkg/opt/cc-switch/frontend/
cp dist/assets/* pkg/opt/cc-switch/frontend/assets/
echo "3.15.0" > pkg/opt/cc-switch/VERSION

# 4. 打包
tar czf cc-switch-3.15.0.tar.gz -C pkg .

# 5. 构建 RPM
rpmbuild -ba cc-switch.spec
```
