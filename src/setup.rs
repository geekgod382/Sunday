use regex::Regex;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    OpenAi,
    Anthropic,
    Gemini,
    Groq,
    Xai,
}

impl Provider {
    pub fn name(self) -> &'static str {
        match self {
            Self::OpenAi => "OpenAI",
            Self::Anthropic => "Anthropic",
            Self::Gemini => "Google Gemini",
            Self::Groq => "Groq",
            Self::Xai => "xAI",
        }
    }

    pub fn default_model(self) -> &'static str {
        match self {
            Self::OpenAi => "gpt-4o-mini",
            Self::Anthropic => "claude-3-5-haiku-latest",
            Self::Gemini => "gemini-2.0-flash",
            Self::Groq => "llama-3.3-70b-versatile",
            Self::Xai => "grok-2-latest",
        }
    }
}

impl std::fmt::Display for Provider {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.name())
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct ProviderInfo {
    pub provider: Provider,
    pub name: &'static str,
    pub default_model: &'static str,
}

pub fn detect_provider(api_key: &str) -> Result<ProviderInfo, String> {
    let patterns = [
        (r"^sk-ant-[A-Za-z0-9_-]{20,}$", Provider::Anthropic),
        (r"^AIza[A-Za-z0-9_-]{20,}$", Provider::Gemini),
        (r"^gsk_[A-Za-z0-9]{20,}$", Provider::Groq),
        (r"^xai-[A-Za-z0-9_-]{20,}$", Provider::Xai),
        (r"^sk-(?:proj-)?[A-Za-z0-9_-]{20,}$", Provider::OpenAi),
    ];

    for (pattern, provider) in patterns {
        let regex =
            Regex::new(pattern).map_err(|error| format!("Invalid provider pattern: {error}"))?;
        if regex.is_match(api_key.trim()) {
            return Ok(ProviderInfo {
                provider,
                name: provider.name(),
                default_model: provider.default_model(),
            });
        }
    }
    Err("API key format is not recognized. Supported key formats: OpenAI, Anthropic, Gemini, Groq, and xAI.".to_string())
}

#[cfg(test)]
mod tests {
    use super::{detect_provider, Provider};

    #[test]
    fn detects_supported_provider_key_formats() {
        let samples = [
            ("sk-ant-aaaaaaaaaaaaaaaaaaaa", Provider::Anthropic),
            ("AIzaaaaaaaaaaaaaaaaaaaaa", Provider::Gemini),
            ("gsk_aaaaaaaaaaaaaaaaaaaa", Provider::Groq),
            ("xai-aaaaaaaaaaaaaaaaaaaa", Provider::Xai),
            ("sk-aaaaaaaaaaaaaaaaaaaa", Provider::OpenAi),
            ("sk-proj-aaaaaaaaaaaaaaaaaaaa", Provider::OpenAi),
        ];

        for (key, expected) in samples {
            assert_eq!(detect_provider(key).unwrap().provider, expected);
        }
    }

    #[test]
    fn rejects_unrecognized_key_formats() {
        assert!(detect_provider("not-an-api-key").is_err());
        assert!(detect_provider("sk-ant-short").is_err());
    }
}
