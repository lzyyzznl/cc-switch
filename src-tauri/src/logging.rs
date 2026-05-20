// [Custom] 二次开发: 独立日志模块（server_only 模式使用）
//! 独立日志模块（server_only 模式使用）
//!
//! 替代 Tauri 模式下的 `tauri_plugin_log`，同时输出到 stdout 和文件。
//! 日志文件位于 `~/.cc-switch/logs/cc-switch.log`，单文件覆盖（启动时重置）。

use log::{LevelFilter, Log, Metadata, Record};
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

/// 同时在 stdout 和文件中输出日志
struct DualLogger {
    file: Mutex<File>,
}

impl Log for DualLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= log::max_level()
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let msg = format!(
            "[{} {} {}] {}\n",
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
            record.level(),
            record.target(),
            record.args()
        );
        // stdout（忽略错误）
        let _ = std::io::stdout().write_all(msg.as_bytes());
        // 文件（忽略错误）
        if let Ok(mut file) = self.file.lock() {
            let _ = file.write_all(msg.as_bytes());
        }
    }

    fn flush(&self) {
        if let Ok(mut file) = self.file.lock() {
            let _ = file.flush();
        }
    }
}

/// 初始化日志系统
///
/// 创建日志文件并注册 `DualLogger` 为全局 logger。
/// 启动时会删除旧的日志文件，实现单文件覆盖（与 Tauri 插件行为一致）。
///
/// # Panics
///
/// 如果无法创建或打开日志文件（例如父目录不存在）。
pub fn init(log_dir: PathBuf) {
    let log_path = log_dir.join("cc-switch.log");

    // 删除旧日志文件，实现单文件覆盖
    let _ = std::fs::remove_file(&log_path);

    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .unwrap_or_else(|e| panic!("无法创建日志文件 {}: {e}", log_path.display()));

    // 初始级别设为 Trace，后续由 DB 配置动态调整
    let r = log::set_boxed_logger(Box::new(DualLogger {
        file: Mutex::new(file),
    }));

    match r {
        Ok(()) => log::set_max_level(LevelFilter::Trace),
        Err(_) => {
            // 已有 logger 注册（如 Tauri 模式），不做任何事
        }
    }
}
