//! HTTP 服务入口（独立服务器模式）
//!
//! 提供独立的 axum HTTP 服务器，替代 Tauri 桌面端。
//! 支持 REST API、SSE 事件推送和静态文件服务。
//!
//! # 两种编译模式
//!
//! - **作为库模块**（`mod main_server` in `lib.rs`）：通过 `start_server()` 对外暴露，
//!   `crate::` 路径解析到库 crate。
//! - **作为独立二进制**（`[[bin]]` in `Cargo.toml`）：`fn main()` 作为入口，
//!   但当前阶段的 `crate::` 路径在二进制命名空间下不可用，属于预期内的过渡状态。

use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;

use axum::{
    extract::{Extension, Path, Request},
    response::{IntoResponse, Response, Sse},
    routing::{get, post},
    Json, Router,
};
use futures::stream::Stream;
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

use crate::init::EventBus;
use crate::store::AppState;

/// 启动 HTTP 服务器
///
/// 调用 `init::initialize_services()` 初始化数据库和 AppState，
/// 然后创建 axum 服务器监听 `CC_SWITCH_PORT`（默认 10245）。
///
/// 提供以下路由：
/// - `POST /api/{command}` — 命令调度（转发到 `handlers::dispatch`）
/// - `GET /events` — SSE 事件推送
/// - `GET /` — 前端 index.html
/// - `GET /assets/*` — 前端静态资源
pub async fn start_server() {
    // 初始化服务（数据库、AppState、事件总线等）
    let (state, event_bus) = crate::init::initialize_services()
        .await
        .expect("Failed to initialize services");

    let port = std::env::var("CC_SWITCH_PORT").unwrap_or_else(|_| "10245".to_string());

    let cors = tower_http::cors::CorsLayer::permissive();

    let app = Router::new()
        // API 路由 — 统一命令分发
        .route("/api/*command", post(api_dispatch))
        // SSE 事件推送
        .route("/events", get(sse_handler))
        // 静态文件服务
        .route("/", get(index_handler))
        .nest_service(
            "/assets",
            tower_http::services::ServeDir::new(format!("{}/assets", dist_dir())),
        )
        .layer(Extension(state))
        .layer(Extension(event_bus))
        .layer(cors);

    let addr: SocketAddr = format!("127.0.0.1:{}", port)
        .parse()
        .expect("Invalid address");

    log::info!("CC Switch server starting at http://{}", addr);
    println!("CC Switch server is running at http://{}", addr);
    println!("Press Ctrl+C to stop");

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("Failed to bind to address (is port already in use?)");

    axum::serve(listener, app)
        .await
        .expect("Server exited with error");
}

/// 前端构建产物目录
///
/// 优先级：
/// 1. 环境变量 `CC_SWITCH_DIST_DIR`
/// 2. 相对于可执行文件路径的 `../../dist`
fn dist_dir() -> String {
    std::env::var("CC_SWITCH_DIST_DIR").unwrap_or_else(|_| {
        let mut path = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.to_path_buf()))
            .unwrap_or_else(|| std::path::PathBuf::from("."));
        path.push("../../dist");
        path.to_string_lossy().to_string()
    })
}

/// 简单的 index.html 处理器 — 读取并返回静态 HTML
async fn index_handler() -> Response {
    let index_path = std::path::Path::new(&dist_dir()).join("index.html");
    match tokio::fs::read_to_string(&index_path).await {
        Ok(html) => (
            axum::http::StatusCode::OK,
            [(axum::http::header::CONTENT_TYPE, "text/html; charset=utf-8")],
            html,
        )
            .into_response(),
        Err(_) => (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({
                "error": "Frontend not built. Run `pnpm build:renderer` first."
            })),
        )
            .into_response(),
    }
}

/// API 统一调度 — 将请求转发到 `handlers::dispatch`
///
/// `handlers` 模块将在 Task 7 中实现。在此之前此函数会导致编译错误，
/// 因为 `crate::handlers` 尚未定义。
async fn api_dispatch(
    Extension(state): Extension<Arc<AppState>>,
    Path(command): Path<String>,
    req: Request,
) -> Response {
    crate::handlers::dispatch(&command, state, req).await
}

/// SSE 事件推送 — 替代 `tauri::Emitter`
///
/// 客户端连接后持续接收服务器推送的事件。
/// 每 15 秒发送一次心跳保持连接。
async fn sse_handler(
    Extension(event_bus): Extension<broadcast::Sender<(String, String)>>,
) -> Sse<impl Stream<Item = Result<axum::response::sse::Event, Infallible>>> {
    let rx = event_bus.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(|result| match result {
        Ok((event_name, payload)) => Some(Ok(
            axum::response::sse::Event::default().event(event_name).data(payload),
        )),
        Err(e) => {
            log::warn!("SSE: {e}");
            None
        }
    });

    Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(std::time::Duration::from_secs(15)),
    )
}

/// Binary entry point
///
/// 作为独立二进制运行时，通过库 crate 的 `run_server()` 间接启动，
/// 它会调用本模块的 `start_server()`。
///
/// 注意：当前阶段此文件同时作为库模块 (`mod main_server`) 编译，
/// `fn main` 是私有函数，不影响库的模块导出。当作为 `--bin cc-switch-server`
/// 编译时，此函数作为入口点，但 `start_server()` 等函数的 `crate::` 路径
/// 在二进制命名空间下不可用——属于过渡状态，后续任务会完善。
#[tokio::main]
async fn main() {
    start_server().await;
}
