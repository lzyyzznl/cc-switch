# Tauri 2 → Standalone HTTP Server 实施计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将 cc-switch 从 Tauri 2 桌面应用改造为 Rust 独立 HTTP 服务 + 浏览器前端，摆脱 WebKitGTK 4.1 和 GLIBC 2.34+ 的依赖。

**Architecture:** Rust 后端保留全部业务逻辑，将入口从 `tauri::Builder` 改为 axum HTTP Server（localhost:10245）。前端通过 Vite resolve alias 将 `@tauri-apps/*` 导入重定向到 `src/tauri-shim/` 适配层，通过 `fetch()` 调用后端 API、SSE 接收事件推送。

**Tech Stack:** Rust (axum, tokio, tower-http), TypeScript (Vite), React

**参考设计文档:** `docs/superpowers/specs/2026-05-20-cc-switch-tauri2-standalone-server-design.md`

---

### Task 1: 修改 Cargo.toml — 剥离 Tauri 依赖，调整 features

**Files:**
- Modify: `src-tauri/Cargo.toml`

- [ ] **Step 1: 读取当前 Cargo.toml 确认完整内容**

Run: `cat src-tauri/Cargo.toml`

- [ ] **Step 2: 移除 Tauri 及相关依赖**

移除以下内容：
- `[build-dependencies]` 中的 `tauri-build`
- `[dependencies]` 中的 `tauri`, 所有 `tauri-plugin-*`, `webkit2gtk`, `winreg`, `objc2`, `objc2-app-kit`, `auto-launch`
- `[target.'cfg(...)'.dependencies]` 下的所有平台特定依赖

保留已有：`axum`, `tower`, `tower-http`, `tokio`, `serde_json`, `serde`, `log` 等

修改内容（在 Cargo.toml 中）：

```diff
-[build-dependencies]
-tauri-build = { version = "2.4.0", features = [] }
+# 不再使用 tauri-build，build.rs 将简化为空

 [dependencies]
-serde_json = { version = "1.0", features = ["preserve_order"] }
-serde = { version = "1.0", features = ["derive"] }
-log = "0.4"
-chrono = { version = "0.4", features = ["serde"] }
-tauri = { version = "2.8.2", features = ["tray-icon", "protocol-asset", "image-png"] }
-tauri-plugin-log = "2"
-tauri-plugin-opener = "2"
-tauri-plugin-process = "2"
-tauri-plugin-updater = "2"
-tauri-plugin-dialog = "2"
-tauri-plugin-store = "2"
-tauri-plugin-deep-link = "2"
-tauri-plugin-window-state = "2"
-tauri-plugin-single-instance = "2"
+# 保留 serde_json, serde, log, chrono 等业务依赖
+# 移除以上所有 tauri-plugin-* 和 tauri
```

`tokio` features 中确保包含 `"net"`（用于 TcpListener）：

```diff
-tokio = { version = "1", features = ["macros", "rt-multi-thread", "time", "sync"] }
+tokio = { version = "1", features = ["macros", "rt-multi-thread", "time", "sync", "net"] }
```

删除整个 `[target.'cfg(...)'.dependencies]` 节。

- [ ] **Step 3: 验证 Cargo 解析**

Run: `cd src-tauri && cargo check 2>&1 | tail -20`
Expected: 大量编译错误（因为我们还没改代码），但 Cargo.toml 本身解析应无问题。

---

### Task 2: 简化 build.rs

**Files:**
- Modify: `src-tauri/build.rs`

- [ ] **Step 1: 将 build.rs 替换为空实现**

```rust
fn main() {
    // 不再需要 tauri_build::build()，此文件保留占位
}
```

- [ ] **Step 2: 验证**

Run: `cd src-tauri && cargo check 2>&1 | head -5`
Expected: 继续编译错误，但 build.rs 无报错。

---

### Task 3: 抽离 lib.rs 公共初始化逻辑

**Files:**
- Modify: `src-tauri/src/lib.rs`
- Create: `src-tauri/src/init.rs`

- [ ] **Step 1: 读取 lib.rs 全部内容**

Run: `wc -l src-tauri/src/lib.rs`
Expected: ~1800 行。确认当前行数后全量读取。

- [ ] **Step 2: 识别 setup 闭包中的公共初始化代码**

`lib.rs` 中 `setup(|app| { ... })` 闭包（约第 283 行起）包含了以下公共初始化逻辑（与 Tauri 无关）：

1. 创建日志目录并初始化文件日志
2. 初始化数据库（`Database::new()` + migration）
3. 从旧 config.json 迁移
4. 创建 `AppState`
5. 导入默认数据（preset providers, MCP, skills, prompts 等）
6. 注册 deep-link handler
7. 创建系统托盘
8. 启动 WebDAV 自动同步
9. 管理状态注入 (`app.manage()`)
10. 初始化全局代理
11. 启动后台任务（crash recovery, proxy restore, backup, session sync）
12. 显示窗口

需要抽离的部分（保留给 Tauri 模式的独有部分）：tray 创建、窗口管理、deep-link、tauri 插件。

- [ ] **Step 3: 创建 `init.rs`**

创建 `src-tauri/src/init.rs`，将公共初始化提取为 `async fn initialize_services() -> Result<(Arc<AppState>, EventBus), AppError>`：

```rust
use std::sync::Arc;
use tokio::sync::broadcast;
use crate::database::Database;
use crate::store::AppState;
use crate::services::*;

/// 事件总线：替代 tauri::Emitter
pub type EventBus = broadcast::Sender<(String, String)>;

pub async fn initialize_services() -> Result<(Arc<AppState>, EventBus), crate::error::AppError> {
    // 1. 确定 app config 目录
    let app_config_dir = crate::config::get_app_config_dir();
    let _ = std::fs::create_dir_all(&app_config_dir);

    // 2. 初始化日志
    let log_dir = app_config_dir.join("logs");
    let _ = std::fs::create_dir_all(&log_dir);
    // 简单文件日志，不使用 tauri_plugin_log
    setup_file_logger(&log_dir);

    // 3. 初始化数据库
    let db = Database::new(app_config_dir.join("cc-switch.db"))?;  // 假设 Database::new 接受 path

    // 4. 创建 AppState
    let app_state = Arc::new(AppState::new(Arc::new(db)));

    // 5. 创建事件总线
    let (event_tx, _) = broadcast::channel(100);

    // 6. 导入预设数据（从原 setup 中提取）
    import_preset_data(&app_state).await?;

    // 7. 启动后台任务
    spawn_background_tasks(&app_state, event_tx.clone());

    Ok((app_state, event_tx))
}

fn setup_file_logger(log_dir: &std::path::Path) {
    // 简单实现：log4rs 或 env_logger + file output
    // 实际上项目中用 log crate，这里用 fern 或直接 env_logger
    // 当前先用 env_logger + 手动文件输出
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .target(env_logger::Target::Stdout)
        .init();
}

async fn import_preset_data(state: &Arc<AppState>) -> Result<(), crate::error::AppError> {
    // 从原 lib.rs setup() 中提取的数据初始化逻辑
    // 包括：default skill repos, official providers, MCP configs, prompts, OMO configs 等
    // 这些逻辑完全复制原有代码，不做业务修改
    Ok(())
}

fn spawn_background_tasks(state: &Arc<AppState>, event_tx: EventBus) {
    // 从原 lib.rs setup() 中提取的后台任务
    // 包括：crash recovery, proxy state, periodic backup, session log sync
    // 注意：这些任务原通过 tauri::async_runtime::spawn 启动，改为 tokio::spawn
}
```

**注意：** 以上只是框架。实际实现时，需要从 lib.rs setup() 闭包中逐段搬移代码，保持业务逻辑完全不变。

- [ ] **Step 4: 在 lib.rs 的 run() 中，保留 Tauri 模式入口**

在 lib.rs 中添加 `pub async fn run_server()` 接口声明，但实现放在 `main_server.rs` 中：

```rust
// lib.rs 末尾添加
pub mod init;

/// 启动独立 HTTP 服务器模式
pub async fn run_server() {
    // 委托给 main_server 模块
    crate::main_server::start_server().await;
}
```

并在 lib.rs 顶部添加 `mod init;` 和 `mod main_server;`。

- [ ] **Step 5: 验证无语法错误**

Run: `cd src-tauri && cargo check 2>&1 | head -20`
Expected: 错误数减少，但仍可能有未解决的引用错误。

---

### Task 4: 修改 commands/*.rs — State → Arc 机械替换

**Files:**
- Modify: 所有 `src-tauri/src/commands/*.rs` 中使用 `State<'_, AppState>` 或 `AppHandle` 的文件

- [ ] **Step 1: 定位需要替换的文件**

Run: `rg -l 'State.*AppState' src-tauri/src/commands/*.rs src-tauri/src/*.rs`
Expected: 列出约 20+ 个文件

Run: `rg -l 'AppHandle' src-tauri/src/commands/*.rs src-tauri/src/*.rs`
Expected: 列出约 11 个文件

- [ ] **Step 2: 替换 State<'_, AppState> → Arc<AppState>**

对所有 commands 文件执行 sed：

```bash
# 注意：rg 已禁用，这里用 sed 做全局替换
cd src-tauri/src
find commands/ -name '*.rs' -exec sed -i 's/State<'\''_, AppState>/Arc<AppState>/g' {} +
# 也修复 lib.rs 和 tray.rs 等非 commands 文件
sed -i 's/State<'\''_, AppState>/Arc<AppState>/g' lib.rs 2>/dev/null || true
```

替换后还需在每个使用 `Arc<AppState>` 的文件中确认导入：

```rust
// 确保每个文件顶部有 use std::sync::Arc;
```

- [ ] **Step 3: 处理 AppHandle 的使用**

对每个文件中的 `AppHandle` 按用途处理：

| 模式 | 替换为 |
|---|---|
| `app: tauri::AppHandle` | 移除参数，改为注入 EventBus |
| `app_handle: tauri::AppHandle` | 同上 |
| `app.emit(...)` | `event_tx.send(...)` |
| `app.exit(0)` | `std::process::exit(0)` |
| `app.state::<T>()` | 改为直接 Arc 参数传递 |
| `app.get_webview_window(...)` | 组件直接调用（无窗口） |

**轻量级命令文件（lightweight.rs）直接标记为"Server 模式下不可用"**：

```rust
// lightweight.rs — 替换为
pub fn enter_lightweight_mode() -> Result<(), String> {
    Err("Server mode: lightweight mode is not available".to_string())
}
pub fn exit_lightweight_mode() -> Result<(), String> {
    Err("Server mode: lightweight mode is not available".to_string())
}
pub fn is_lightweight_mode() -> bool {
    false
}
```

- [ ] **Step 4: 移除所有 `#[tauri::command]` 属性宏**

```bash
cd src-tauri/src/commands
find . -name '*.rs' -exec sed -i '/#\[tauri::command\]/d' {} +
```

保留函数签名中的 `pub`/`pub async fn` 不变。

- [ ] **Step 5: 修复 commands/mod.rs**

更新 `commands/mod.rs` — 移除导出的 `pub use lightweight::*;`（或保留但无害）。

- [ ] **Step 6: 尝试编译验证**

Run: `cd src-tauri && cargo check 2>&1 | grep -E 'error\[|warning\[' | head -30`
Expected: 列出剩余编译错误，主要是未处理的 AppHandle 引用和导入缺失。

---

### Task 5: 修复 error.rs — 移除 Tauri 依赖

**Files:**
- Modify: `src-tauri/src/error.rs`

- [ ] **Step 1: 确认 error.rs 无 Tauri 依赖**

检查 error.rs 中是否有 `impl From<tauri::Error>` 或 `impl From<tauri_plugin_*::Error>`。

Run: `rg 'tauri' src-tauri/src/error.rs`
Expected: 无输出（error.rs 已检查过，没有 Tauri 引用）。

- [ ] **Step 2: 添加 serde::Serialize 适配 HTTP 响应**

确保 `AppError` 可以序列化为 JSON：

```rust
// 已存在 Serialize impl，确认其格式适合 HTTP 响应
impl serde::Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

// 新增：用于 axum Response 的转换
impl axum::response::IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        let body = serde_json::json!({ "error": self.to_string() });
        (axum::http::StatusCode::INTERNAL_SERVER_ERROR, axum::Json(body)).into_response()
    }
}
```

- [ ] **Step 3: 验证**

Run: `cd src-tauri && cargo check 2>&1 | grep -E 'error\[E' | head -10`
Expected: 错误数比上一步减少。

---

### Task 6: 创建 main_server.rs — HTTP 服务入口

**Files:**
- Create: `src-tauri/src/main_server.rs`

- [ ] **Step 1: 创建二进制入口文件**

```rust
// main_server.rs — HTTP 服务模式入口
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[tokio::main]
async fn main() {
    cc_switch_lib::run_server().await;
}
```

- [ ] **Step 2: 在 lib.rs 中添加 run_server() 实现**

```rust
// lib.rs 中
mod main_server;

pub async fn run_server() {
    println!("CC Switch Server starting...");
    main_server::start_server().await;
}
```

- [ ] **Step 3: 在 main_server.rs 中实现 start_server**

```rust
use std::sync::Arc;
use axum::{
    Router,
    routing::{get, post},
    response::Sse,
    extract::Path,
    Extension,
    body::Body,
};
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;
use futures::stream::Stream;
use std::convert::Infallible;

pub async fn start_server() {
    let (state, event_bus) = crate::init::initialize_services().await
        .expect("Failed to initialize services");

    let port = std::env::var("CC_SWITCH_PORT")
        .unwrap_or_else(|_| "10245".to_string());

    let app = Router::new()
        // API 路由 — 统一 dispatch
        .route("/api/{*command}", post(api_dispatch))
        // SSE 事件推送
        .route("/events", get(sse_handler))
        // 静态前端文件
        .nest_service("/", tower_http::services::ServeDir::new(
            std::env::var("CC_SWITCH_DIST_DIR")
                .unwrap_or_else(|_| "../dist".to_string())
        ))
        .layer(Extension(state))
        .layer(Extension(event_bus))
        .layer(tower_http::cors::CorsLayer::permissive());

    let addr = format!("127.0.0.1:{}", port);
    log::info!("CC Switch server listening on http://{}", addr.Request;

    let listener = tokio::net::TcpListener::bind(&addr).await
        .expect("Failed to bind address");

    axum::serve(listener, app).await
        .expect("Server exited with error");
}

async fn api_dispatch(
    Extension(state): Extension<Arc<crate::store::AppState>>,
    Path(command): Path<String>,
    axum::extract::Request body,
) -> axum::response::Response {
    // 委托给 handlers::dispatch
    crate::handlers::dispatch(&command, state, body).await
}

async fn sse_handler(
    Extension(event_bus): Extension<broadcast::Sender<(String, String)>>,
) -> Sse<impl Stream<Item = Result<axum::response::sse::Event, Infallible>>> {
    use tokio_stream::wrappers::BroadcastStream;

    let rx = event_bus.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(|result| {
        match result {
            Ok((event_name, payload)) => Some(Ok(
                axum::response::sse::Event::default()
                    .event(event_name)
                    .data(payload)
            )),
            Err(_) => None, // lagged — 跳过
        }
    });

    Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(std::time::Duration::from_secs(15))
    )
}
```

- [ ] **Step 4: 在 Cargo.toml 中添加 [[bin]] 定义**

```toml
[[bin]]
name = "cc-switch-server"
path = "src/main_server.rs"
```

- [ ] **Step 5: 验证编译**

Run: `cd src-tauri && cargo check --bin cc-switch-server 2>&1 | grep -E 'error' | head -20`
Expected: 错误逐步收敛。

---

### Task 7: 创建 handlers.rs — 命令调度

**Files:**
- Create: `src-tauri/src/handlers.rs`

- [ ] **Step 1: 创建命令调度器**

```rust
use std::sync::Arc;
use axum::{
    extract::Path,
    Extension,
    Json,
};
use serde_json::Value;

use crate::store::AppState62;

/// 统一命令调度 — 根据命令名称分发到对应的处理函数
pub async fn dispatch(
    command: &str,
    state: Arc<AppState>,
    body: axum::extract::Request,
) -> axum::response::Response {
    // 解析 body 为 JSON
    let body_bytes = axum::body::to_bytes(body.into_body(), 1024 * 1024).await
        .unwrap_or_default();
    let args: Option<Value> = serde_json::from_slice(&body_bytes).ok();

    let result = match command {
        // 配置
        "get_settings" => handle_json(crate::commands::get_settings(state)).await,
        "save_settings" => {
            let settings = args.and_then(|v| v.as_object().cloned()).unwrap_or_default();
            handle_json(crate::commands::save_settings(state, serde_json::Value::Object(settings))).await
        }
        "get_app_config_dir_override" => handle_json(crate::commands::get_app_config_dir_override(state)).await,
        "set_app_config_dir_override" => handle_json(crate::commands::set_app_config_dir_override(state, args)).await,
        "get_tool_versions" => handle_json(crate::commands::get_tool_versions()).await,
        "get_init_error" => handle_json(crate::commands::get_init_error()).await,

        // Provider
        "get_providers" => handle_json(crate::commands::get_providers(state)).await,
        "get_current_provider" => handle_json(crate::commands::get_current_provider(state, args)).await,
        "add_provider" => {
            let data = args.unwrap_or_default();
            handle_json(crate::commands::add_provider(state, data)).await
        }
        "update_provider" => handle_json(crate::commands::update_provider(state, args)).await,
        "delete_provider" => handle_json(crate::commands::delete_provider(state, args)).await,
        "switch_provider" => handle_json(crate::commands::switch_provider(state, args)).await,

        // Proxy
        "start_proxy_server" => handle_json(crate::commands::start_proxy_server(state)).await,
        "stop_proxy_with_restore" => handle_json(crate::commands::stop_proxy_with_restore(state)).await,
        "get_proxy_status" => handle_json(crate::commands::get_proxy_status(state)).await,
        "is_proxy_running" => handle_json(crate::commands::is_proxy_running(state)).await,

        // 进程管理（替换 AppHandle.exit）
        "exit_app" => {
            std::process::exit(0);
        }

        // 其余命令... 完整列表见设计文档中的命令映射表
        _ => (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": format!("Unknown command: {}", command)})),
        ).into_response(),
    };

    result
}

/// 统一的异步结果 → HTTP 响应转换
async fn handle_json<T: serde::Serialize>(
    result: Result<T, crate::error::AppError>
) -> axum::response::Response {
    match result {
        Ok(value) => (axum::http::StatusCode::OK, Json(serde_json::to_value(value).unwrap_or_default())).into_response(),
        Err(err) => err.into_response(),
    }
}
```

**完整命令列表**（约 150 个）需要从 `lib.rs` 的 `generate_handler![]` 宏中提取。

- [ ] **Step 2: 从 lib.rs 提取所有注册的命令**

Run: `rg 'generate_handler!\[' -A 300 src-tauri/src/lib.rs | head -200`
提取 `generate_handler![]` 宏中的完整命令列表，转换到 `handlers.rs` 的 match 分支。

命令较多，按模块分组添加。每个命令一行：

```rust
// Provider
"get_providers" => ...
"get_current_provider" => ...
// ... 继续添加所有命令
```

- [ ] **Step 3: 验证编译**

Run: `cd src-tauri && cargo check --bin cc-switch-server 2>&1 | grep 'error' | head -20`
Expected: 命令未完整实现会有__，但结构应无编译问题。

---

### Task 8: 删除废弃的 Rust 文件

**Files:**
- Delete: 多个 Tauri 独有文件

- [ ] **Step 1: 删除废弃代码**

```bash
cd src-tauri/src
rm -f tray.rs
rm -f lightweight.rs
rm -f auto_launch.rs
rm -f linux_fix.rs
rm -f panic_hook.rs    # 改为简单版本
```

- [ ] **Step 2: 删除 Tauri 配置和资源**

```bash
cd src-tauri
rm -f tauri.conf.json
rm -f tauri.windows.conf.json
rm -f Info.plist
rm -f common-controls.manifest
rm -rf wix/
rm -rf icons/
rm -rf capabilities/
```

- [ ] **Step 3: 在 lib.rs 中移除已删除模块的 mod 声明**

在 `lib.rs` 中删除以下行：
```rust
// mod tray;
// mod lightweight;
// mod auto_launch;
// #[cfg(target_os = "linux")]
// mod linux_fix;
```

保留 `mod panic_hook;` 但替换为简化版（只保留 crash 日志写入，移除 Tauri 相关内容）。

- [ ] **Step 4: 创建简化版 panic_hook.rs**

```rust
use std::fs;
use std::path::PathBuf;
use std::sync::OnceLock;

static APP_CONFIG_DIR: OnceLock<PathBuf> = OnceLock::new();

pub fn init_app_config_dir(dir: PathBuf) {
    let _ = APP_CONFIG_DIR.set(dir);
}

pub fn get_log_dir() -> PathBuf {
    APP_CONFIG_DIR.get()
        .cloned()
        .unwrap_or_else(|| {
            dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")).join(".cc-switch")
        })
        .join("logs")
}

pub fn setup_panic_hook() {
    let log_dir = get_log_dir();
    let _ = fs::create_dir_all(&log_dir);
    let crash_log = log_dir.join("crash.log");

    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        let msg = format!(
            "[{}] PANIC: {}\n  location: {:?}\n  backtrace: {:?}\n",
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
            panic_info.to_string().trim(),
            panic_info.location(),
            std::backtrace::Backtrace::capture(),
        );
        let _ = fs::write(&crash_log, &msg);
        eprintln!("{}", msg);
        default_hook(panic_info);
    }));
}
```

---

### Task 9: 配置前端 tauri-shim 适配层

**Files:**
- Create: `src/tauri-shim/core.ts`
- Create: `src/tauri-shim/event.ts`
- Create: `src/tauri-shim/app.ts`
- Create: `src/tauri-shim/path.ts`
- Create: `src/tauri-shim/window.ts`
- Create: `src/tauri-shim/plugin-dialog.ts`
- Create: `src/tauri-shim/plugin-process.ts`
- Create: `src/tauri-shim/index.ts`
- Modify: `vite.config.ts`
- Modify: `package.json`

- [ ] **Step 1: 创建目录和 core.ts**

```bash
mkdir -p src/tauri-shim
```

```typescript
// src/tauri-shim/core.ts
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

export async function transformCallback<T>(val: T): Promise<T> {
  return val;
}
```

- [ ] **Step 2: 创建 event.ts**

```typescript
// src/tauri-shim/event.ts
const BASE = `http://localhost:${import.meta.env.VITE_CC_SWITCH_PORT || 10245}`;

const activeSources = new Map<string, EventSource>();

export async function listen<T>(
  event: string,
  handler: (event: { payload: T }) => void,
): Promise<() => void> {
  let es = activeSources.get('default');
  if (!es) {
    es = new EventSource(`${BASE}/events`);
    activeSources.set('default', es);
  }

  const wrappedHandler = (e: MessageEvent) => {
    try {
      handler({ payload: JSON.parse(e.data) });
    } catch {
      handler({ payload: e.data as unknown as T });
    }
  };

  es.addEventListener(event, wrappedHandlerapse);
  return () => {
    es?.removeEventListener(event, wrappedHandler);
  };
}

export type UnlistenFn = () => void;
```

- [ ] **Step 3: 创建 app.ts**

```typescript
// src/tauri-shim/app.ts
import { invoke } from './core';

export async function getVersion(): Promise<string> {
  try {
    // 在后端添加一个 get_app_version 命令返回 package version
    return await invoke<string>('get_app_version');
  } catch {
    return '3.15.0'; // 默认 fallback
  }
}

export async function getName(): Promise<string> {
  return 'CC Switch';
}
```

- [ ] **Step 4: 创建 path.ts**

```typescript
// src/tauri-shim/path.ts
export async function homeDir(): Promise<string> {
  return '/home'; // 服务端模式下静态返回
}

export async function join(...paths: string[]): Promise<string> {
  return paths.join('/');
}
```

- [ ] **Step 5: 创建 window.ts**

```typescript
// src/tauri-shim/window.ts
// 浏览器模式下，窗口 API 无操作
export function getCurrentWindow() {
  return {
    setTitle: async () => {},
    setSize: async () => {},
    show: async () => {},
    hide: async () => {},
    setFocus: async () => {},
    setSkipTaskbar: async () => {},
    setDecorations: async () => {},
    innerSize: async () => ({ width: 0, height: 0 }),
    minimize: async () => {},
    unminimize: async () => {},
    close: async () => {},
    destroy: async () => {},
    setFullscreen: async () => {},
    isFullscreen: async () => false,
    isMaximized: async () => false,
    maximize: async () => {},
    unmaximize: async () => {},
  };
}
```

- [ ] **Step 6: 创建 plugin-dialog.ts**

```typescript
// src/tauri-shim/plugin-dialog.ts
export async function message(message: string): Promise<void> {
  alert(message);
}

export async function ask(message: string): Promise<boolean> {
  return window.confirm(message);
}

export async function open(options?: Record<string, unknown>): Promise<string | null> {
  return new Promise((resolve) => {
    const input = document.createElement('input');
    input.type = 'file';
    if (options?.directory) {
      input.setAttribute('webkitdirectory', '');
    }
    if (options?.multiple) {
      input.multiple = true;
    }
    if (options?.filters) {
      input.accept = (options.filters as Array<{ extensions: string[] }>)
        .flatMap((f) => f.extensions.map((e) => `.${e}`))
        .join(',');
    }
    input.onchange = () => {
      resolve(input.files?.[0]?.name || null);
    };
    input.click();
  });
}
```

- [ ] **Step 7: 创建 plugin-process.ts**

```typescript
// src/tauri-shim/plugin-process.ts
import { invoke } from './core';

export async function exit(code?: number): Promise<void> {
  await invoke('exit_app', { code: code ?? 0 });
}

export async function relaunch(): Promise<void> {
  // 在 server 模式下，relaunch 需要外部逻辑
  // 简单重定向用户手动操作
  console.warn('relaunch not available in server mode');
}
```

- [ ] **Step 8: 创建 index.ts**

```typescript
// src/tauri-shim/index.ts
// 检测当前是否运行在 Tauri 环境（兼容原 Tauri 运行时）
export function isTauri(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}
```

- [ ] **Step 9: 修改 vite.config.ts**

```typescript
// 在 resolve.alias 中添加
resolve: {
  alias: {
    '@': path.resolve(__dirname, './src'),
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

- [ ] **Step 10: 修改 package.json 脚本**

```json
{
  "scripts": {
    "dev": "pnpm dev:renderer",
    "dev:full": "concurrently \"pnpm dev:renderer\" \"cargo run --bin cc-switch-server\"",
    "build": "pnpm build:renderer && cargo build --bin cc-switch-server",
    "build:renderer": "vite build",
    "dev:renderer": "vite",
    "typecheck": "tsc --noEmit",
    "format": "prettier --write \"src/**/*.{js,jsx,ts,tsx,css,json}\"",
    "format:check": "prettier --check \"src/**/*.{js,jsx,ts,tsx,css,json}\"",
    "test:unit": "vitest run",
    "test:unit:watch": "vitest watch"
  }
}
```

---

### Task 10: 编译验证和修复

**Files:**
- Modify: 各种修复

- [ ] **Step 1: 完整编译**

Run: `cd src-tauri && cargo check --bin cc-switch-server 2>&1`

- [ ] **Step 2: 逐条修复剩余错误**

典型的错误类型：
1. `use std::sync::Arc` 缺失 — 在对应文件添加 import
2. `AppHandle` 未处理 — 改为 EventBus 或移除
3. `app.state::<T>()` 无法使用 — 改为参数传递
4. `tauri::Result` / `tauri::Error` 引用 — 改为 `AppError`
5. `tokio::sync::broadcast` 相关错误 — 确认导入

逐条修复直到编译通过，每次修改后运行 `cargo check`。

- [ ] **Step 3: 前端 TypeScript 检查**

Run: `pnpm typecheck 2>&1`
Expected: 零错误

- [ ] **Step 4: 前端构建验证**

Run: `pnpm build:renderer 2>&1`
Expected: 构建成功，`dist/` 输出

---

### Task 11: 全链路验证

- [ ] **Step 1: 构建完整项目**

Run: `pnpm build 2>&1`
Expected: 前端构建 + Rust 编译成功，生成 `target/release/cc-switch-server`。

- [ ] **Step 2: 启动服务**

Run: `./target/release/cc-switch-server`
Expected: 服务启动，日志输出，监听 `127.0.0.1:10245`。

- [ ] **Step 3: 浏览器访问**

Open: `http://localhost:10245`
Expected: 前端加载，功能可用（Provider 管理、Settings 等）。

- [ ] **Step 4: API 测试**

Run: `curl -s -X POST http://localhost:10245/api/get_providers | head -50`
Expected: 返回 JSON 格式的 Providers 列表。

- [ ] **Step 5: SSE 事件测试**

Run: `timeout 5 curl -s http://localhost:10245/events`
Expected: SSE 连接建立，收到 keepalive 事件。
