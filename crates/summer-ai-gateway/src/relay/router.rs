/// 渠道路由引擎
///
/// 算法：
/// 1. 查 ai_ability 表获取 (group, model) 的候选渠道
/// 2. 过滤掉已禁用的渠道
/// 3. 按 priority 降序分组
/// 4. 最高优先级组内按 weight 加权随机选择
/// 5. 失败后排除该渠道，重试或降级到次优先级
pub struct ChannelRouter;

impl ChannelRouter {
    pub async fn select_channel(&self, _group: &str, _model: &str, _exclude: &[i64]) -> Option<()> {
        // TODO: 返回 ChannelWithMapping
        todo!()
    }
}
