// [Custom] 二次开发: server_only 模式二进制入口
//! CC Switch Standalone Server — Binary Entry Point
//!
//! When compiled with `--features server_only`, this binary starts
//! the HTTP server. The actual server implementation is in `main_server`
//! module within the library crate (`cc_switch_lib`).

#[tokio::main]
async fn main() {
    cc_switch_lib::main_server::start_server().await;
}
