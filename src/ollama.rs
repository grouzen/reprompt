use ollama_rs::{Ollama, generation::completion::request::GenerationRequest, models::LocalModel};
use tokio::sync::broadcast;
use tokio_stream::StreamExt;

#[derive(Clone)]
pub struct OllamaClient {
    ollama: Ollama,
    cancel_tx: broadcast::Sender<()>,
}

impl OllamaClient {
    pub fn new(ollama: Ollama) -> Self {
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
            .map_err(anyhow::Error::new)
    }
}
