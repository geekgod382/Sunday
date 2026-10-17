use serde::Deserialize;

use crate::Provider;

#[derive(Deserialize)]
struct OpenAiResponse {
    choices: Vec<OpenAiChoice>,
}

#[derive(Deserialize)]
struct OpenAiChoice {
    message: OpenAiMessage,
}

#[derive(Deserialize)]
struct OpenAiMessage {
    content: String,
}

#[derive(Deserialize)]
struct AnthropicResponse {
    content: Vec<AnthropicBlock>,
}

#[derive(Deserialize)]
struct AnthropicBlock {
    #[serde(rename = "type")]
    block_type: String,
    text: Option<String>,
}

#[derive(Deserialize)]
struct GeminiResponse {
    candidates: Vec<GeminiCandidate>,
}

#[derive(Deserialize)]
struct GeminiCandidate {
    content: GeminiContent,
}

#[derive(Deserialize)]
struct GeminiContent {
    parts: Vec<GeminiPart>,
}

#[derive(Deserialize)]
struct GeminiPart {
    text: Option<String>,
}

pub struct Answer {
    pub text: String,
    pub links: Vec<String>,
}

pub async fn ask(
    provider: Provider,
    api_key: &str,
    model: &str,
    question: &str,
) -> Result<Answer, String> {
    let client = reqwest::Client::new();
    let system_prompt = "You are Sunday, a concise desktop assistant. Answer clearly and briefly. Include useful source URLs when relevant.";
    let response = match provider {
        Provider::OpenAi | Provider::Groq | Provider::Xai => {
            let base_url = match provider {
                Provider::OpenAi => "https://api.openai.com/v1/chat/completions",
                Provider::Groq => "https://api.groq.com/openai/v1/chat/completions",
                Provider::Xai => "https://api.x.ai/v1/chat/completions",
                _ => unreachable!(),
            };
            let response = client
                .post(base_url)
                .bearer_auth(api_key)
                .json(&serde_json::json!({
                    "model": model,
                    "messages": [
                        {"role": "system", "content": system_prompt},
                        {"role": "user", "content": question}
                    ]
                }))
                .send()
                .await
                .map_err(|error| format!("Could not reach the {provider} API: {error}"))?;
            let value = checked_json(response, provider).await?;
            serde_json::from_value::<OpenAiResponse>(value)
                .map_err(|error| format!("Unexpected {provider} response: {error}"))?
                .choices
                .into_iter()
                .next()
                .ok_or_else(|| format!("{provider} returned no answer"))?
                .message
                .content
        }
        Provider::Anthropic => {
            let response = client
                .post("https://api.anthropic.com/v1/messages")
                .header("x-api-key", api_key)
                .header("anthropic-version", "2023-06-01")
                .json(&serde_json::json!({
                    "model": model,
                    "max_tokens": 800,
                    "system": system_prompt,
                    "messages": [{"role": "user", "content": question}]
                }))
                .send()
                .await
                .map_err(|error| format!("Could not reach the Anthropic API: {error}"))?;
            let value = checked_json(response, provider).await?;
            serde_json::from_value::<AnthropicResponse>(value)
                .map_err(|error| format!("Unexpected Anthropic response: {error}"))?
                .content
                .into_iter()
                .filter(|block| block.block_type == "text")
                .filter_map(|block| block.text)
                .collect::<Vec<_>>()
                .join("\n")
        }
        Provider::Gemini => {
            let url = format!(
                "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent",
                urlencoding(model)
            );
            let response = client
                .post(url)
                .header("x-goog-api-key", api_key)
                .json(&serde_json::json!({
                    "systemInstruction": {"parts": [{"text": system_prompt}]},
                    "contents": [{"parts": [{"text": question}]}]
                }))
                .send()
                .await
                .map_err(|error| format!("Could not reach the Gemini API: {error}"))?;
            let value = checked_json(response, provider).await?;
            serde_json::from_value::<GeminiResponse>(value)
                .map_err(|error| format!("Unexpected Gemini response: {error}"))?
                .candidates
                .into_iter()
                .next()
                .ok_or_else(|| "Gemini returned no answer".to_string())?
                .content
                .parts
                .into_iter()
                .filter_map(|part| part.text)
                .collect::<Vec<_>>()
                .join("\n")
        }
    };

    if response.trim().is_empty() {
        return Err(format!("{provider} returned an empty answer"));
    }
    let links = extract_links(&response);
    Ok(Answer {
        text: response,
        links,
    })
}

async fn checked_json(
    response: reqwest::Response,
    provider: Provider,
) -> Result<serde_json::Value, String> {
    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|error| format!("Could not read the {provider} response: {error}"))?;
    if !status.is_success() {
        let detail = serde_json::from_str::<serde_json::Value>(&body)
            .ok()
            .and_then(|value| {
                value
                    .pointer("/error/message")
                    .or_else(|| value.pointer("/error"))
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned)
            })
            .unwrap_or(body);
        return Err(format!("{provider} API returned HTTP {status}: {detail}"));
    }
    serde_json::from_str(&body).map_err(|error| format!("Invalid {provider} response: {error}"))
}

fn extract_links(text: &str) -> Vec<String> {
    let mut links = Vec::new();
    for word in text.split_whitespace() {
        let candidate = word
            .trim_start_matches(['(', '[', '{', '<', '"', '\''])
            .trim_end_matches([')', ']', '}', '>', '.', ',', ';', ':', '!', '?', '"', '\'']);
        if (candidate.starts_with("https://") || candidate.starts_with("http://"))
            && !links.iter().any(|link| link == candidate)
        {
            links.push(candidate.to_string());
        }
    }
    links
}

fn urlencoding(value: &str) -> String {
    value
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                char::from(byte).to_string()
            }
            _ => format!("%{byte:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::extract_links;

    #[test]
    fn extracts_unique_http_links_and_trims_punctuation() {
        assert_eq!(
            extract_links("Read (https://example.com/page). Also https://example.com/page"),
            ["https://example.com/page"]
        );
    }

    #[test]
    fn ignores_non_http_values() {
        assert!(extract_links("mailto:user@example.com javascript:alert(1)").is_empty());
    }
}
