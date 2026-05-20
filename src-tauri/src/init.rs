// [Custom] 二次开发: 公共初始化逻辑（不依赖 Tauri AppHandle）
//! 公共初始化逻辑（不依赖 Tauri AppHandle）
//!
//! 此模块从 `lib.rs` 的 `setup()` 闭包中提取了所有不依赖 `tauri::AppHandle` 的
//! 初始化代码，包括数据库初始化、配置迁移、预设数据导入等。
//!
//! 两种入口共享此逻辑：
//! - Tauri 桌面端 (`lib.rs::run()`)：`setup()` 闭包内仍保留原有内联初始化
//! - HTTP 服务端 (`lib.rs::run_server()`)：调用 `initialize_services()` 获得 AppState

use std::sync::Arc;
use tokio::sync::broadcast;

use crate::app_config::{AppType, MultiAppConfig};
use crate::database::Database;
use crate::error::AppError;
use crate::store::AppState;

/// 事件总线：替代 `tauri::Emitter`
///
/// 在非 Tauri 模式下，通过 channel 广播事件，各监听者自行过滤。
/// 消息格式为 `(event_name, payload_json)`。
pub type EventBus = broadcast::Sender<(String, String)>;

/// 初始化所有服务（不含 Tauri 特定部分）
///
/// ## 完成的工作
///
/// 1. 确保应用配置目录和日志目录存在
/// 2. 初始化 Panic Hook
/// 3. 初始化 rustls 加密提供者
/// 4. 初始化 SQLite 数据库（含 Schema 迁移）
/// 5. 从旧版 `config.json` 迁移数据到 SQLite（如需要）
/// 6. 创建 `AppState`（含 ProxyService）
/// 7. 创建事件总线
/// 8. 导入预设数据（Skills/Providers/MCP/Prompts/OMO）
/// 9. 初始化通用配置片段
/// 10. 恢复代理启动状态
/// 11. 初始化全局出站代理 HTTP 客户端
/// 12. 从数据库加载日志级别配置
pub async fn initialize_services() -> Result<(Arc<AppState>, EventBus), AppError> {
    // ============================================================
    // 1. 确保应用配置目录和日志目录存在
    // ============================================================
    let app_config_dir = crate::config::get_app_config_dir();
    std::fs::create_dir_all(&app_config_dir)
        .map_err(|e| AppError::Config(format!("创建配置目录失败: {e}")))?;

    let log_dir = app_config_dir.join("logs");
    let _ = std::fs::create_dir_all(&log_dir);

    // ============================================================
    // 2. 初始化 Panic Hook
    // ============================================================
    crate::panic_hook::setup_panic_hook();

    // ============================================================
    // 3. 初始化文件日志（stdout + ~/.cc-switch/logs/cc-switch.log）
    // ============================================================
    crate::logging::init(log_dir);
    log::info!("日志系统已初始化");

    // ============================================================
    // 4. 初始化 rustls 加密提供者
    // ============================================================
    let _ = rustls::crypto::ring::default_provider().install_default();

    // ============================================================
    // 4. 数据库初始化 + Schema 迁移
    // ============================================================
    let db_path = app_config_dir.join("cc-switch.db");
    let json_path = app_config_dir.join("config.json");

    // 检查是否需要从 config.json 迁移到 SQLite
    let has_json = json_path.exists();
    let has_db = db_path.exists();

    // 如果需要迁移，先验证 config.json 是否可以加载（在创建数据库之前）
    let migration_config = if !has_db && has_json {
        log::info!("检测到旧版配置文件，验证配置文件...");

        match MultiAppConfig::load() {
            Ok(config) => {
                log::info!("✓ 配置文件加载成功");
                Some(config)
            }
            Err(e) => {
                log::error!("加载旧配置文件失败: {e}");
                // 非 Tauri 模式下无法弹出对话框，直接返回错误
                return Err(AppError::Config(format!("加载旧配置文件失败: {e}")));
            }
        }
    } else {
        None
    };

    // 创建数据库（包含 Schema 迁移）
    let db = Arc::new(Database::init().map_err(|e| {
        log::error!("数据库初始化失败: {e}");
        e
    })?);

    // 如果有预加载的配置，执行迁移
    if let Some(config) = migration_config {
        log::info!("开始执行数据迁移...");

        match db.migrate_from_json(&config) {
            Ok(_) => {
                log::info!("✓ 配置迁移成功");
                crate::init_status::set_migration_success();
                // 归档旧配置文件（重命名而非删除，便于用户恢复）
                let archive_path = json_path.with_extension("json.migrated");
                if let Err(e) = std::fs::rename(&json_path, &archive_path) {
                    log::warn!("归档旧配置文件失败: {e}");
                } else {
                    log::info!("✓ 旧配置已归档为 config.json.migrated");
                }
            }
            Err(e) => {
                log::error!("配置迁移失败: {e}，将从现有配置导入");
            }
        }
    }

    // ============================================================
    // 5. 创建 AppState
    // ============================================================
    let app_state = Arc::new(AppState::new(db));

    // ============================================================
    // 6. 创建事件总线
    // ============================================================
    let (event_tx, _) = broadcast::channel(100);

    // ============================================================
    // 7. 导入预设数据（各类数据独立检查，互不影响）
    // ============================================================
    import_seed_data(&app_state)?;

    // ============================================================
    // 8. 初始化通用配置片段
    // ============================================================
    crate::initialize_common_config_snippets(&app_state);

    // ============================================================
    // 9. 恢复代理启动状态
    // ============================================================
    crate::restore_proxy_state_on_startup(&app_state).await;

    // ============================================================
    // 10. 初始化全局出站代理 HTTP 客户端
    // ============================================================
    {
        let proxy_url = app_state.db.get_global_proxy_url().ok().flatten();

        if let Err(e) = crate::proxy::http_client::init(proxy_url.as_deref()) {
            log::error!("[GlobalProxy] [GP-005] 使用保存的配置初始化失败: {e}");

            // 清除无效的代理配置
            if proxy_url.is_some() {
                log::warn!("[GlobalProxy] [GP-006] 清除数据库中无效的代理配置");
                if let Err(clear_err) = app_state.db.set_global_proxy_url(None) {
                    log::error!("[GlobalProxy] [GP-007] 清除无效配置失败: {clear_err}");
                }
            }

            // 使用直连模式重新初始化
            if let Err(fallback_err) = crate::proxy::http_client::init(None) {
                log::error!("[GlobalProxy] [GP-008] 初始化直连模式失败: {fallback_err}");
            }
        }
    }

    // ============================================================
    // 11. 从数据库加载日志配置并应用
    // ============================================================
    {
        if let Ok(log_config) = app_state.db.get_log_config() {
            log::set_max_level(log_config.to_level_filter());
            log::info!(
                "已加载日志配置: enabled={}, level={}",
                log_config.enabled,
                log_config.level
            );
        }
    }

    Ok((app_state, event_tx))
}

/// 导入预设数据（各类数据独立检查，互不影响）
///
/// 包含：
/// - Skills 仓库初始化 + SSOT 迁移
/// - Providers 导入（live 配置 + 官方预设）
/// - OpenCode / OpenClaw / Hermes providers 导入
/// - OMO 配置导入
/// - MCP 服务器配置导入
/// - Prompts 导入
fn import_seed_data(state: &AppState) -> Result<(), AppError> {
    // ============================================================
    // 1. 初始化默认 Skills 仓库
    // ============================================================
    match state.db.init_default_skill_repos() {
        Ok(count) if count > 0 => {
            log::info!("✓ 初始化了 {count} 个默认 Skill 仓库");
        }
        Ok(_) => {}
        Err(e) => log::warn!("✗ 初始化默认 Skill 仓库失败: {e}"),
    }

    // ============================================================
    // 1.1. Skills 统一管理迁移（SSOT）
    // ============================================================
    match state.db.get_setting("skills_ssot_migration_pending") {
        Ok(Some(flag)) if flag == "true" || flag == "1" => {
            let has_existing = state
                .db
                .get_all_installed_skills()
                .map(|skills| !skills.is_empty())
                .unwrap_or(false);

            if has_existing {
                log::info!(
                    "检测到 skills_ssot_migration_pending 但 Skills 表非空，跳过自动导入"
                );
                let _ = state.db.set_setting("skills_ssot_migration_pending", "false");
            } else {
                match crate::services::skill::migrate_skills_to_ssot(&state.db) {
                    Ok(count) => {
                        log::info!("✓ 自动导入 {count} 个 Skill 到 SSOT");
                        if count > 0 {
                            crate::init_status::set_skills_migration_result(count);
                        }
                        let _ =
                            state.db.set_setting("skills_ssot_migration_pending", "false");
                    }
                    Err(e) => {
                        log::warn!("✗ 自动导入遗留 Skills 到 SSOT 失败: {e}");
                        crate::init_status::set_skills_migration_error(e.to_string());
                    }
                }
            }
        }
        Ok(_) => {}
        Err(e) => log::warn!("✗ 读取 Skills 迁移标志失败: {e}"),
    }

    // ============================================================
    // 1.5. 自动导入 live 配置 + seed 官方预设供应商
    // ============================================================
    let first_run_already_confirmed =
        crate::settings::get_settings()
            .first_run_notice_confirmed
            .unwrap_or(false);
    let fresh_install_at_startup = state.db.is_providers_empty().unwrap_or(false);

    for app_type in AppType::all().filter(|t| !t.is_additive_mode()) {
        if !crate::services::provider::should_import_default_config_on_startup(state, &app_type)
            .unwrap_or(false)
        {
            log::debug!(
                "○ {} 已有 providers，跳过 live 导入",
                app_type.as_str()
            );
            continue;
        }

        match crate::services::provider::import_default_config(state, app_type.clone()) {
            Ok(true) => log::info!(
                "✓ 导入 {} 的 live 配置为默认 provider",
                app_type.as_str()
            ),
            Ok(false) => log::debug!(
                "○ {} 已有 providers，跳过 live 导入",
                app_type.as_str()
            ),
            Err(e) => log::debug!("○ 无 {} 的 live 配置可导入: {e}", app_type.as_str()),
        }
    }

    match state.db.init_default_official_providers() {
        Ok(count) if count > 0 => {
            log::info!("✓ 种子化 {count} 个官方 provider");
        }
        Ok(_) => {}
        Err(e) => log::warn!("✗ 种子化官方 providers 失败: {e}"),
    }

    if !first_run_already_confirmed && fresh_install_at_startup {
        log::info!("✓ 首次运行欢迎通知待显示");
    }

    // ============================================================
    // 1.6. 自动同步 OpenCode / OpenClaw / Hermes 的 live providers
    // ============================================================
    match crate::services::provider::import_opencode_providers_from_live(state) {
        Ok(count) if count > 0 => {
            log::info!("✓ 从 live 配置导入 {count} 个 OpenCode provider");
        }
        Ok(_) => log::debug!("○ 无新的 OpenCode providers 可导入"),
        Err(e) => log::warn!("✗ 导入 OpenCode providers 失败: {e}"),
    }
    match crate::services::provider::import_openclaw_providers_from_live(state) {
        Ok(count) if count > 0 => {
            log::info!("✓ 从 live 配置导入 {count} 个 OpenClaw provider");
        }
        Ok(_) => log::debug!("○ 无新的 OpenClaw providers 可导入"),
        Err(e) => log::warn!("✗ 导入 OpenClaw providers 失败: {e}"),
    }
    match crate::services::provider::import_hermes_providers_from_live(state) {
        Ok(count) if count > 0 => {
            log::info!("✓ 从 live 配置导入 {count} 个 Hermes provider");
        }
        Ok(_) => log::debug!("○ 无新的 Hermes providers 可导入"),
        Err(e) => log::warn!("✗ 导入 Hermes providers 失败: {e}"),
    }

    // ============================================================
    // 2. OMO 配置导入
    // ============================================================
    {
        let has_omo = state
            .db
            .get_all_providers("opencode")
            .map(|providers| providers.values().any(|p| p.category.as_deref() == Some("omo")))
            .unwrap_or(false);
        if !has_omo {
            match crate::services::OmoService::import_from_local(
                state,
                &crate::services::omo::STANDARD,
            ) {
                Ok(provider) => {
                    log::info!("✓ 从本地导入 OMO 配置为 provider '{}'", provider.name);
                }
                Err(AppError::OmoConfigNotFound) => {
                    log::debug!("○ 无 OMO 配置可导入");
                }
                Err(e) => {
                    log::warn!("✗ 从本地导入 OMO 配置失败: {e}");
                }
            }
        }
    }

    // ============================================================
    // 2.3. OMO Slim 配置导入
    // ============================================================
    {
        let has_omo_slim = state
            .db
            .get_all_providers("opencode")
            .map(|providers| {
                providers
                    .values()
                    .any(|p| p.category.as_deref() == Some("omo-slim"))
            })
            .unwrap_or(false);
        if !has_omo_slim {
            match crate::services::OmoService::import_from_local(
                state,
                &crate::services::omo::SLIM,
            ) {
                Ok(provider) => {
                    log::info!(
                        "✓ 从本地导入 OMO Slim 配置为 provider '{}'",
                        provider.name
                    );
                }
                Err(AppError::OmoConfigNotFound) => {
                    log::debug!("○ 无 OMO Slim 配置可导入");
                }
                Err(e) => {
                    log::warn!("✗ 从本地导入 OMO Slim 配置失败: {e}");
                }
            }
        }
    }

    // ============================================================
    // 3. 导入 MCP 服务器配置
    // ============================================================
    if state.db.is_mcp_table_empty().unwrap_or(false) {
        log::info!("MCP 表为空，从 live 配置导入...");

        match crate::services::mcp::McpService::import_from_claude(state) {
            Ok(count) if count > 0 => log::info!("✓ 从 Claude 导入 {count} 个 MCP 服务器"),
            Ok(_) => log::debug!("○ 无 Claude MCP 服务器可导入"),
            Err(e) => log::warn!("✗ 导入 Claude MCP 失败: {e}"),
        }
        match crate::services::mcp::McpService::import_from_codex(state) {
            Ok(count) if count > 0 => log::info!("✓ 从 Codex 导入 {count} 个 MCP 服务器"),
            Ok(_) => log::debug!("○ 无 Codex MCP 服务器可导入"),
            Err(e) => log::warn!("✗ 导入 Codex MCP 失败: {e}"),
        }
        match crate::services::mcp::McpService::import_from_gemini(state) {
            Ok(count) if count > 0 => log::info!("✓ 从 Gemini 导入 {count} 个 MCP 服务器"),
            Ok(_) => log::debug!("○ 无 Gemini MCP 服务器可导入"),
            Err(e) => log::warn!("✗ 导入 Gemini MCP 失败: {e}"),
        }
        match crate::services::mcp::McpService::import_from_opencode(state) {
            Ok(count) if count > 0 => log::info!("✓ 从 OpenCode 导入 {count} 个 MCP 服务器"),
            Ok(_) => log::debug!("○ 无 OpenCode MCP 服务器可导入"),
            Err(e) => log::warn!("✗ 导入 OpenCode MCP 失败: {e}"),
        }
        match crate::services::mcp::McpService::import_from_hermes(state) {
            Ok(count) if count > 0 => log::info!("✓ 从 Hermes 导入 {count} 个 MCP 服务器"),
            Ok(_) => log::debug!("○ 无 Hermes MCP 服务器可导入"),
            Err(e) => log::warn!("✗ 导入 Hermes MCP 失败: {e}"),
        }
    }

    // ============================================================
    // 4. 导入提示词文件
    // ============================================================
    if state.db.is_prompts_table_empty().unwrap_or(false) {
        log::info!("Prompts 表为空，从 live 配置导入...");

        for app in [
            AppType::Claude,
            AppType::Codex,
            AppType::Gemini,
            AppType::OpenCode,
            AppType::OpenClaw,
            AppType::Hermes,
        ] {
            match crate::services::prompt::PromptService::import_from_file_on_first_launch(
                state,
                app.clone(),
            ) {
                Ok(count) if count > 0 => {
                    log::info!("✓ 为 {} 导入 {count} 个 prompt", app.as_str());
                }
                Ok(_) => log::debug!("○ 未找到 {} 的 prompt 文件", app.as_str()),
                Err(e) => log::warn!("✗ 为 {} 导入 prompt 失败: {e}", app.as_str()),
            }
        }
    }

    Ok(())
}
