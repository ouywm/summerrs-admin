//! 预扣估算器：从入站 [`ChatRequest`] 推算一个"够花"的 quota 上界。
//!
//! # 为什么需要估算
//!
//! [`crate::BillingService::reserve`] 要在调上游之前就写 `used_quota += estimated`。而
//! 真实 token 数要上游响应才知道。估算的目标是：**宁可多估不少估**——少估会让真实
//! 消耗超出预扣时 `settle` 阶段补扣到负余额。
//!
//! # 启发式
//!
//! - `prompt_tokens ≈ char_count / 4`（英文 4 字符 ≈ 1 token，中文 1.5 字符 ≈ 1 token
//!   但 token 本身有差别；统一按 4 字符/token 对英文精确、对中文**偏低估**，由
//!   completion 上界吸收）
//! - `completion_tokens = max_completion_tokens.or(max_tokens).unwrap_or(`
//!   [`DEFAULT_MAX_COMPLETION`]`)`——客户端不设上限时给默认 4096
//! - 图片 part 按 [`IMAGE_PART_TOKENS`] 估；音频 part 按 [`AUDIO_PART_TOKENS`] 估
//!   （OpenAI 官方文档里图片一张约 85-170 tokens；这里取保守上界 200）
//!
//! # 边界
//!
//! - `cache` 和 `reasoning` token 在入站 request 里都不可知，估算时全按 `input` /
//!   `output` 原价计——折价仅在 settle 时真实 usage 触发。这会**高估** quota，即预扣
//!   偏多，对平台安全。

use summer_ai_core::{
    ChatRequest, CompletionTokensDetails, ContentPart, MessageContent, PromptTokensDetails, Usage,
};

use crate::price::{CostBreakdown, PriceTable, compute_cost};

/// 客户端未设 `max_tokens` / `max_completion_tokens` 时的默认 completion 上界。
pub const DEFAULT_MAX_COMPLETION: i64 = 4096;

/// 一张 image part 的估算 token 数（保守上界）。
pub const IMAGE_PART_TOKENS: i64 = 200;

/// 一段 audio part 的估算 token 数（保守上界）。
pub const AUDIO_PART_TOKENS: i64 = 500;

/// 字符数 → token 数的换算比。
const CHARS_PER_TOKEN: f64 = 4.0;

/// 按 [`PriceTable`] 估算一次 [`ChatRequest`] 的预扣 quota。
///
/// 返回的 quota 已向上取整（复用 [`compute_cost`] 的 ceil 语义），直接传给
/// [`crate::BillingService::reserve`] 做 `estimated_quota` 参数。
pub fn estimate_quota(req: &ChatRequest, price: &PriceTable) -> i64 {
    let usage = estimate_usage(req);
    let CostBreakdown { quota, .. } = compute_cost(&usage, price, "");
    quota
}

/// 估算一次 [`ChatRequest`] 的近似 [`Usage`]（供上层复用，如提前做风控判断）。
pub fn estimate_usage(req: &ChatRequest) -> Usage {
    let prompt_tokens = estimate_prompt_tokens(req).max(1);
    let completion_tokens = estimate_completion_tokens(req).max(1);
    Usage {
        prompt_tokens,
        completion_tokens,
        total_tokens: prompt_tokens + completion_tokens,
        prompt_tokens_details: Some(PromptTokensDetails {
            cached_tokens: None,
            cache_creation_tokens: None,
            audio_tokens: None,
        }),
        completion_tokens_details: Some(CompletionTokensDetails {
            reasoning_tokens: None,
            audio_tokens: None,
            accepted_prediction_tokens: None,
            rejected_prediction_tokens: None,
        }),
    }
}

fn estimate_prompt_tokens(req: &ChatRequest) -> i64 {
    let mut tokens: i64 = 0;
    for msg in &req.messages {
        if let Some(content) = &msg.content {
            tokens += content_tokens(content);
        }
        if let Some(name) = &msg.name {
            tokens += chars_to_tokens(name.chars().count());
        }
    }
    tokens
}

fn content_tokens(content: &MessageContent) -> i64 {
    match content {
        MessageContent::Text(s) => chars_to_tokens(s.chars().count()),
        MessageContent::Parts(parts) => parts.iter().map(part_tokens).sum(),
    }
}

fn part_tokens(part: &ContentPart) -> i64 {
    match part {
        ContentPart::Text { text } => chars_to_tokens(text.chars().count()),
        ContentPart::ImageUrl { .. } => IMAGE_PART_TOKENS,
        ContentPart::InputAudio { .. } => AUDIO_PART_TOKENS,
    }
}

fn chars_to_tokens(chars: usize) -> i64 {
    // ceil(chars / CHARS_PER_TOKEN)，保守偏大
    ((chars as f64) / CHARS_PER_TOKEN).ceil() as i64
}

fn estimate_completion_tokens(req: &ChatRequest) -> i64 {
    req.max_completion_tokens
        .or(req.max_tokens)
        .unwrap_or(DEFAULT_MAX_COMPLETION)
        .max(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use summer_ai_core::{ChatMessage, Role};

    fn user_msg(text: &str) -> ChatMessage {
        ChatMessage {
            role: Role::User,
            content: Some(MessageContent::text(text)),
            reasoning_content: None,
            refusal: None,
            name: None,
            tool_calls: None,
            tool_call_id: None,
            audio: None,
            options: None,
        }
    }

    fn req_with(messages: Vec<ChatMessage>, max_tokens: Option<i64>) -> ChatRequest {
        ChatRequest {
            model: "gpt-4o-mini".into(),
            messages,
            stream: false,
            max_tokens,
            ..Default::default()
        }
    }

    #[test]
    fn prompt_tokens_ceil_on_short_text() {
        // "hello" = 5 chars → ceil(5/4) = 2 tokens
        let req = req_with(vec![user_msg("hello")], None);
        assert_eq!(estimate_prompt_tokens(&req), 2);
    }

    #[test]
    fn prompt_tokens_sum_over_messages() {
        // "abcd" = 1 token; "efgh" = 1 token → total 2
        let req = req_with(vec![user_msg("abcd"), user_msg("efgh")], None);
        assert_eq!(estimate_prompt_tokens(&req), 2);
    }

    #[test]
    fn completion_uses_max_completion_then_max_tokens_then_default() {
        let mut req = req_with(vec![user_msg("hi")], None);
        assert_eq!(estimate_completion_tokens(&req), DEFAULT_MAX_COMPLETION);

        req.max_tokens = Some(100);
        assert_eq!(estimate_completion_tokens(&req), 100);

        req.max_completion_tokens = Some(50);
        assert_eq!(estimate_completion_tokens(&req), 50);
    }

    #[test]
    fn parts_contribute_text_and_image_tokens() {
        let msg = ChatMessage {
            role: Role::User,
            content: Some(MessageContent::Parts(vec![
                ContentPart::Text {
                    text: "abcd".into(),
                },
                ContentPart::ImageUrl {
                    image_url: summer_ai_core::ImageUrl {
                        url: "https://x".into(),
                        detail: None,
                    },
                },
            ])),
            reasoning_content: None,
            refusal: None,
            name: None,
            tool_calls: None,
            tool_call_id: None,
            audio: None,
            options: None,
        };
        let req = req_with(vec![msg], Some(100));
        // 4 chars / 4 = 1 token + 200 image = 201
        assert_eq!(estimate_prompt_tokens(&req), 1 + IMAGE_PART_TOKENS);
    }

    #[test]
    fn empty_prompt_still_estimates_at_least_one_token() {
        let req = req_with(vec![], Some(100));
        let usage = estimate_usage(&req);
        assert_eq!(usage.prompt_tokens, 1);
        assert_eq!(usage.completion_tokens, 100);
    }

    #[test]
    fn estimate_quota_routes_through_compute_cost() {
        // 1 char prompt + max_tokens=1_000_000 = 1_000_001 tokens
        // prompt ≈ ceil(1/4) = 1 token; output = 1M
        // price: input 0.15, output 0.60 per M → cost = 0.15 × 1/1M + 0.60 × 1M/1M
        //     ≈ 0.00000015 + 0.60 = ~0.60 USD → × 500_000 = 300_000.08 → ceil = 300_001
        let price = PriceTable::for_test("0.15", "0.60");
        let req = req_with(vec![user_msg("a")], Some(1_000_000));
        let quota = estimate_quota(&req, &price);
        assert_eq!(quota, 300_001);
    }
}
