// [Custom] 二次开发: HTTP API 路由处理器（server_only 模式）
//! HTTP 命令调度
//!
//! 将 POST /api/{command} 请求分派到对应的 commands::* 函数。
//! 负责从 JSON 请求体中提取类型化参数，调用命令函数，并将结果序列化为 HTTP 响应。

use std::sync::{Arc, OnceLock};
use axum::{
    extract::Request,
    response::{IntoResponse, Response},
    Json,
};
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::commands::copilot::CopilotAuthState;
use crate::commands::codex_oauth::CodexOAuthState;
use crate::commands::skill::SkillServiceState;
use crate::store::AppState;

// ============================================================================
// 惰性初始化的全局状态（HTTP 模式下无 Tauri 状态管理）
// ============================================================================

fn copilot_state() -> Arc<CopilotAuthState> {
    static STATE: OnceLock<Arc<CopilotAuthState>> = OnceLock::new();
    STATE.get_or_init(|| {
        let dir = crate::config::get_app_config_dir();
        let manager = crate::proxy::providers::copilot_auth::CopilotAuthManager::new(dir);
        Arc::new(CopilotAuthState(Arc::new(tokio::sync::RwLock::new(manager))))
    }).clone()
}

fn codex_state() -> Arc<CodexOAuthState> {
    static STATE: OnceLock<Arc<CodexOAuthState>> = OnceLock::new();
    STATE.get_or_init(|| {
        let dir = crate::config::get_app_config_dir();
        let manager = crate::proxy::providers::codex_oauth_auth::CodexOAuthManager::new(dir);
        Arc::new(CodexOAuthState(Arc::new(tokio::sync::RwLock::new(manager))))
    }).clone()
}

fn skill_service() -> Arc<SkillServiceState> {
    static STATE: OnceLock<Arc<SkillServiceState>> = OnceLock::new();
    STATE.get_or_init(|| {
        let service = crate::services::skill::SkillService::new();
        Arc::new(SkillServiceState(Arc::new(service)))
    }).clone()
}

// ============================================================================
// 参数提取辅助函数
// ============================================================================

/// 提取必填字段，返回 serde 反序列化结果
fn req<T: DeserializeOwned>(args: &Option<Value>, key: &str) -> Result<T, String> {
    args.as_ref()
        .and_then(|v| v.get(key))
        .ok_or_else(|| format!("Missing required field: {key}"))
        .and_then(|v| serde_json::from_value(v.clone()).map_err(|e| format!("Invalid {key}: {e}")))
}

/// 提取可选字段
fn opt<T: DeserializeOwned>(args: &Option<Value>, key: &str) -> Option<T> {
    args.as_ref()
        .and_then(|v| v.get(key))
        .and_then(|v| serde_json::from_value(v.clone()).ok())
}

/// 提取必填字符串
fn req_str(args: &Option<Value>, key: &str) -> Result<String, String> {
    args.as_ref()
        .and_then(|v| v.get(key))
        .and_then(|v| v.as_str())
        .map(String::from)
        .ok_or_else(|| format!("Missing required string field: {key}"))
}

/// 提取可选字符串
fn opt_str(args: &Option<Value>, key: &str) -> Option<String> {
    args.as_ref()
        .and_then(|v| v.get(key))
        .and_then(|v| v.as_str())
        .map(String::from)
}

/// 提取可选布尔值
fn opt_bool(args: &Option<Value>, key: &str) -> Option<bool> {
    args.as_ref()
        .and_then(|v| v.get(key))
        .and_then(|v| v.as_bool())
}

/// 提取必填布尔值
fn req_bool(args: &Option<Value>, key: &str) -> Result<bool, String> {
    opt_bool(args, key).ok_or_else(|| format!("Missing required bool field: {key}"))
}

/// 提取可选 i64
fn opt_i64(args: &Option<Value>, key: &str) -> Option<i64> {
    args.as_ref()
        .and_then(|v| v.get(key))
        .and_then(|v| v.as_i64())
}

/// 提取必填 u32
fn req_u32(args: &Option<Value>, key: &str) -> Result<u32, String> {
    args.as_ref()
        .and_then(|v| v.get(key))
        .and_then(|v| v.as_u64())
        .map(|v| v as u32)
        .ok_or_else(|| format!("Missing required integer field: {key}"))
}

/// 提取必填 usize
fn req_usize(args: &Option<Value>, key: &str) -> Result<usize, String> {
    args.as_ref()
        .and_then(|v| v.get(key))
        .and_then(|v| v.as_u64())
        .map(|v| v as usize)
        .ok_or_else(|| format!("Missing required integer field: {key}"))
}

// ============================================================================
// 响应构建辅助函数
// ============================================================================

/// 将 Result<T, E> 转为 HTTP 响应，其中 T: Serialize
fn json_ok<T: serde::Serialize>(result: Result<T, impl IntoResponse>) -> Response {
    match result {
        Ok(val) => Json(serde_json::to_value(val).unwrap_or_default()).into_response(),
        Err(err) => {
            let resp = err.into_response();
            if resp.status() == axum::http::StatusCode::OK {
                (axum::http::StatusCode::INTERNAL_SERVER_ERROR, resp).into_response()
            } else {
                resp
            }
        },
    }
}

/// 将普通值转为 HTTP 响应
fn json_val<T: serde::Serialize>(val: T) -> Response {
    Json(serde_json::to_value(val).unwrap_or_default()).into_response()
}

// ============================================================================
// 统一命令调度
// ============================================================================

/// 统一命令调度入口
///
/// 从请求体中读取 JSON 参数，根据 command 名称分发到对应的命令函数。
pub async fn dispatch(command: &str, state: Arc<AppState>, req: Request) -> Response {
    let body_bytes = match axum::body::to_bytes(req.into_body(), 1024 * 1024).await {
        Ok(b) => b.to_vec(),
        Err(_) => {
            return (axum::http::StatusCode::BAD_REQUEST, "Failed to read body").into_response();
        }
    };
    let args: Option<Value> = serde_json::from_slice(&body_bytes).ok();
    match dispatch_inner(command, state, args).await {
        Ok(resp) => resp,
        Err(e) => (axum::http::StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": e}))).into_response(),
    }
}

async fn dispatch_inner(command: &str, state: Arc<AppState>, args: Option<Value>) -> Result<Response, String> {

    Ok(match command {
        // ====================================================================
        // Provider 命令
        // ====================================================================
        "get_providers" => {
            let app = req_str(&args, "app")?;
            json_ok(crate::commands::get_providers(state, app))
        }
        "get_current_provider" => {
            let app = req_str(&args, "app")?;
            json_ok(crate::commands::get_current_provider(state, app))
        }
        "add_provider" => {
            let app = req_str(&args, "app")?;
            let provider = req(&args, "provider")?;
            let add_to_live: Option<bool> = opt(&args, "addToLive");
            json_ok(crate::commands::add_provider(state, app, provider, add_to_live))
        }
        "update_provider" => {
            let app = req_str(&args, "app")?;
            let provider = req(&args, "provider")?;
            let original_id: Option<String> = opt(&args, "originalId");
            json_ok(crate::commands::update_provider(state, app, provider, original_id))
        }
        "delete_provider" => {
            let app = req_str(&args, "app")?;
            let id = req_str(&args, "id")?;
            json_ok(crate::commands::delete_provider(state, app, id))
        }
        "remove_provider_from_live_config" => {
            let app = req_str(&args, "app")?;
            let id = req_str(&args, "id")?;
            json_ok(crate::commands::remove_provider_from_live_config(state, app, id))
        }
        "switch_provider" => {
            let app = req_str(&args, "app")?;
            let id = req_str(&args, "id")?;
            json_ok(crate::commands::switch_provider(state, app, id))
        }
        "import_default_config" => {
            let app = req_str(&args, "app")?;
            json_ok(crate::commands::import_default_config(state, app))
        }
        "get_claude_desktop_status" => {
            json_ok(crate::commands::get_claude_desktop_status(state).await)
        }
        "get_claude_desktop_default_routes" => {
            json_val(crate::commands::get_claude_desktop_default_routes())
        }
        "import_claude_desktop_providers_from_claude" => {
            json_ok(crate::commands::import_claude_desktop_providers_from_claude(state))
        }
        "queryProviderUsage" => {
            let provider_id = req_str(&args, "providerId")?;
            let app = req_str(&args, "app")?;
            json_ok(crate::commands::queryProviderUsage(state, copilot_state(), provider_id, app).await)
        }
        "testUsageScript" => {
            let provider_id = req_str(&args, "providerId")?;
            let app = req_str(&args, "app")?;
            let script_code = req_str(&args, "scriptCode")?;
            let timeout: Option<u64> = opt(&args, "timeout");
            let api_key: Option<String> = opt(&args, "apiKey");
            let base_url: Option<String> = opt(&args, "baseUrl");
            let access_token: Option<String> = opt(&args, "accessToken");
            let user_id: Option<String> = opt(&args, "userId");
            let template_type: Option<String> = opt(&args, "templateType");
            json_ok(
                crate::commands::testUsageScript(
                    state, provider_id, app, script_code, timeout,
                    api_key, base_url, access_token, user_id, template_type,
                ).await
            )
        }
        "read_live_provider_settings" => {
            let app = req_str(&args, "app")?;
            json_ok(crate::commands::read_live_provider_settings(app))
        }
        "test_api_endpoints" => {
            let urls = req(&args, "urls")?;
            let timeout_secs: Option<u64> = opt(&args, "timeoutSecs");
            json_ok(crate::commands::test_api_endpoints(urls, timeout_secs).await)
        }
        "get_custom_endpoints" => {
            let app = req_str(&args, "app")?;
            let provider_id = req_str(&args, "providerId")?;
            json_ok(crate::commands::get_custom_endpoints(state, app, provider_id))
        }
        "add_custom_endpoint" => {
            let app = req_str(&args, "app")?;
            let provider_id = req_str(&args, "providerId")?;
            let url = req_str(&args, "url")?;
            json_ok(crate::commands::add_custom_endpoint(state, app, provider_id, url))
        }
        "remove_custom_endpoint" => {
            let app = req_str(&args, "app")?;
            let provider_id = req_str(&args, "providerId")?;
            let url = req_str(&args, "url")?;
            json_ok(crate::commands::remove_custom_endpoint(state, app, provider_id, url))
        }
        "update_endpoint_last_used" => {
            let app = req_str(&args, "app")?;
            let provider_id = req_str(&args, "providerId")?;
            let url = req_str(&args, "url")?;
            json_ok(crate::commands::update_endpoint_last_used(state, app, provider_id, url))
        }
        "update_providers_sort_order" => {
            let app = req_str(&args, "app")?;
            let updates = req(&args, "updates")?;
            json_ok(crate::commands::update_providers_sort_order(state, app, updates))
        }
        "get_universal_providers" => {
            json_ok(crate::commands::get_universal_providers(state))
        }
        "get_universal_provider" => {
            let id = req_str(&args, "id")?;
            json_ok(crate::commands::get_universal_provider(state, id))
        }
        "upsert_universal_provider" => {
            let provider = req(&args, "provider")?;
            json_ok(crate::commands::upsert_universal_provider(state, provider))
        }
        "delete_universal_provider" => {
            let id = req_str(&args, "id")?;
            json_ok(crate::commands::delete_universal_provider(state, id))
        }
        "sync_universal_provider" => {
            let id = req_str(&args, "id")?;
            json_ok(crate::commands::sync_universal_provider(state, id))
        }
        "import_opencode_providers_from_live" => {
            json_ok(crate::commands::import_opencode_providers_from_live(state))
        }
        "get_opencode_live_provider_ids" => {
            json_ok(crate::commands::get_opencode_live_provider_ids())
        }

        // ====================================================================
        // 配置相关命令 (config.rs)
        // ====================================================================
        "get_claude_config_status" => {
            json_ok(crate::commands::get_claude_config_status().await)
        }
        "get_config_status" => {
            let app = req_str(&args, "app")?;
            json_ok(crate::commands::get_config_status(state, app).await)
        }
        "get_claude_code_config_path" => {
            json_ok(crate::commands::get_claude_code_config_path().await)
        }
        "get_config_dir" => {
            let app = req_str(&args, "app")?;
            json_ok(crate::commands::get_config_dir(app).await)
        }
        "open_config_folder" => {
            let app = req_str(&args, "app")?;
            json_ok(crate::commands::open_config_folder(app).await)
        }
        "pick_directory" => {
            let default_path: Option<String> = opt(&args, "defaultPath");
            json_ok(crate::commands::pick_directory(default_path).await)
        }
        "open_external" => {
            let url = req_str(&args, "url")?;
            json_ok(crate::commands::open_external(url).await)
        }
        "get_init_error" => {
            json_ok(crate::commands::get_init_error().await)
        }
        "get_migration_result" => {
            json_ok(crate::commands::get_migration_result().await)
        }
        "get_skills_migration_result" => {
            json_ok(crate::commands::get_skills_migration_result().await)
        }
        "get_app_config_path" => {
            json_ok(crate::commands::get_app_config_path().await)
        }
        "open_app_config_folder" => {
            json_ok(crate::commands::open_app_config_folder().await)
        }
        "get_claude_common_config_snippet" => {
            json_ok(crate::commands::get_claude_common_config_snippet(state).await)
        }
        "set_claude_common_config_snippet" => {
            let snippet = req_str(&args, "snippet")?;
            json_ok(crate::commands::set_claude_common_config_snippet(snippet, state).await)
        }
        "get_common_config_snippet" => {
            let app_type = req_str(&args, "appType")?;
            json_ok(crate::commands::get_common_config_snippet(app_type, state).await)
        }
        "set_common_config_snippet" => {
            let app_type = req_str(&args, "appType")?;
            let snippet = req_str(&args, "snippet")?;
            json_ok(crate::commands::set_common_config_snippet(app_type, snippet, state).await)
        }
        "extract_common_config_snippet" => {
            let app_type = req_str(&args, "appType")?;
            let settings_config: Option<String> = opt(&args, "settingsConfig");
            json_ok(crate::commands::extract_common_config_snippet(app_type, settings_config, state).await)
        }

        // ====================================================================
        // Misc 命令
        // ====================================================================
        "copy_text_to_clipboard" => {
            let text = req_str(&args, "text")?;
            json_ok(crate::commands::copy_text_to_clipboard(text).await)
        }
        "check_for_updates" => {
            json_ok(crate::commands::check_for_updates().await)
        }
        "is_portable_mode" => {
            json_ok(crate::commands::is_portable_mode().await)
        }
        "get_tool_versions" => {
            let tools: Option<Vec<String>> = opt(&args, "tools");
            let wsl_shell_by_tool: Option<std::collections::HashMap<String, crate::commands::misc::WslShellPreferenceInput>> = opt(&args, "wslShellByTool");
            json_ok(crate::commands::get_tool_versions(tools, wsl_shell_by_tool).await)
        }
        "open_provider_terminal" => {
            let app = req_str(&args, "app")?;
            let provider_id = req_str(&args, "providerId")?;
            let cwd: Option<String> = opt(&args, "cwd");
            json_ok(crate::commands::open_provider_terminal(state, app, provider_id, cwd).await)
        }
        "set_window_theme" => {
            let theme = req_str(&args, "theme")?;
            json_ok(crate::commands::set_window_theme(theme).await)
        }

        // ====================================================================
        // Settings 命令
        // ====================================================================
        "get_settings" => {
            json_ok(crate::commands::get_settings().await)
        }
        "save_settings" => {
            let settings = req(&args, "settings")?;
            json_ok(crate::commands::save_settings(settings).await)
        }
        "restart_app" => {
            json_ok(crate::commands::restart_app().await)
        }
        "get_app_config_dir_override" => {
            json_ok(crate::commands::get_app_config_dir_override().await)
        }
        "set_app_config_dir_override" => {
            let path: Option<String> = opt(&args, "path");
            json_ok(crate::commands::set_app_config_dir_override(path).await)
        }
        "get_rectifier_config" => {
            json_ok(crate::commands::get_rectifier_config(state).await)
        }
        "set_rectifier_config" => {
            let config = req(&args, "config")?;
            json_ok(crate::commands::set_rectifier_config(state, config).await)
        }
        "get_optimizer_config" => {
            json_ok(crate::commands::get_optimizer_config(state).await)
        }
        "set_optimizer_config" => {
            let config = req(&args, "config")?;
            json_ok(crate::commands::set_optimizer_config(state, config).await)
        }
        "get_copilot_optimizer_config" => {
            json_ok(crate::commands::get_copilot_optimizer_config(state).await)
        }
        "set_copilot_optimizer_config" => {
            let config = req(&args, "config")?;
            json_ok(crate::commands::set_copilot_optimizer_config(state, config).await)
        }
        "get_log_config" => {
            json_ok(crate::commands::get_log_config(state).await)
        }
        "set_log_config" => {
            let config = req(&args, "config")?;
            json_ok(crate::commands::set_log_config(state, config).await)
        }

        // ====================================================================
        // Plugin 命令
        // ====================================================================
        "get_claude_plugin_status" => {
            json_ok(crate::commands::get_claude_plugin_status().await)
        }
        "read_claude_plugin_config" => {
            json_ok(crate::commands::read_claude_plugin_config().await)
        }
        "apply_claude_plugin_config" => {
            let official = req_bool(&args, "official")?;
            json_ok(crate::commands::apply_claude_plugin_config(official).await)
        }
        "is_claude_plugin_applied" => {
            json_ok(crate::commands::is_claude_plugin_applied().await)
        }
        "apply_claude_onboarding_skip" => {
            json_ok(crate::commands::apply_claude_onboarding_skip().await)
        }
        "clear_claude_onboarding_skip" => {
            json_ok(crate::commands::clear_claude_onboarding_skip().await)
        }

        // ====================================================================
        // Claude MCP 命令
        // ====================================================================
        "get_claude_mcp_status" => {
            json_ok(crate::commands::get_claude_mcp_status().await)
        }
        "read_claude_mcp_config" => {
            json_ok(crate::commands::read_claude_mcp_config().await)
        }
        "upsert_claude_mcp_server" => {
            let id = req_str(&args, "id")?;
            let spec = req(&args, "spec")?;
            json_ok(crate::commands::upsert_claude_mcp_server(id, spec).await)
        }
        "delete_claude_mcp_server" => {
            let id = req_str(&args, "id")?;
            json_ok(crate::commands::delete_claude_mcp_server(id).await)
        }
        "validate_mcp_command" => {
            let cmd = req_str(&args, "cmd")?;
            json_ok(crate::commands::validate_mcp_command(cmd).await)
        }

        // ====================================================================
        // MCP Config 命令
        // ====================================================================
        "get_mcp_config" => {
            let app = req_str(&args, "app")?;
            json_ok(crate::commands::get_mcp_config(state, app).await)
        }
        "upsert_mcp_server_in_config" => {
            let app = req_str(&args, "app")?;
            let id = req_str(&args, "id")?;
            let spec = req(&args, "spec")?;
            let sync_other_side: Option<bool> = opt(&args, "syncOtherSide");
            json_ok(crate::commands::upsert_mcp_server_in_config(state, app, id, spec, sync_other_side).await)
        }
        "delete_mcp_server_in_config" => {
            let app = req_str(&args, "app")?;
            let id = req_str(&args, "id")?;
            json_ok(crate::commands::delete_mcp_server_in_config(state, app, id).await)
        }
        "set_mcp_enabled" => {
            let app = req_str(&args, "app")?;
            let id = req_str(&args, "id")?;
            let enabled = req_bool(&args, "enabled")?;
            json_ok(crate::commands::set_mcp_enabled(state, app, id, enabled).await)
        }
        "get_mcp_servers" => {
            json_ok(crate::commands::get_mcp_servers(state).await)
        }
        "upsert_mcp_server" => {
            let server = req(&args, "server")?;
            json_ok(crate::commands::upsert_mcp_server(state, server).await)
        }
        "delete_mcp_server" => {
            let id = req_str(&args, "id")?;
            json_ok(crate::commands::delete_mcp_server(state, id).await)
        }
        "toggle_mcp_app" => {
            let server_id = req_str(&args, "serverId")?;
            let app = req_str(&args, "app")?;
            let enabled = req_bool(&args, "enabled")?;
            json_ok(crate::commands::toggle_mcp_app(state, server_id, app, enabled).await)
        }
        "import_mcp_from_apps" => {
            json_ok(crate::commands::import_mcp_from_apps(state).await)
        }

        // ====================================================================
        // Prompt 命令
        // ====================================================================
        "get_prompts" => {
            let app = req_str(&args, "app")?;
            json_ok(crate::commands::get_prompts(app, state).await)
        }
        "upsert_prompt" => {
            let app = req_str(&args, "app")?;
            let id = req_str(&args, "id")?;
            let prompt = req(&args, "prompt")?;
            json_ok(crate::commands::upsert_prompt(app, id, prompt, state).await)
        }
        "delete_prompt" => {
            let app = req_str(&args, "app")?;
            let id = req_str(&args, "id")?;
            json_ok(crate::commands::delete_prompt(app, id, state).await)
        }
        "enable_prompt" => {
            let app = req_str(&args, "app")?;
            let id = req_str(&args, "id")?;
            json_ok(crate::commands::enable_prompt(app, id, state).await)
        }
        "import_prompt_from_file" => {
            let app = req_str(&args, "app")?;
            json_ok(crate::commands::import_prompt_from_file(app, state).await)
        }
        "get_current_prompt_file_content" => {
            let app = req_str(&args, "app")?;
            json_ok(crate::commands::get_current_prompt_file_content(app).await)
        }

        // ====================================================================
        // Model 命令
        // ====================================================================
        "fetch_models_for_config" => {
            let base_url = req_str(&args, "baseUrl")?;
            let api_key = req_str(&args, "apiKey")?;
            let is_full_url: Option<bool> = opt(&args, "isFullUrl");
            let models_url: Option<String> = opt(&args, "modelsUrl");
            json_ok(crate::commands::fetch_models_for_config(base_url, api_key, is_full_url, models_url).await)
        }

        // ====================================================================
        // Usage/Subscription/Balance/Coding Plan 命令
        // ====================================================================
        "get_subscription_quota" => {
            let tool = req_str(&args, "tool")?;
            json_ok(crate::commands::get_subscription_quota(state, tool).await)
        }
        "get_codex_oauth_quota" => {
            let account_id: Option<String> = opt(&args, "accountId");
            json_ok(crate::commands::get_codex_oauth_quota(account_id, codex_state()).await)
        }
        "get_codex_oauth_models" => {
            let account_id: Option<String> = opt(&args, "accountId");
            json_ok(crate::commands::get_codex_oauth_models(account_id, codex_state()).await)
        }
        "get_coding_plan_quota" => {
            let base_url = req_str(&args, "baseUrl")?;
            let api_key = req_str(&args, "apiKey")?;
            json_ok(crate::commands::get_coding_plan_quota(base_url, api_key).await)
        }
        "get_balance" => {
            let base_url = req_str(&args, "baseUrl")?;
            let api_key = req_str(&args, "apiKey")?;
            json_ok(crate::commands::get_balance(base_url, api_key).await)
        }

        // ====================================================================
        // Proxy 命令
        // ====================================================================
        "start_proxy_server" => {
            json_ok(crate::commands::start_proxy_server(state).await)
        }
        "stop_proxy_server" => {
            json_ok(crate::commands::stop_proxy_server(state).await)
        }
        "stop_proxy_with_restore" => {
            json_ok(crate::commands::stop_proxy_with_restore(state).await)
        }
        "get_proxy_takeover_status" => {
            json_ok(crate::commands::get_proxy_takeover_status(state).await)
        }
        "set_proxy_takeover_for_app" => {
            let app_type = req_str(&args, "appType")?;
            let enabled = req_bool(&args, "enabled")?;
            json_ok(crate::commands::set_proxy_takeover_for_app(state, app_type, enabled).await)
        }
        "get_proxy_status" => {
            json_ok(crate::commands::get_proxy_status(state).await)
        }
        "get_proxy_config" => {
            json_ok(crate::commands::get_proxy_config(state).await)
        }
        "update_proxy_config" => {
            let config = req(&args, "config")?;
            json_ok(crate::commands::update_proxy_config(state, config).await)
        }
        "get_global_proxy_config" => {
            json_ok(crate::commands::get_global_proxy_config(state).await)
        }
        "update_global_proxy_config" => {
            let config = req(&args, "config")?;
            json_ok(crate::commands::update_global_proxy_config(state, config).await)
        }
        "get_proxy_config_for_app" => {
            let app_type = req_str(&args, "appType")?;
            json_ok(crate::commands::get_proxy_config_for_app(state, app_type).await)
        }
        "update_proxy_config_for_app" => {
            let config = req(&args, "config")?;
            json_ok(crate::commands::update_proxy_config_for_app(state, config).await)
        }
        "get_default_cost_multiplier" => {
            let app_type = req_str(&args, "appType")?;
            json_ok(crate::commands::get_default_cost_multiplier(state, app_type).await)
        }
        "set_default_cost_multiplier" => {
            let app_type = req_str(&args, "appType")?;
            let value = req_str(&args, "value")?;
            json_ok(crate::commands::set_default_cost_multiplier(state, app_type, value).await)
        }
        "get_pricing_model_source" => {
            let app_type = req_str(&args, "appType")?;
            json_ok(crate::commands::get_pricing_model_source(state, app_type).await)
        }
        "set_pricing_model_source" => {
            let app_type = req_str(&args, "appType")?;
            let value = req_str(&args, "value")?;
            json_ok(crate::commands::set_pricing_model_source(state, app_type, value).await)
        }
        "is_proxy_running" => {
            json_ok(crate::commands::is_proxy_running(state).await)
        }
        "is_live_takeover_active" => {
            json_ok(crate::commands::is_live_takeover_active(state).await)
        }
        "switch_proxy_provider" => {
            let app_type = req_str(&args, "appType")?;
            let provider_id = req_str(&args, "providerId")?;
            json_ok(crate::commands::switch_proxy_provider(state, app_type, provider_id).await)
        }
        "get_provider_health" => {
            let provider_id = req_str(&args, "providerId")?;
            let app_type = req_str(&args, "appType")?;
            json_ok(crate::commands::get_provider_health(state, provider_id, app_type).await)
        }
        "reset_circuit_breaker" => {
            let provider_id = req_str(&args, "providerId")?;
            let app_type = req_str(&args, "appType")?;
            json_ok(crate::commands::reset_circuit_breaker(state, provider_id, app_type).await)
        }
        "get_circuit_breaker_config" => {
            json_ok(crate::commands::get_circuit_breaker_config(state).await)
        }
        "update_circuit_breaker_config" => {
            let config = req(&args, "config")?;
            json_ok(crate::commands::update_circuit_breaker_config(state, config).await)
        }
        "get_circuit_breaker_stats" => {
            let provider_id = req_str(&args, "providerId")?;
            let app_type = req_str(&args, "appType")?;
            json_ok(crate::commands::get_circuit_breaker_stats(state, provider_id, app_type).await)
        }

        // ====================================================================
        // Failover 命令
        // ====================================================================
        "get_failover_queue" => {
            let app_type = req_str(&args, "appType")?;
            json_ok(crate::commands::get_failover_queue(state, app_type).await)
        }
        "get_available_providers_for_failover" => {
            let app_type = req_str(&args, "appType")?;
            json_ok(crate::commands::get_available_providers_for_failover(state, app_type).await)
        }
        "add_to_failover_queue" => {
            let app_type = req_str(&args, "appType")?;
            let provider_id = req_str(&args, "providerId")?;
            json_ok(crate::commands::add_to_failover_queue(state, app_type, provider_id).await)
        }
        "remove_from_failover_queue" => {
            let app_type = req_str(&args, "appType")?;
            let provider_id = req_str(&args, "providerId")?;
            json_ok(crate::commands::remove_from_failover_queue(state, app_type, provider_id).await)
        }
        "get_auto_failover_enabled" => {
            let app_type = req_str(&args, "appType")?;
            json_ok(crate::commands::get_auto_failover_enabled(state, app_type).await)
        }
        "set_auto_failover_enabled" => {
            let app_type = req_str(&args, "appType")?;
            let enabled = req_bool(&args, "enabled")?;
            json_ok(crate::commands::set_auto_failover_enabled(state, app_type, enabled).await)
        }

        // ====================================================================
        // Usage Stats 命令
        // ====================================================================
        "get_usage_summary" => {
            let start_date: Option<i64> = opt_i64(&args, "startDate");
            let end_date: Option<i64> = opt_i64(&args, "endDate");
            let app_type: Option<String> = opt_str(&args, "appType");
            json_ok(crate::commands::get_usage_summary(state, start_date, end_date, app_type))
        }
        "get_usage_summary_by_app" => {
            let start_date: Option<i64> = opt_i64(&args, "startDate");
            let end_date: Option<i64> = opt_i64(&args, "endDate");
            json_ok(crate::commands::get_usage_summary_by_app(state, start_date, end_date))
        }
        "get_usage_trends" => {
            let start_date: Option<i64> = opt_i64(&args, "startDate");
            let end_date: Option<i64> = opt_i64(&args, "endDate");
            let app_type: Option<String> = opt_str(&args, "appType");
            json_ok(crate::commands::get_usage_trends(state, start_date, end_date, app_type))
        }
        "get_provider_stats" => {
            let start_date: Option<i64> = opt_i64(&args, "startDate");
            let end_date: Option<i64> = opt_i64(&args, "endDate");
            let app_type: Option<String> = opt_str(&args, "appType");
            json_ok(crate::commands::get_provider_stats(state, start_date, end_date, app_type))
        }
        "get_model_stats" => {
            let start_date: Option<i64> = opt_i64(&args, "startDate");
            let end_date: Option<i64> = opt_i64(&args, "endDate");
            let app_type: Option<String> = opt_str(&args, "appType");
            json_ok(crate::commands::get_model_stats(state, start_date, end_date, app_type))
        }
        "get_request_logs" => {
            let filters = req(&args, "filters")?;
            let page = req_u32(&args, "page")?;
            let page_size = req_u32(&args, "pageSize")?;
            json_ok(crate::commands::get_request_logs(state, filters, page, page_size))
        }
        "get_request_detail" => {
            let request_id = req_str(&args, "requestId")?;
            json_ok(crate::commands::get_request_detail(state, request_id))
        }
        "get_model_pricing" => {
            json_ok(crate::commands::get_model_pricing(state))
        }
        "update_model_pricing" => {
            let model_id = req_str(&args, "modelId")?;
            let display_name = req_str(&args, "displayName")?;
            let input_cost = req_str(&args, "inputCost")?;
            let output_cost = req_str(&args, "outputCost")?;
            let cache_read_cost = req_str(&args, "cacheReadCost")?;
            let cache_creation_cost = req_str(&args, "cacheCreationCost")?;
            json_ok(crate::commands::update_model_pricing(state, model_id, display_name, input_cost, output_cost, cache_read_cost, cache_creation_cost))
        }
        "delete_model_pricing" => {
            let model_id = req_str(&args, "modelId")?;
            json_ok(crate::commands::delete_model_pricing(state, model_id))
        }
        "check_provider_limits" => {
            let provider_id = req_str(&args, "providerId")?;
            let app_type = req_str(&args, "appType")?;
            json_ok(crate::commands::check_provider_limits(state, provider_id, app_type))
        }
        "sync_session_usage" => {
            json_ok(crate::commands::sync_session_usage(state))
        }
        "get_usage_data_sources" => {
            json_ok(crate::commands::get_usage_data_sources(state))
        }

        // ====================================================================
        // Stream Check 命令
        // ====================================================================
        "stream_check_provider" => {
            let app_type: crate::app_config::AppType = req(&args, "appType")?;
            let provider_id = req_str(&args, "providerId")?;
            json_ok(crate::commands::stream_check_provider(state, copilot_state(), app_type, provider_id).await)
        }
        "stream_check_all_providers" => {
            let app_type: crate::app_config::AppType = req(&args, "appType")?;
            let proxy_targets_only = req_bool(&args, "proxyTargetsOnly")?;
            json_ok(crate::commands::stream_check_all_providers(state, copilot_state(), app_type, proxy_targets_only).await)
        }
        "get_stream_check_config" => {
            json_ok(crate::commands::get_stream_check_config(state))
        }
        "save_stream_check_config" => {
            let config = req(&args, "config")?;
            json_ok(crate::commands::save_stream_check_config(state, config))
        }

        // ====================================================================
        // Session 命令
        // ====================================================================
        "list_sessions" => {
            json_ok(crate::commands::list_sessions().await)
        }
        "get_session_messages" => {
            let provider_id = req_str(&args, "providerId")?;
            let source_path = req_str(&args, "sourcePath")?;
            json_ok(crate::commands::get_session_messages(provider_id, source_path).await)
        }
        "delete_session" => {
            let provider_id = req_str(&args, "providerId")?;
            let session_id = req_str(&args, "sessionId")?;
            let source_path = req_str(&args, "sourcePath")?;
            json_ok(crate::commands::delete_session(provider_id, session_id, source_path).await)
        }
        "delete_sessions" => {
            let items = req(&args, "items")?;
            json_ok(crate::commands::delete_sessions(items).await)
        }
        "launch_session_terminal" => {
            let command = req_str(&args, "command")?;
            let cwd: Option<String> = opt_str(&args, "cwd");
            let custom_config: Option<String> = opt_str(&args, "customConfig");
            json_ok(crate::commands::launch_session_terminal(command, cwd, custom_config).await)
        }

        // ====================================================================
        // OpenClaw 命令
        // ====================================================================
        "import_openclaw_providers_from_live" => {
            json_ok(crate::commands::import_openclaw_providers_from_live(state))
        }
        "get_openclaw_live_provider_ids" => {
            json_ok(crate::commands::get_openclaw_live_provider_ids())
        }
        "get_openclaw_live_provider" => {
            let provider_id = req_str(&args, "providerId")?;
            json_ok(crate::commands::get_openclaw_live_provider(provider_id))
        }
        "scan_openclaw_config_health" => {
            json_ok(crate::commands::scan_openclaw_config_health())
        }
        "get_openclaw_default_model" => {
            json_ok(crate::commands::get_openclaw_default_model())
        }
        "set_openclaw_default_model" => {
            let model = req(&args, "model")?;
            json_ok(crate::commands::set_openclaw_default_model(model))
        }
        "get_openclaw_model_catalog" => {
            json_ok(crate::commands::get_openclaw_model_catalog())
        }
        "set_openclaw_model_catalog" => {
            let catalog = req(&args, "catalog")?;
            json_ok(crate::commands::set_openclaw_model_catalog(catalog))
        }
        "get_openclaw_agents_defaults" => {
            json_ok(crate::commands::get_openclaw_agents_defaults())
        }
        "set_openclaw_agents_defaults" => {
            let defaults = req(&args, "defaults")?;
            json_ok(crate::commands::set_openclaw_agents_defaults(defaults))
        }
        "get_openclaw_env" => {
            json_ok(crate::commands::get_openclaw_env())
        }
        "set_openclaw_env" => {
            let env = req(&args, "env")?;
            json_ok(crate::commands::set_openclaw_env(env))
        }
        "get_openclaw_tools" => {
            json_ok(crate::commands::get_openclaw_tools())
        }
        "set_openclaw_tools" => {
            let tools = req(&args, "tools")?;
            json_ok(crate::commands::set_openclaw_tools(tools))
        }

        // ====================================================================
        // Hermes 命令
        // ====================================================================
        "import_hermes_providers_from_live" => {
            json_ok(crate::commands::import_hermes_providers_from_live(state))
        }
        "get_hermes_live_provider_ids" => {
            json_ok(crate::commands::get_hermes_live_provider_ids())
        }
        "get_hermes_live_provider" => {
            let provider_id = req_str(&args, "providerId")?;
            json_ok(crate::commands::get_hermes_live_provider(provider_id))
        }
        "get_hermes_model_config" => {
            json_ok(crate::commands::get_hermes_model_config())
        }
        "open_hermes_web_ui" => {
            let path: Option<String> = opt_str(&args, "path");
            json_ok(crate::commands::open_hermes_web_ui(path).await)
        }
        "launch_hermes_dashboard" => {
            json_ok(crate::commands::launch_hermes_dashboard().await)
        }
        "get_hermes_memory" => {
            let kind = req(&args, "kind")?;
            json_ok(crate::commands::get_hermes_memory(kind))
        }
        "set_hermes_memory" => {
            let kind = req(&args, "kind")?;
            let content = req_str(&args, "content")?;
            json_ok(crate::commands::set_hermes_memory(kind, content))
        }
        "get_hermes_memory_limits" => {
            json_ok(crate::commands::get_hermes_memory_limits())
        }
        "set_hermes_memory_enabled" => {
            let kind = req(&args, "kind")?;
            let enabled = req_bool(&args, "enabled")?;
            json_ok(crate::commands::set_hermes_memory_enabled(kind, enabled))
        }

        // ====================================================================
        // Global Proxy 命令
        // ====================================================================
        "get_global_proxy_url" => {
            json_ok(crate::commands::get_global_proxy_url(state))
        }
        "set_global_proxy_url" => {
            let url = req_str(&args, "url")?;
            json_ok(crate::commands::set_global_proxy_url(state, url))
        }
        "test_proxy_url" => {
            let url = req_str(&args, "url")?;
            json_ok(crate::commands::test_proxy_url(url).await)
        }
        "get_upstream_proxy_status" => {
            json_val(crate::commands::get_upstream_proxy_status())
        }
        "scan_local_proxies" => {
            json_val(crate::commands::scan_local_proxies().await)
        }

        // ====================================================================
        // Auth 命令
        // ====================================================================
        "auth_start_login" => {
            let auth_provider = req_str(&args, "authProvider")?;
            let github_domain: Option<String> = opt_str(&args, "githubDomain");
            json_ok(crate::commands::auth_start_login(auth_provider, github_domain, copilot_state(), codex_state()).await)
        }
        "auth_poll_for_account" => {
            let auth_provider = req_str(&args, "authProvider")?;
            let device_code = req_str(&args, "deviceCode")?;
            let github_domain: Option<String> = opt_str(&args, "githubDomain");
            json_ok(crate::commands::auth_poll_for_account(auth_provider, device_code, github_domain, copilot_state(), codex_state()).await)
        }
        "auth_list_accounts" => {
            let auth_provider = req_str(&args, "authProvider")?;
            json_ok(crate::commands::auth_list_accounts(auth_provider, copilot_state(), codex_state()).await)
        }
        "auth_get_status" => {
            let auth_provider = req_str(&args, "authProvider")?;
            json_ok(crate::commands::auth_get_status(auth_provider, copilot_state(), codex_state()).await)
        }
        "auth_remove_account" => {
            let auth_provider = req_str(&args, "authProvider")?;
            let account_id = req_str(&args, "accountId")?;
            json_ok(crate::commands::auth_remove_account(auth_provider, account_id, copilot_state(), codex_state()).await)
        }
        "auth_set_default_account" => {
            let auth_provider = req_str(&args, "authProvider")?;
            let account_id = req_str(&args, "accountId")?;
            json_ok(crate::commands::auth_set_default_account(auth_provider, account_id, copilot_state(), codex_state()).await)
        }
        "auth_logout" => {
            let auth_provider = req_str(&args, "authProvider")?;
            json_ok(crate::commands::auth_logout(auth_provider, copilot_state(), codex_state()).await)
        }

        // ====================================================================
        // Copilot 命令
        // ====================================================================
        "copilot_start_device_flow" => {
            let github_domain: Option<String> = opt_str(&args, "githubDomain");
            json_ok(crate::commands::copilot_start_device_flow(github_domain, copilot_state()).await)
        }
        "copilot_poll_for_auth" => {
            let device_code = req_str(&args, "deviceCode")?;
            let github_domain: Option<String> = opt_str(&args, "githubDomain");
            json_ok(crate::commands::copilot_poll_for_auth(device_code, github_domain, copilot_state()).await)
        }
        "copilot_poll_for_account" => {
            let device_code = req_str(&args, "deviceCode")?;
            let github_domain: Option<String> = opt_str(&args, "githubDomain");
            json_ok(crate::commands::copilot_poll_for_account(device_code, github_domain, copilot_state()).await)
        }
        "copilot_list_accounts" => {
            json_ok(crate::commands::copilot_list_accounts(copilot_state()).await)
        }
        "copilot_remove_account" => {
            let account_id = req_str(&args, "accountId")?;
            json_ok(crate::commands::copilot_remove_account(account_id, copilot_state()).await)
        }
        "copilot_set_default_account" => {
            let account_id = req_str(&args, "accountId")?;
            json_ok(crate::commands::copilot_set_default_account(account_id, copilot_state()).await)
        }
        "copilot_get_auth_status" => {
            json_ok(crate::commands::copilot_get_auth_status(copilot_state()).await)
        }
        "copilot_is_authenticated" => {
            json_ok(crate::commands::copilot_is_authenticated(copilot_state()).await)
        }
        "copilot_logout" => {
            json_ok(crate::commands::copilot_logout(copilot_state()).await)
        }
        "copilot_get_token" => {
            json_ok(crate::commands::copilot_get_token(copilot_state()).await)
        }
        "copilot_get_token_for_account" => {
            let account_id = req_str(&args, "accountId")?;
            json_ok(crate::commands::copilot_get_token_for_account(account_id, copilot_state()).await)
        }
        "copilot_get_models" => {
            json_ok(crate::commands::copilot_get_models(copilot_state()).await)
        }
        "copilot_get_models_for_account" => {
            let account_id = req_str(&args, "accountId")?;
            json_ok(crate::commands::copilot_get_models_for_account(account_id, copilot_state()).await)
        }
        "copilot_get_usage" => {
            json_ok(crate::commands::copilot_get_usage(copilot_state()).await)
        }
        "copilot_get_usage_for_account" => {
            let account_id = req_str(&args, "accountId")?;
            json_ok(crate::commands::copilot_get_usage_for_account(account_id, copilot_state()).await)
        }

        // ====================================================================
        // OMO 命令
        // ====================================================================
        "read_omo_local_file" => {
            json_ok(crate::commands::read_omo_local_file().await)
        }
        "get_current_omo_provider_id" => {
            json_ok(crate::commands::get_current_omo_provider_id(state).await)
        }
        "disable_current_omo" => {
            json_ok(crate::commands::disable_current_omo(state).await)
        }
        "read_omo_slim_local_file" => {
            json_ok(crate::commands::read_omo_slim_local_file().await)
        }
        "get_current_omo_slim_provider_id" => {
            json_ok(crate::commands::get_current_omo_slim_provider_id(state).await)
        }
        "disable_current_omo_slim" => {
            json_ok(crate::commands::disable_current_omo_slim(state).await)
        }

        // ====================================================================
        // Workspace 命令
        // ====================================================================
        "read_workspace_file" => {
            let filename = req_str(&args, "filename")?;
            json_ok(crate::commands::read_workspace_file(filename).await)
        }
        "write_workspace_file" => {
            let filename = req_str(&args, "filename")?;
            let content = req_str(&args, "content")?;
            json_ok(crate::commands::write_workspace_file(filename, content).await)
        }
        "list_daily_memory_files" => {
            json_ok(crate::commands::list_daily_memory_files().await)
        }
        "read_daily_memory_file" => {
            let filename = req_str(&args, "filename")?;
            json_ok(crate::commands::read_daily_memory_file(filename).await)
        }
        "write_daily_memory_file" => {
            let filename = req_str(&args, "filename")?;
            let content = req_str(&args, "content")?;
            json_ok(crate::commands::write_daily_memory_file(filename, content).await)
        }
        "delete_daily_memory_file" => {
            let filename = req_str(&args, "filename")?;
            json_ok(crate::commands::delete_daily_memory_file(filename).await)
        }
        "search_daily_memory_files" => {
            let query = req_str(&args, "query")?;
            json_ok(crate::commands::search_daily_memory_files(query).await)
        }
        "open_workspace_directory" => {
            let subdir = req_str(&args, "subdir")?;
            json_ok(crate::commands::open_workspace_directory(subdir).await)
        }

        // ====================================================================
        // Lightweight 命令 (stubs)
        // ====================================================================
        "enter_lightweight_mode" => {
            json_ok(crate::commands::enter_lightweight_mode())
        }
        "exit_lightweight_mode" => {
            json_ok(crate::commands::exit_lightweight_mode())
        }
        "is_lightweight_mode" => {
            json_val(crate::commands::is_lightweight_mode())
        }

        // ====================================================================
        // WebDAV 命令
        // ====================================================================
        "webdav_test_connection" => {
            let settings = req(&args, "settings")?;
            let preserve_empty_password: Option<bool> = opt(&args, "preserveEmptyPassword");
            json_ok(crate::commands::webdav_test_connection(settings, preserve_empty_password).await)
        }
        "webdav_sync_upload" => {
            json_ok(crate::commands::webdav_sync_upload(state).await)
        }
        "webdav_sync_download" => {
            json_ok(crate::commands::webdav_sync_download(state).await)
        }
        "webdav_sync_save_settings" => {
            let settings = req(&args, "settings")?;
            let password_touched: Option<bool> = opt(&args, "passwordTouched");
            json_ok(crate::commands::webdav_sync_save_settings(settings, password_touched).await)
        }
        "webdav_sync_fetch_remote_info" => {
            json_ok(crate::commands::webdav_sync_fetch_remote_info().await)
        }

        // ====================================================================
        // Import/Export 命令
        // ====================================================================
        "export_config_to_file" => {
            let file_path = req_str(&args, "filePath")?;
            json_ok(crate::commands::export_config_to_file(file_path, state).await)
        }
        "import_config_from_file" => {
            let file_path = req_str(&args, "filePath")?;
            json_ok(crate::commands::import_config_from_file(file_path, state).await)
        }
        "sync_current_providers_live" => {
            json_ok(crate::commands::sync_current_providers_live(state).await)
        }
        "save_file_dialog" => {
            let default_name = req_str(&args, "defaultName")?;
            json_ok(crate::commands::save_file_dialog(default_name).await)
        }
        "open_file_dialog" => {
            json_ok(crate::commands::open_file_dialog().await)
        }
        "open_zip_file_dialog" => {
            json_ok(crate::commands::open_zip_file_dialog().await)
        }
        "create_db_backup" => {
            json_ok(crate::commands::create_db_backup(state).await)
        }
        "list_db_backups" => {
            json_ok(crate::commands::list_db_backups())
        }
        "restore_db_backup" => {
            let filename = req_str(&args, "filename")?;
            json_ok(crate::commands::restore_db_backup(state, filename).await)
        }
        "rename_db_backup" => {
            let old_filename = req_str(&args, "oldFilename")?;
            let new_name = req_str(&args, "newName")?;
            json_ok(crate::commands::rename_db_backup(old_filename, new_name))
        }
        "delete_db_backup" => {
            let filename = req_str(&args, "filename")?;
            json_ok(crate::commands::delete_db_backup(filename))
        }

        // ====================================================================
        // Deep Link 命令
        // ====================================================================
        "parse_deeplink" => {
            let url = req_str(&args, "url")?;
            json_ok(crate::commands::parse_deeplink(url))
        }
        "merge_deeplink_config" => {
            let request = req(&args, "request")?;
            json_ok(crate::commands::merge_deeplink_config(request))
        }
        "import_from_deeplink" => {
            let request = req(&args, "request")?;
            json_ok(crate::commands::import_from_deeplink(state, request))
        }
        "import_from_deeplink_unified" => {
            let request = req(&args, "request")?;
            json_ok(crate::commands::import_from_deeplink_unified(state, request).await)
        }

        // ====================================================================
        // Env 命令
        // ====================================================================
        "check_env_conflicts" => {
            let app = req_str(&args, "app")?;
            json_ok(crate::commands::check_env_conflicts(app))
        }
        "delete_env_vars" => {
            let conflicts = req(&args, "conflicts")?;
            json_ok(crate::commands::delete_env_vars(conflicts))
        }
        "restore_env_backup" => {
            let backup_path = req_str(&args, "backupPath")?;
            json_ok(crate::commands::restore_env_backup(backup_path))
        }

        // ====================================================================
        // Skills (unified) 命令
        // ====================================================================
        "get_installed_skills" => {
            json_ok(crate::commands::get_installed_skills(state))
        }
        "get_skill_backups" => {
            json_ok(crate::commands::get_skill_backups())
        }
        "delete_skill_backup" => {
            let backup_id = req_str(&args, "backupId")?;
            json_ok(crate::commands::delete_skill_backup(backup_id))
        }
        "install_skill_unified" => {
            let skill = req(&args, "skill")?;
            let current_app = req_str(&args, "currentApp")?;
            json_ok(crate::commands::install_skill_unified(skill, current_app, skill_service(), state).await)
        }
        "uninstall_skill_unified" => {
            let id = req_str(&args, "id")?;
            json_ok(crate::commands::uninstall_skill_unified(id, state))
        }
        "restore_skill_backup" => {
            let backup_id = req_str(&args, "backupId")?;
            let current_app = req_str(&args, "currentApp")?;
            json_ok(crate::commands::restore_skill_backup(backup_id, current_app, state))
        }
        "toggle_skill_app" => {
            let id = req_str(&args, "id")?;
            let app = req_str(&args, "app")?;
            let enabled = req_bool(&args, "enabled")?;
            json_ok(crate::commands::toggle_skill_app(id, app, enabled, state))
        }
        "scan_unmanaged_skills" => {
            json_ok(crate::commands::scan_unmanaged_skills(state))
        }
        "import_skills_from_apps" => {
            let imports = req(&args, "imports")?;
            json_ok(crate::commands::import_skills_from_apps(imports, state))
        }
        "discover_available_skills" => {
            json_ok(crate::commands::discover_available_skills(skill_service(), state).await)
        }
        "check_skill_updates" => {
            json_ok(crate::commands::check_skill_updates(skill_service(), state).await)
        }
        "update_skill" => {
            let id = req_str(&args, "id")?;
            json_ok(crate::commands::update_skill(id, skill_service(), state).await)
        }
        "migrate_skill_storage" => {
            let location = req(&args, "location")?;
            json_ok(crate::commands::migrate_skill_storage(location, state).await)
        }
        "search_skills_sh" => {
            let query = req_str(&args, "query")?;
            let limit = req_usize(&args, "limit")?;
            let offset = req_usize(&args, "offset")?;
            json_ok(crate::commands::search_skills_sh(query, limit, offset).await)
        }

        // ====================================================================
        // Skills (legacy) 命令
        // ====================================================================
        "get_skills" => {
            json_ok(crate::commands::get_skills(skill_service(), state).await)
        }
        "get_skills_for_app" => {
            let app = req_str(&args, "app")?;
            json_ok(crate::commands::get_skills_for_app(app, skill_service(), state).await)
        }
        "install_skill" => {
            let directory = req_str(&args, "directory")?;
            json_ok(crate::commands::install_skill(directory, skill_service(), state).await)
        }
        "install_skill_for_app" => {
            let app = req_str(&args, "app")?;
            let directory = req_str(&args, "directory")?;
            json_ok(crate::commands::install_skill_for_app(app, directory, skill_service(), state).await)
        }
        "uninstall_skill" => {
            let directory = req_str(&args, "directory")?;
            json_ok(crate::commands::uninstall_skill(directory, state))
        }
        "uninstall_skill_for_app" => {
            let app = req_str(&args, "app")?;
            let directory = req_str(&args, "directory")?;
            json_ok(crate::commands::uninstall_skill_for_app(app, directory, state))
        }
        "get_skill_repos" => {
            json_ok(crate::commands::get_skill_repos(state))
        }
        "add_skill_repo" => {
            let repo = req(&args, "repo")?;
            json_ok(crate::commands::add_skill_repo(repo, state))
        }
        "remove_skill_repo" => {
            let owner = req_str(&args, "owner")?;
            let name = req_str(&args, "name")?;
            json_ok(crate::commands::remove_skill_repo(owner, name, state))
        }
        "install_skills_from_zip" => {
            let file_path = req_str(&args, "filePath")?;
            let current_app = req_str(&args, "currentApp")?;
            json_ok(crate::commands::install_skills_from_zip(file_path, current_app, state))
        }

        // ====================================================================
        // Auto Launch 命令
        // ====================================================================
        "set_auto_launch" => {
            let enabled = req_bool(&args, "enabled")?;
            json_ok(crate::commands::set_auto_launch(enabled).await)
        }
        "get_auto_launch_status" => {
            json_ok(crate::commands::get_auto_launch_status().await)
        }

        // ====================================================================
        // Tray 命令 (server mode stub)
        // ====================================================================
        "update_tray_menu" => {
            // Tray menu is not applicable in server mode
            json_ok::<bool>(Ok::<bool, String>(true))
        }

        // ====================================================================
        // 未知命令
        // ====================================================================
        _ => {
            (
                axum::http::StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": format!("Unknown command: {}", command)})),
            ).into_response()
        }
    })
}
