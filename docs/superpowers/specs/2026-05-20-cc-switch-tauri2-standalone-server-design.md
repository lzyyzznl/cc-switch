---
name: cc-switch-tauri2-standalone-server
description: 将 cc-switch 从 Tauri 2 桌面应用改造为 Rust 独立 HTTP 服务 + 浏览器前端的架构设计
metadata:
  type: spec
  status: draft
  created: 2026-05-20
---

# cc-switch Tauri 2 → Standalone HTTP Server 改造设计

## 背景

cc-switch v3.15.0 是一个 Tauri 2 桌面应用（React + Rust），目标部署系统为 **NewStartOS V4.4.2-ZTE (el8)**，该系统：

- 只有 `webkit2gtk-4.0`（版本 2.30.4），Tauri 2 要求 `webkit2gtk-4.1`（最低 2.38）
- GLIBC 2.31，低于 Tauri 2 推荐的 2.34+

无法运行 Tauri 2 应用。需要以最小改造成本改造为兼容当前系统的框架，且改造后的代码需能继续合入开源社区的上游代码。

## 目标架构

```
浏览器 (Chrome/Firefox)           Rust 二进制 (localhost:10245)
┌──────────────────┐   fetch()   ┌────────────────────────────────┐
│ React SPA         │◄──────────►│ Axum HTTP Server               │
│ (未改组件)         │   SSE ◄────│                                │
│                    │            │  /api/:command  → Handler      │
│ tauri-shim/ 适配层 │            │  /events (SSE)  → EventBus     │
│ (替换 @tauri-apps) │            │  / (static)     → ServeDir     │
└──────────────────┘            └────────────────────────────────┘
   无 WebKitGTK 依赖              单二进制，无外部运行时依赖
   无 GLIBC 版本限制
```

## 变更范围

### 不改的代码

- 所有 Rust 业务逻辑模块：`proxy/`, `services/`, `database/`, `session_manager/`, `mcp/`, `deeplink/`, `config.rs`, `settings.rs`, `provider.rs`, `hermes_config.rs` 等
- 所有前端 React 组件、React Query hooks
- 前端 API 模块 `src/lib/api/*.ts`（导入路径不变）

### 改动的代码

| 文件 | 改动类型 | 说明 |
|---|---|---|
| `src-tauri/src/main_server.rs` | **新增** | HTTP 服务入口 |
| `src-tauri/src/lib.rs` | **修改** | 抽离公共初始化逻辑 |
| `src-tauri/src/commands/*.rs` | **修改** | `State<'_, AppState>` → `Arc<AppState>` |
| `src-tauri/src/error.rs` | **修改** | 移除 Tauri 依赖的 Error 变体 |
| `src-tauri/Cargo.toml` | **修改** | 移除 tauri 依赖，保留 axum |
| `src-tauri/src/handlers.rs` | **新增** | 命令 → HTTP handler 映射（~150 个） |
| `src-tauri/build.rs` | **修改** | 移除 tauri_build |
| `src/tauri-shim/` | **新增** | 8 个前端适配文件 |
| `vite.config.ts` | **修改** | 添加 resolve alias |
| `package.json` | **修改** | 更新 dev/build 脚本 |

### 移除的代码

- `src-tauri/tauri.conf.json` — 不再需要
- `src-tauri/tauri.windows.conf.json` — 不再需要
- `src-tauri/tray.rs` — 系统托盘
- `src-tauri/auto_launch.rs` — 自启动（当前系统不需要）
- `src-tauri/lightweight.rs` — 轻量模式
- `src-tauri/linux_fix.rs` — WebKitGTK 修复（不再使用 WebKitGTK）
- `src-tauri/panic_hook.rs` — 可保留简化版
- `src-tauri/icons/` — 不再需要
- `src-tauri/wix/` — Windows 安装包
- `src-tauri/Info.plist` — macOS 配置
- `src-tauri/common-controls.manifest` — Windows 清单
- `src-tauri/capabilities/` — Tauri 安全策略

## Rust 后端改造

### 1. lib.rs — 抽离公共初始化

将原有的 `pub fn run()` 拆分为：

```rust
// 公共初始化（与 Tauri 无关）
pub async fn initialize_services() -> Result<(Arc<AppState>, EventBus, ServiceRegistry), AppError> {
    // 1. 数据库初始化 + migration
    // 2. ProxyService 初始化
    // 3. ProviderService, McpService, SkillService 等初始化
    // 4. AppState 创建
    // 5. EventBus 创建 (broadcast::channel)
    // 6. 数据导入（预设 providers, MCP, skills, prompts 等）
}

// Tauri 模式入口（保留，用于上游构建）
pub fn run() { ... }

// Server 模式入口
pub async fn run_server() {
    let (state, event_bus, _) = initialize_services().await.unwrap();
    // ... 启动 axum server
}
```

### 2. 命令函数 — State → Arc 替换

所有 `#[tauri::command]` 函数做机械替换：

```
State<'_, AppState>  →  Arc<AppState>
tauri::AppHandle     →  按用途替换（EventBus / process::exit）
tauri::State<'_, T>  →  Arc<RwLock<T>>
```

示例：

```rust
// 改前
#[tauri::command]
pub async fn get_providers(state: State<'_, AppState>) -> Result<Vec<Provider>, AppError> {
    let db = state.db.clone();
    provider_service.list_providers(&db).await
}

// 改后
pub async fn get_providers(state: Arc<AppState>) -> Result<Vec<Provider>, AppError> {
    let db = state.db.clone();
    provider_service.list_providers(&db).await
}
```

移除 `#[tauri::command]` 属性宏，保留函数体和返回类型。

### 3. handlers.rs — HTTP 命令映射

集中管理所有 ~150 个命令的 HTTP 映射：

```rust
pub async fn get_providers(
    Extension(state): Extension<Arc<AppState>>,
) -> Result<Json<Value>, AppErrorJson> {
    commands::provider::get_providers(state).await.map(Json)
}

pub async fn get_settings(
    Extension(state): Extension<Arc<AppState>>,
) -> Result<Json<Value>, AppErrorJson> {
    commands::settings::get_settings(state).await.map(Json)
}

pub async fn save_settings(
    Extension(state): Extension<Arc<AppState>>,
    Json(body): Json<HashMap<String, Value>>,
) -> Result<Json<Value>, AppErrorJson> {
    commands::settings::save_settings(state, body).await.map(Json)
}
// ... 每个命令 ~5 行
```

### 4. axum 服务启动

```rust
pub async fn run_server() {
    let (state, event_bus, _) = initialize_services().await.expect("init failed");
    let port = env::var("CC_SWITCH_PORT").unwrap_or_else(|_| "10245".to_string());

    let app = Router::new()
        // API 路由（具体命令列表由 handlers.rs 导出）
        .route("/api/{*command}", post(handlers::dispatch))  // 方案A: 统一路由
        // .route("/api/get_providers", post(handlers::get_providers))  // 方案B: 逐条注册
        .route("/events", get(sse_handler))
        .nest_service("/", get_service(ServeDir::new("../dist")))
        .layer(Extension(state))
        .layer(Extension(event_bus))
        .layer(CorsLayer::permissive());

    let addr = format!("127.0.0.1:{}", port);
    info!("CC Switch server starting at http://{}", addr);
    let listener = TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
```

**路由方案选择**：采用统一路由 `/api/{command}` + dispatch 模式，减少逐条注册的重复代码。

### 5. 事件系统 — SSE

```rust
pub async fn sse_handler(
    Extension(event_bus): Extension<EventBus>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = event_bus.subscribe();
    let stream = async_stream::stream! {
        let mut rx = rx;
        while let Ok((event_name, payload)) = rx.recv().await {
            yield Ok(Event::default().event(event_name).data(payload));
        }
    };
    Sse::new(stream).keep_alive(KeepAlive::default())
}

// 事件发射替代 app.emit()
event_bus.send(("provider-switched".into(), json_payload)).ok();
```

### 6. Cargo.toml 依赖变更

```diff
[build-dependencies]
- tauri-build = { version = "2.4.0", features = [] }

[dependencies]
- tauri = { version = "2.8.2", features = [...] }
- tauri-plugin-log = "2"
- tauri-plugin-opener = "2"
- tauri-plugin-process = "2"
- tauri-plugin-dialog = "2"
- tauri-plugin-store = "2"
- tauri-plugin-deep-link = "2"
- tauri-plugin-window-state = "2"
- tauri-plugin-single-instance = "2"
- auto-launch = "0.5"

- [target.'cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))'.dependencies]
- tauri-plugin-single-instance = "2"

- [target.'cfg(target_os = "linux")'.dependencies]
- webkit2gtk = { version = "2.0.1", features = ["v2_16"] }

- [target.'cfg(target_os = "windows")'.dependencies]
- winreg = "0.52"

- [target.'cfg(target_os = "macos")'.dependencies]
- objc2 = "0.5"
- objc2-app-kit = { version = "0.2", features = ["NSColor"] }

# 保留已有的 axum / tower / tokio
# 在 tokio features 中添加 "net"
```

## 前端改造

### 1. tauri-shim 文件

在 `src/tauri-shim/` 下创建 8 个文件，实现与 `@tauri-apps/*` 相同的导出接口：

| Shim 文件 | 对应包 | 导出 |
|---|---|---|
| `core.ts` | `@tauri-apps/api/core` | `invoke` |
| `event.ts` | `@tauri-apps/api/event` | `listen`, `UnlistenFn` |
| `app.ts` | `@tauri-apps/api/app` | `getVersion` |
| `path.ts` | `@tauri-apps/api/path` | `homeDir`, `join` |
| `window.ts` | `@tauri-apps/api/window` | `getCurrentWindow` → 无操作 mock |
| `plugin-dialog.ts` | `@tauri-apps/plugin-dialog` | `message` → `alert` |
| `plugin-process.ts` | `@tauri-apps/plugin-process` | `exit`, `relaunch` |

核心实现模式：

```typescript
// core.ts
const BASE = `http://localhost:${import.meta.env.VITE_CC_SWITCH_PORT || 10245}`;

export async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const resp = await fetch(`${BASE}/api/${cmd}`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: args ? JSON.stringify(args) : '{}',
  });
  if (!resp.ok) {
    const body = await resp.json().catch(() => ({ error: resp.statusText }));
    throw body;
  }
  return resp.json();
}

// event.ts
export async function listen<T>(
  event: string,
  handler: (ev: { payload: T }) => void,
): Promise<() => void> {
  const es = new EventSource(`${BASE}/events`);
  es.addEventListener(event, (e) => handler({ payload: JSON.parse(e.data) }));
  return () => es.close();
}
export type UnlistenFn = () => void;
```

### 2. vite.config.ts

```typescript
resolve: {
  alias: {
    '@tauri-apps/api/core': path.resolve(__dirname, './src/tauri-shim/core.ts'),
    '@tauri-apps/api/event': path.resolve(__dirname, './src/tauri-shim/event.ts'),
    '@tauri-apps/api/app': path.resolve(__dirname, './src/tauri-shim/app.ts'),
    '@tauri-apps/api/path': path.resolve(__dirname, './src/tauri-shim/path.ts'),
    '@tauri-apps/api/window': path.resolve(__dirname, './src/tauri-shim/window.ts'),
    '@tauri-apps/plugin-dialog': path.resolve(__dirname, './src/tauri-shim/plugin-dialog.ts'),
    '@tauri-apps/plugin-process': path.resolve(__dirname, './src/tauri-shim/plugin-process.ts'),
  },
},
```

### 3. package.json

```json
{
  "scripts": {
    "dev": "vite",
    "dev:server": "cargo run --bin cc-switch-server",
    "build": "vite build && cargo build --bin cc-switch-server",
    "build:renderer": "vite build",
    "dev:full": "concurrently \"vite\" \"cargo run --bin cc-switch-server\""
  }
}
```

## 构建产物

最终输出：**单二进制文件**

```
target/release/cc-switch-server
```

运行方式：

```bash
# 默认端口 10245
./cc-switch-server

# 指定端口
CC_SWITCH_PORT=9999 ./cc-switch-server

# 然后在浏览器打开 http://localhost:10245
```

## 移除特性清单

| 特性 | 原因 | 用户影响 |
|---|---|---|
| 系统托盘 | WebKitGTK/Tauri API，浏览器无此概念 | 无，Web UI 中已有全部功能 |
| 原生窗口管理 | 浏览器管理窗口 | 无 |
| 原生文件对话框 | 替换为浏览器 `<input type="file">` | 视觉差异，功能一致 |
| 自启动 | 系统策略不允许 | 可后续通过 systemd 实现 |
| 自动更新 | 已确认移除 | 需手动下载新版 |
| Deep Link (`ccswitch://`) | 浏览器不支持注册 URL scheme | 可后续通过 Web API 恢复 |
| 单一实例 | 替换为 PID 文件 | 无影响 |
| Lightweight 模式 | 浏览器无此概念 | 无 |
| macOS Dock / Windows 任务栏集成 | 平台特定，不部署在 macOS/Windows | 无 |

## 对上游合并的影响

| 更改类型 | 冲突概率 | 合并策略 |
|---|---|---|
| `commands/*.rs` 的 `State` → `Arc` 替换 | 低（机械变更，易合并） | 逐文件 diff，只改参数类型不改逻辑 |
| `lib.rs` 初始化逻辑 | 中（初始化和入口函数） | 保持 `run()` 函数签名不变，新增 `run_server()` |
| 新增 `main_server.rs`, `handlers.rs` | 极低（上游没有这些文件） | 无冲突 |
| 新增 `src/tauri-shim/` | 极低（上游没有这些文件） | 无冲突 |
| 修改 `vite.config.ts` | 低（仅添加 alias） | 仅在前/后追加行，不修改中间逻辑 |
| 移除 `tray.rs` 等文件 | 中（上游可能修改这些文件） | 上游合并时需确认这些文件的改动是否需要适配 |

## 实施步骤

1. 修改 `Cargo.toml` — 移除 Tauri 依赖，调整 features
2. 修改 `build.rs` — 移除 `tauri_build::build()`
3. 修改 `lib.rs` — 抽离公共初始化，保留 `run()`，新增 `run_server()`
4. 修改 `commands/*.rs` — `State<'_, AppState>` → `Arc<AppState>` 机械替换
5. 修改 `error.rs` — 移除 Tauri 相关 Error 变体（如有）
6. 新增 `main_server.rs` — HTTP 服务入口
7. 新增 `handlers.rs` — 命令 → HTTP handler 映射
8. 删除废弃文件 — `tray.rs`, `auto_launch.rs`, `lightweight.rs`, `linux_fix.rs`, `panic_hook.rs`（简化为日志）
9. 创建 `src/tauri-shim/` — 8 个前端适配文件
10. 修改 `vite.config.ts` — 添加 resolve alias
11. 修改 `package.json` — 更新脚本
12. 删除 Tauri 配置和资源 — `tauri.conf.json`, `icons/`, `wix/` 等
13. 验证构建和运行
