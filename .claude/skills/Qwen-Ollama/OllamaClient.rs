// Complete Ollama client implementation from Marvinous project
// Location: /home/matt/Marvinous/src/llm/client.rs

use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum OllamaError {
    #[error("HTTP request failed: {0}")]
    RequestError(#[from] reqwest::Error),
    #[error("Generation failed: {0}")]
    GenerationError(String),
}

#[derive(Serialize)]
struct GenerateRequest {
    model: String,
    prompt: String,
    stream: bool,
}

#[derive(Deserialize)]
struct GenerateResponse {
    #[allow(dead_code)]
    model: Option<String>,
    response: String,
    done: bool,
    #[allow(dead_code)]
    context: Option<Vec<i64>>,
}

pub struct OllamaClient {
    client: Client,
    endpoint: String,
    model: String,
}

impl OllamaClient {
    /// Create a new Ollama client
    ///
    /// # Arguments
    /// * `endpoint` - Ollama API endpoint (e.g., "http://localhost:11434")
    /// * `model` - Model name (e.g., "qwen2.5:7b")
    /// * `timeout_secs` - Request timeout in seconds (recommended: 120-300)
    pub fn new(endpoint: &str, model: &str, timeout_secs: u64) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(timeout_secs))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            client,
            endpoint: endpoint.to_string(),
            model: model.to_string(),
        }
    }

    /// Generate a completion from a prompt
    ///
    /// # Arguments
    /// * `prompt` - The text prompt to send to the model
    ///
    /// # Returns
    /// The generated text response
    ///
    /// # Errors
    /// Returns error if network fails or response parsing fails
    pub async fn generate(&self, prompt: &str) -> Result<String, OllamaError> {
        let request = GenerateRequest {
            model: self.model.clone(),
            prompt: prompt.to_string(),
            stream: false,
        };

        tracing::info!("Sending prompt to Ollama ({} chars)", prompt.len());

        let response = self
            .client
            .post(format!("{}/api/generate", self.endpoint))
            .json(&request)
            .send()
            .await
            .map_err(|e| OllamaError::GenerationError(format!("HTTP request failed: {}", e)))?;

        let result = response
            .json::<GenerateResponse>()
            .await
            .map_err(|e| OllamaError::GenerationError(format!("Failed to parse response: {}", e)))?;

        if !result.done {
            return Err(OllamaError::GenerationError(
                "Incomplete response from Ollama".to_string(),
            ));
        }

        tracing::info!("Response received ({} chars)", result.response.len());

        Ok(result.response)
    }
}

// Example usage in async main:
//
// #[tokio::main]
// async fn main() -> Result<(), Box<dyn std::error::Error>> {
//     let client = OllamaClient::new(
//         "http://localhost:11434",
//         "qwen2.5:7b",
//         120
//     );
//
//     let prompt = "Analyze this server data and identify issues...";
//     let response = client.generate(prompt).await?;
//
//     println!("LLM Response:\n{}", response);
//     Ok(())
// }
