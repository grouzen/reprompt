use ollama_rs::{Ollama, generation::completion::request::GenerationRequest, models::LocalModel};
use tokio_stream::StreamExt;

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
        model: &LocalModel,
        on_next: impl Fn(String),
    ) -> anyhow::Result<String> {
        let mut stream = self
            .ollama
            .generate_stream(GenerationRequest::new(model.name.clone(), prompt))
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
