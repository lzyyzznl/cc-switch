use std::str::FromStr;
use std::sync::Arc;

use crate::app_config::AppType;
use crate::services::subscription::{CredentialStatus, SubscriptionQuota};
use crate::store::AppState;

/// 查询官方订阅额度
///
/// 读取 CLI 工具已有的 OAuth 凭据并调用官方 API 获取使用额度。
/// 结果（无论业务失败还是 transport 层 Err）都会写入 `UsageCache`、通知托盘
/// 刷新，并 emit `usage-cache-updated`，让前端 React Query 与托盘共享同一份
/// 最新数据。失败快照写入后 `format_subscription_summary` 会通过 `success=false`
/// 守卫返回 `None`，托盘 suffix 自然消失，避免长期滞留旧配额数字。
/// Err 原样向前端返回，React Query 的 onError 不会被吞掉。
pub async fn get_subscription_quota(
    state: Arc<AppState>,
    tool: String,
) -> Result<SubscriptionQuota, String> {
    let inner = crate::services::subscription::get_subscription_quota(&tool).await;
    let snapshot = match &inner {
        Ok(q) => q.clone(),
        // transport 层 Err —— 凭据状态不明，用 Valid 表达"凭据没问题，是通信/parse 出错"。
        Err(err_msg) => SubscriptionQuota::error(&tool, CredentialStatus::Valid, err_msg.clone()),
    };
    if let Ok(app_type) = AppType::from_str(&tool) {
        state.usage_cache.put_subscription(app_type, snapshot);
    }
    inner
}
