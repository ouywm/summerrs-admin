/// SSE 流式中继
///
/// 接收后端 SSE → ProviderAdapter::parse_stream → 累积 usage → 转发客户端
/// 流结束时触发 post_consume 后结算
pub struct StreamRelay;
