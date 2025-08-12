use ollama_rs::{Ollama, generation::completion::request::GenerationRequest, models::LocalModel};
use tokio::sync::broadcast;
use tokio_stream::StreamExt;
use async_trait::async_trait;

#[async_trait]
pub trait OllamaApi: Send + Sync {
    async fn generate_stream(
        &self,
        req: GenerationRequest,
    ) -> anyhow::Result<Box<dyn tokio_stream::Stream<Item = anyhow::Result<Vec<OllamaCompletionChunk>>> + Send + Unpin>>;

    async fn list_local_models(&self) -> anyhow::Result<Vec<LocalModel>>;
}

// Real implementation for production use
#[derive(Clone)]
pub struct OllamaRsImpl(pub Ollama);

#[async_trait]
impl OllamaApi for OllamaRsImpl {
    async fn generate_stream(
        &self,
        req: GenerationRequest,
    ) -> anyhow::Result<Box<dyn tokio_stream::Stream<Item = anyhow::Result<Vec<OllamaCompletionChunk>>> + Send + Unpin>> {
        let s = self.0.generate_stream(req).await?;
        // Map the ollama_rs CompletionChunk to OllamaCompletionChunk used by client
        let mapped_stream = s.map(|res| {
            res.map(|chunks| {
                chunks.into_iter()
                    .map(|c| OllamaCompletionChunk { response: c.response })
                    .collect()
            })
        });
        Ok(Box::new(mapped_stream))
    }

    async fn list_local_models(&self) -> anyhow::Result<Vec<LocalModel>> {
        self.0.list_local_models().await
    }
}

pub struct OllamaClient<T: OllamaApi + Clone + Send + Sync + 'static> {
    ollama: T,
    cancel_tx: broadcast::Sender<()>,
}

// Helper type for mock chunk construction
pub struct OllamaCompletionChunk {
    pub response: String,
}


impl<T: OllamaApi + Clone + Send + Sync + 'static> OllamaClient<T> {
    pub fn new(ollama: T) -> Self {
        let (cancel_tx, _) = broadcast::channel(1);
        Self { ollama, cancel_tx }
    }

    pub fn get_cancel_receiver(&self) -> broadcast::Receiver<()> {
        self.cancel_tx.subscribe()
    }

    pub fn cancel_generation(&self) {
        let _ = self.cancel_tx.send(());
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
        let mut cancel_rx = self.get_cancel_receiver();

        loop {
            tokio::select! {
                maybe_next = stream.next() => {
                    match maybe_next {
                        Some(Ok(next)) => {
                            for n in next {
                                response += &n.response;
                                on_next(response.clone());
                            }
                        }
                        None => break,
                        Some(Err(e)) => return Err(e.into()),
                    }
                }
                _ = cancel_rx.recv() => {
                    break;
                }
            }
        }

        Ok(response)
    }

    pub async fn list_models(&self) -> anyhow::Result<Vec<LocalModel>> {
        self.ollama
            .list_local_models()
            .await
            .map_err(|e| anyhow::Error::msg(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::pin::Pin;
    use futures::stream;
    use tokio_stream::Stream;

    // Mock implementation of OllamaApi for unit testing
    #[derive(Clone)]
    struct MockOllama;

    #[async_trait]
    impl OllamaApi for MockOllama {
        async fn generate_stream(
            &self,
            _req: GenerationRequest,
        ) -> anyhow::Result<Box<dyn Stream<Item = anyhow::Result<Vec<OllamaCompletionChunk>>> + Send + Unpin>> {
            let chunk1 = OllamaCompletionChunk { response: "Hello".to_string() };
            let chunk2 = OllamaCompletionChunk { response: ", world!".to_string() };
            let chunks = vec![
                Ok(vec![chunk1]),
                Ok(vec![chunk2]),
                None // End of stream
            ];
            // Stream of two msgs then done
            let s = stream::iter(chunks.into_iter().take(2).map(|c| c));
            Ok(Box::new(s))
        }

        async fn list_local_models(&self) -> anyhow::Result<Vec<LocalModel>> {
            let model = LocalModel { name: "test-model".to_string(), size: 42, modified_at: Default::default() };
            Ok(vec![model])
        }
    }

    #[tokio::test]
    async fn test_ollama_client_creation() {
        let ollama = MockOllama {};
        let _client = OllamaClient::new(ollama.clone());
        // Should construct
    }

    #[tokio::test]
    async fn test_cancel_generation() {
        let ollama = MockOllama {};
        let client = OllamaClient::new(ollama.clone());
        client.cancel_generation();
    }

    #[tokio::test]
    async fn test_generate_completion_with_mock() {
        let ollama = MockOllama {};
        let client = OllamaClient::new(ollama.clone());
        let model = LocalModel { name: "mock".to_string(), size: 1, modified_at: Default::default() };
        let mut observed = vec![];
        let result = client
            .generate_completion("hi".to_string(), &model, |v| observed.push(v))
            .await
            .unwrap();
        assert_eq!(result, "Hello, world!");
        // The observed sequence should show intermediate completions
        assert_eq!(observed, vec!["Hello".to_string(), "Hello, world!".to_string()]);
    }

    #[tokio::test]
    async fn test_list_models_mock() {
        let ollama = MockOllama {};
        let client = OllamaClient::new(ollama.clone());
        let models = client.list_models().await.unwrap();
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].name, "test-model");
    }
}
