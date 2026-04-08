#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(i16)]
pub enum ProviderKind {
    OpenAi = 1,
    Anthropic = 3,
    AzureOpenAi = 14,
    Baidu = 15,
    Ali = 17,
    Gemini = 24,
    Ollama = 28,
    DeepSeek = 30,
    Groq = 31,
    Mistral = 32,
    SiliconFlow = 33,
    Vllm = 34,
    Fireworks = 35,
    Together = 36,
    OpenRouter = 37,
    Moonshot = 38,
    Lingyi = 39,
    Cohere = 40,
}

impl ProviderKind {
    pub fn from_channel_type(channel_type: i16) -> Option<Self> {
        match channel_type {
            1 => Some(Self::OpenAi),
            3 => Some(Self::Anthropic),
            14 => Some(Self::AzureOpenAi),
            15 => Some(Self::Baidu),
            17 => Some(Self::Ali),
            24 => Some(Self::Gemini),
            28 => Some(Self::Ollama),
            30 => Some(Self::DeepSeek),
            31 => Some(Self::Groq),
            32 => Some(Self::Mistral),
            33 => Some(Self::SiliconFlow),
            34 => Some(Self::Vllm),
            35 => Some(Self::Fireworks),
            36 => Some(Self::Together),
            37 => Some(Self::OpenRouter),
            38 => Some(Self::Moonshot),
            39 => Some(Self::Lingyi),
            40 => Some(Self::Cohere),
            _ => None,
        }
    }

    pub const fn channel_type(self) -> i16 {
        self as i16
    }

    pub fn display_name(self) -> &'static str {
        crate::provider::ProviderRegistry::meta(self).name
    }

    pub fn default_base_url(self) -> &'static str {
        crate::provider::ProviderRegistry::meta(self).default_base_url
    }

    pub fn is_openai_compatible(self) -> bool {
        crate::provider::ProviderRegistry::meta(self).openai_compatible
    }
}
