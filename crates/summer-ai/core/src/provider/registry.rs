use super::{
    AnthropicAdapter, AzureOpenAiAdapter, ChatProvider, EmbeddingProvider, GeminiAdapter,
    OpenAiAdapter, Provider, ProviderKind, ResponsesProvider,
};

const CHAT_ONLY_SCOPES: &[&str] = &["chat"];
const CHAT_AND_RESPONSES_SCOPES: &[&str] = &["chat", "responses"];
const CHAT_AND_EMBEDDINGS_SCOPES: &[&str] = &["chat", "embeddings"];
const CHAT_RESPONSES_AND_EMBEDDINGS_SCOPES: &[&str] = &["chat", "responses", "embeddings"];
const CHAT_RESPONSES_RERANK_AND_EMBEDDINGS_SCOPES: &[&str] =
    &["chat", "responses", "rerank", "embeddings"];

#[derive(Debug, Clone)]
pub struct ProviderMeta {
    pub name: &'static str,
    pub default_base_url: &'static str,
    pub supported_scopes: &'static [&'static str],
    pub openai_compatible: bool,
}

pub struct ProviderRegistry;

impl ProviderRegistry {
    pub fn get(kind: ProviderKind) -> &'static dyn Provider {
        match kind {
            ProviderKind::Anthropic => &ANTHROPIC,
            ProviderKind::AzureOpenAi => &AZURE,
            ProviderKind::Gemini => &GEMINI,
            _ => &OPENAI,
        }
    }

    pub fn chat(kind: ProviderKind) -> Option<&'static dyn ChatProvider> {
        if supports_scope(kind, "chat") {
            Some(match kind {
                ProviderKind::Anthropic => &ANTHROPIC,
                ProviderKind::AzureOpenAi => &AZURE,
                ProviderKind::Gemini => &GEMINI,
                _ => &OPENAI,
            })
        } else {
            None
        }
    }

    pub fn embedding(kind: ProviderKind) -> Option<&'static dyn EmbeddingProvider> {
        if !supports_scope(kind, "embeddings") {
            return None;
        }

        Some(match kind {
            ProviderKind::AzureOpenAi => &AZURE,
            ProviderKind::Gemini => &GEMINI,
            _ => &OPENAI,
        })
    }

    pub fn responses(kind: ProviderKind) -> Option<&'static dyn ResponsesProvider> {
        if !supports_scope(kind, "responses") {
            return None;
        }

        Some(match kind {
            ProviderKind::Anthropic => &ANTHROPIC,
            ProviderKind::AzureOpenAi => &AZURE,
            ProviderKind::Gemini => &GEMINI,
            _ => &OPENAI,
        })
    }

    pub fn meta(kind: ProviderKind) -> &'static ProviderMeta {
        meta_by_kind(kind)
    }

    pub fn supported_scopes(kind: ProviderKind) -> &'static [&'static str] {
        meta_by_kind(kind).supported_scopes
    }

    pub fn supports_scope(kind: ProviderKind, scope: &str) -> bool {
        supports_scope(kind, scope)
    }
}

fn meta_by_kind(kind: ProviderKind) -> &'static ProviderMeta {
    match kind {
        ProviderKind::OpenAi => &ProviderMeta {
            name: "OpenAI",
            default_base_url: "https://api.openai.com",
            supported_scopes: CHAT_RESPONSES_AND_EMBEDDINGS_SCOPES,
            openai_compatible: true,
        },
        ProviderKind::Anthropic => &ProviderMeta {
            name: "Anthropic",
            default_base_url: "https://api.anthropic.com",
            supported_scopes: CHAT_AND_RESPONSES_SCOPES,
            openai_compatible: false,
        },
        ProviderKind::AzureOpenAi => &ProviderMeta {
            name: "Azure OpenAI",
            default_base_url: "",
            supported_scopes: CHAT_RESPONSES_AND_EMBEDDINGS_SCOPES,
            openai_compatible: false,
        },
        ProviderKind::Baidu => &ProviderMeta {
            name: "百度文心",
            default_base_url: "https://aip.baidubce.com",
            supported_scopes: CHAT_ONLY_SCOPES,
            openai_compatible: true,
        },
        ProviderKind::Ali => &ProviderMeta {
            name: "阿里通义",
            default_base_url: "https://dashscope.aliyuncs.com/compatible-mode",
            supported_scopes: CHAT_RESPONSES_AND_EMBEDDINGS_SCOPES,
            openai_compatible: true,
        },
        ProviderKind::Gemini => &ProviderMeta {
            name: "Google Gemini",
            default_base_url: "https://generativelanguage.googleapis.com",
            supported_scopes: CHAT_RESPONSES_AND_EMBEDDINGS_SCOPES,
            openai_compatible: false,
        },
        ProviderKind::Ollama => &ProviderMeta {
            name: "Ollama",
            default_base_url: "http://localhost:11434",
            supported_scopes: CHAT_AND_EMBEDDINGS_SCOPES,
            openai_compatible: true,
        },
        ProviderKind::DeepSeek => &ProviderMeta {
            name: "DeepSeek",
            default_base_url: "https://api.deepseek.com",
            supported_scopes: CHAT_RESPONSES_AND_EMBEDDINGS_SCOPES,
            openai_compatible: true,
        },
        ProviderKind::Groq => &ProviderMeta {
            name: "Groq",
            default_base_url: "https://api.groq.com/openai",
            supported_scopes: CHAT_ONLY_SCOPES,
            openai_compatible: true,
        },
        ProviderKind::Mistral => &ProviderMeta {
            name: "Mistral",
            default_base_url: "https://api.mistral.ai",
            supported_scopes: CHAT_RESPONSES_AND_EMBEDDINGS_SCOPES,
            openai_compatible: true,
        },
        ProviderKind::SiliconFlow => &ProviderMeta {
            name: "SiliconFlow",
            default_base_url: "https://api.siliconflow.cn",
            supported_scopes: CHAT_RESPONSES_AND_EMBEDDINGS_SCOPES,
            openai_compatible: true,
        },
        ProviderKind::Vllm => &ProviderMeta {
            name: "vLLM",
            default_base_url: "http://localhost:8000",
            supported_scopes: CHAT_AND_EMBEDDINGS_SCOPES,
            openai_compatible: true,
        },
        ProviderKind::Fireworks => &ProviderMeta {
            name: "Fireworks AI",
            default_base_url: "https://api.fireworks.ai/inference",
            supported_scopes: CHAT_AND_EMBEDDINGS_SCOPES,
            openai_compatible: true,
        },
        ProviderKind::Together => &ProviderMeta {
            name: "Together AI",
            default_base_url: "https://api.together.xyz",
            supported_scopes: CHAT_AND_EMBEDDINGS_SCOPES,
            openai_compatible: true,
        },
        ProviderKind::OpenRouter => &ProviderMeta {
            name: "OpenRouter",
            default_base_url: "https://openrouter.ai/api",
            supported_scopes: CHAT_RESPONSES_AND_EMBEDDINGS_SCOPES,
            openai_compatible: true,
        },
        ProviderKind::Moonshot => &ProviderMeta {
            name: "Moonshot",
            default_base_url: "https://api.moonshot.cn",
            supported_scopes: CHAT_ONLY_SCOPES,
            openai_compatible: true,
        },
        ProviderKind::Lingyi => &ProviderMeta {
            name: "零一万物",
            default_base_url: "https://api.lingyiwanwu.com",
            supported_scopes: CHAT_ONLY_SCOPES,
            openai_compatible: true,
        },
        ProviderKind::Cohere => &ProviderMeta {
            name: "Cohere",
            default_base_url: "https://api.cohere.com/compatibility",
            supported_scopes: CHAT_RESPONSES_RERANK_AND_EMBEDDINGS_SCOPES,
            openai_compatible: true,
        },
    }
}

fn supports_scope(kind: ProviderKind, scope: &str) -> bool {
    let supported_scopes = meta_by_kind(kind).supported_scopes;
    !supported_scopes.is_empty() && supported_scopes.contains(&scope)
}

static ANTHROPIC: AnthropicAdapter = AnthropicAdapter;
static AZURE: AzureOpenAiAdapter = AzureOpenAiAdapter;
static GEMINI: GeminiAdapter = GeminiAdapter;
static OPENAI: OpenAiAdapter = OpenAiAdapter;
