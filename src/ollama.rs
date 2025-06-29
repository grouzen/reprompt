use ollama_rs::{Ollama, generation::completion::request::GenerationRequest, models::LocalModel};
use tokio_stream::StreamExt;

const DEFAULT_OLLAMA_MODEL: &str = "qwen2.5:7b";

#[derive(Clone)]
pub struct OllamaClient {
    ollama: Ollama,
}

impl OllamaClient {
    pub fn new(ollama: Ollama) -> Self {
        Self { ollama }
    }

    pub async fn generate_completion(
        &self,
        prompt: String,
        on_next: impl Fn(String),
    ) -> anyhow::Result<String> {
        let mut stream = self
            .ollama
            .generate_stream(GenerationRequest::new(DEFAULT_OLLAMA_MODEL.into(), prompt))
            .await?;
        let mut response = String::new();

        while let Some(Ok(next)) = stream.next().await {
            for n in next {
                response += &n.response;
                on_next(response.clone());
            }
        }

        Ok(response)
    }

    pub async fn list_models(&self) -> anyhow::Result<Vec<LocalModel>> {
        self.ollama
            .list_local_models()
            .await
            .map_err(anyhow::Error::new)
    }
}
