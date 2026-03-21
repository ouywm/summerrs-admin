/// 配额计算：tokens * model_ratio * group_ratio
///
/// 流程：
/// - pre_consume: 请求前按预估 prompt_tokens 预扣
/// - post_consume: 请求后按实际 usage 调整（多退少补）
pub struct BillingEngine;
