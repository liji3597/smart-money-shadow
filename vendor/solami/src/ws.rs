use solana_client::nonblocking::pubsub_client::PubsubClient;

use crate::error::{Result, SolamiError};

pub struct WsClient {
    url: String,
    inner: PubsubClient,
}

impl WsClient {
    pub(crate) async fn connect(token: &str, base: &str) -> Result<Self> {
        let url = format!("{base}?api_key={token}");
        let inner = PubsubClient::new(&url)
            .await
            .map_err(|e| SolamiError::Ws(e.to_string()))?;

        Ok(Self { url, inner })
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    pub fn inner(&self) -> &PubsubClient {
        &self.inner
    }

    pub async fn shutdown(self) -> Result<()> {
        self.inner
            .shutdown()
            .await
            .map_err(|e| SolamiError::Ws(e.to_string()))
    }
}

impl std::ops::Deref for WsClient {
    type Target = PubsubClient;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use futures::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message;

    pub(crate) async fn spawn_ws_server() -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                tokio::spawn(async move {
                    let mut ws = match tokio_tungstenite::accept_async(stream).await {
                        Ok(ws) => ws,
                        Err(_) => return,
                    };
                    while let Some(msg) = ws.next().await {
                        match msg {
                            Ok(Message::Close(_)) | Err(_) => break,
                            Ok(_) => {
                                let _ = ws.send(Message::Text("{}".into())).await;
                            }
                        }
                    }
                });
            }
        });
        format!("ws://{addr}/ws")
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn connect_url_inner_shutdown() {
        let base = spawn_ws_server().await;
        let client = WsClient::connect("tok", &base).await.unwrap();
        assert_eq!(client.url(), format!("{base}?api_key=tok"));
        let _inner: &PubsubClient = client.inner();
        let _derefed: &PubsubClient = &client;
        client.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn connect_fails_on_dead_port() {
        let r = WsClient::connect("tok", "ws://127.0.0.1:1").await;
        assert!(matches!(r, Err(SolamiError::Ws(_))));
    }
}
