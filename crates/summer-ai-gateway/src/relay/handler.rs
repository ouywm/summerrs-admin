/// /v1/chat/completions 的核心处理流程：
///
/// 1. 验证 Token（Bearer sk-xxx）
/// 2. 解析 request.model → 确定 group
/// 3. 路由选择渠道（router::select_channel）
/// 4. 模型映射（channel.model_mapping）
/// 5. 预扣配额（billing::pre_consume）
/// 6. 选择 ProviderAdapter → build_request → 发送
/// 7. 成功：parse_response/parse_stream → 返回 → 后结算 → 写日志
/// 8. 失败：退还预扣 → 排除该渠道 → 重试下一个
pub async fn handle_chat_completion() {
    todo!()
}
