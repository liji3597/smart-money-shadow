mod subscribe;

pub use subscribe::SubscriptionBuilder;

use std::collections::HashMap;
use std::time::Duration;

use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::transport::{Channel, ClientTlsConfig};
use tonic::{Request, Status, Streaming};

use crate::error::{Result, SolamiError};
use crate::geyser::geyser_client::GeyserClient;
use crate::geyser::{
    CommitmentLevel, SubscribeBlurRequest, SubscribeDeshredRequest, SubscribeRequest,
    SubscribeRequestFilterDeshredTransactions, SubscribeRequestFilterTransactions, SubscribeUpdate,
    SubscribeUpdateBlur, SubscribeUpdateDeshred, SubscribeUpdateWebhook, SubscribeWebhookRequest,
};

pub use crate::geyser::subscribe_update::UpdateOneof as GrpcUpdateKind;
pub use crate::geyser::subscribe_update_deshred::UpdateOneof as GrpcDeshredUpdateKind;
pub use crate::geyser::SubscribeUpdate as GrpcUpdate;
pub use crate::geyser::SubscribeUpdateDeshred as GrpcDeshredUpdate;
pub use crate::geyser::{
    SubscribeRequestFilterAccounts, SubscribeRequestFilterBlocks, SubscribeRequestFilterBlocksMeta,
    SubscribeRequestFilterDeshredTransactions as DeshredTxFilter, SubscribeRequestFilterEntry,
    SubscribeRequestFilterSlots, SubscribeRequestFilterTransactions as TxFilter,
};

const REQUEST_CHANNEL: usize = 32;

pub struct BlurStream {
    pub filters: mpsc::Sender<SubscribeBlurRequest>,
    pub updates: Streaming<SubscribeUpdateBlur>,
}

pub struct WebhookStream {
    pub filters: mpsc::Sender<SubscribeWebhookRequest>,
    pub updates: Streaming<SubscribeUpdateWebhook>,
}

pub struct GrpcClient {
    pub(crate) url: String,
    pub(crate) client: GeyserClient<Channel>,
    pub(crate) token: String,
}

fn authed<T>(token: &str, body: T) -> Result<Request<T>> {
    let mut req = Request::new(body);
    let value = token
        .parse()
        .map_err(|_| SolamiError::Grpc("api key is not a valid header value".into()))?;
    req.metadata_mut().insert("x-token", value);
    Ok(req)
}

impl GrpcClient {
    pub fn url(&self) -> &str {
        &self.url
    }

    pub async fn subscribe(
        &mut self,
        request: SubscribeRequest,
    ) -> Result<(mpsc::Sender<SubscribeRequest>, Streaming<SubscribeUpdate>)> {
        let (tx, rx) = mpsc::channel(REQUEST_CHANNEL);
        tx.send(request)
            .await
            .map_err(|_| SolamiError::Grpc("request channel closed".into()))?;
        let req = authed(&self.token, ReceiverStream::new(rx))?;
        let updates = self
            .client
            .subscribe(req)
            .await
            .map_err(|e: Status| SolamiError::Grpc(e.to_string()))?
            .into_inner();
        Ok((tx, updates))
    }

    pub async fn subscribe_transactions(
        &mut self,
        label: &str,
        accounts: Vec<String>,
        commitment: CommitmentLevel,
    ) -> Result<(mpsc::Sender<SubscribeRequest>, Streaming<SubscribeUpdate>)> {
        let mut transactions = HashMap::new();
        transactions.insert(
            label.to_owned(),
            SubscribeRequestFilterTransactions {
                vote: Some(false),
                failed: Some(false),
                account_include: accounts,
                account_exclude: vec![],
                account_required: vec![],
                signature: None,
            },
        );

        let request = SubscribeRequest {
            accounts: HashMap::default(),
            slots: HashMap::default(),
            transactions,
            transactions_status: HashMap::default(),
            blocks: HashMap::default(),
            blocks_meta: HashMap::default(),
            entry: HashMap::default(),
            commitment: Some(commitment as i32),
            accounts_data_slice: vec![],
            ping: None,
            from_slot: None,
        };

        self.subscribe(request).await
    }

    pub async fn subscribe_deshred(
        &mut self,
        request: SubscribeDeshredRequest,
    ) -> Result<(mpsc::Sender<SubscribeDeshredRequest>, Streaming<SubscribeUpdateDeshred>)> {
        let (tx, rx) = mpsc::channel(REQUEST_CHANNEL);
        tx.send(request)
            .await
            .map_err(|_| SolamiError::Grpc("request channel closed".into()))?;
        let req = authed(&self.token, ReceiverStream::new(rx))?;
        let updates = self
            .client
            .subscribe_deshred(req)
            .await
            .map_err(|e: Status| SolamiError::Grpc(e.to_string()))?
            .into_inner();
        Ok((tx, updates))
    }

    pub async fn subscribe_deshred_transactions(
        &mut self,
        label: &str,
        accounts: Vec<String>,
    ) -> Result<(mpsc::Sender<SubscribeDeshredRequest>, Streaming<SubscribeUpdateDeshred>)> {
        let mut deshred_transactions = HashMap::new();
        deshred_transactions.insert(
            label.to_owned(),
            SubscribeRequestFilterDeshredTransactions {
                vote: Some(false),
                account_include: accounts,
                account_exclude: vec![],
                account_required: vec![],
            },
        );

        let request = SubscribeDeshredRequest {
            deshred_transactions,
            ping: None,
            slots: HashMap::default(),
        };

        self.subscribe_deshred(request).await
    }

    pub async fn subscribe_blur(&mut self, request: SubscribeBlurRequest) -> Result<BlurStream> {
        let (tx, rx) = mpsc::channel(REQUEST_CHANNEL);
        tx.send(request)
            .await
            .map_err(|_| SolamiError::Grpc("request channel closed".into()))?;
        let req = authed(&self.token, ReceiverStream::new(rx))?;
        let updates = self
            .client
            .subscribe_blur(req)
            .await
            .map_err(|e: Status| SolamiError::Grpc(e.to_string()))?
            .into_inner();
        Ok(BlurStream { filters: tx, updates })
    }

    pub async fn subscribe_blur_events(&mut self, event_types: Vec<String>) -> Result<BlurStream> {
        self.subscribe_blur(SubscribeBlurRequest {
            event_type: event_types,
            ..Default::default()
        })
        .await
    }

    pub async fn subscribe_blur_mints(&mut self, mints: Vec<String>) -> Result<BlurStream> {
        self.subscribe_blur(SubscribeBlurRequest { mint: mints, ..Default::default() }).await
    }

    pub async fn subscribe_webhook(
        &mut self,
        request: SubscribeWebhookRequest,
    ) -> Result<WebhookStream> {
        let (tx, rx) = mpsc::channel(REQUEST_CHANNEL);
        tx.send(request)
            .await
            .map_err(|_| SolamiError::Grpc("request channel closed".into()))?;
        let req = authed(&self.token, ReceiverStream::new(rx))?;
        let updates = self
            .client
            .subscribe_webhook(req)
            .await
            .map_err(|e: Status| SolamiError::Grpc(e.to_string()))?
            .into_inner();
        Ok(WebhookStream { filters: tx, updates })
    }

    pub async fn subscribe_webhooks(&mut self, webhook_ids: Vec<String>) -> Result<WebhookStream> {
        self.subscribe_webhook(SubscribeWebhookRequest { webhook_id: webhook_ids }).await
    }
}

pub async fn connect_public(token: &str, url: &str) -> Result<GrpcClient> {
    connect(token, url).await
}

pub(crate) async fn connect(token: &str, url: &str) -> Result<GrpcClient> {
    let mut endpoint = Channel::from_shared(url.to_string())
        .map_err(|e| SolamiError::Grpc(e.to_string()))?
        .connect_timeout(Duration::from_secs(10))
        .http2_keep_alive_interval(Duration::from_secs(10))
        .keep_alive_timeout(Duration::from_secs(20))
        .keep_alive_while_idle(true);

    if url.starts_with("https") {
        endpoint = endpoint
            .tls_config(ClientTlsConfig::new().with_native_roots())
            .map_err(|e| SolamiError::Grpc(e.to_string()))?;
    }

    let channel = endpoint
        .connect()
        .await
        .map_err(|e| SolamiError::Grpc(e.to_string()))?;

    Ok(GrpcClient {
        url: url.to_string(),
        client: GeyserClient::new(channel),
        token: token.to_string(),
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use futures::SinkExt;
    use tonic::transport::Server;
    use crate::geyser::{
        geyser_server::{Geyser, GeyserServer},
        GetBlockHeightRequest, GetBlockHeightResponse, GetLatestBlockhashRequest,
        GetLatestBlockhashResponse, GetSlotRequest, GetSlotResponse, GetVersionRequest,
        GetVersionResponse, IsBlockhashValidRequest, IsBlockhashValidResponse, PingRequest,
        PongResponse, SubscribeReplayInfoRequest, SubscribeReplayInfoResponse,
        SubscribeUpdatePing,
    };

    struct MockGeyser;

    #[tonic::async_trait]
    impl Geyser for MockGeyser {
        type SubscribeStream = std::pin::Pin<
            Box<
                dyn futures::Stream<Item = std::result::Result<SubscribeUpdate, tonic::Status>>
                    + Send,
            >,
        >;
        async fn subscribe(
            &self,
            _request: tonic::Request<tonic::Streaming<SubscribeRequest>>,
        ) -> std::result::Result<tonic::Response<Self::SubscribeStream>, tonic::Status> {
            let stream = async_stream::stream! {
                yield Ok(SubscribeUpdate {
                    filters: vec!["t".into()],
                    update_oneof: Some(crate::geyser::subscribe_update::UpdateOneof::Ping(SubscribeUpdatePing {})),
                    created_at: None,
                });
            };
            Ok(tonic::Response::new(Box::pin(stream)))
        }


        type SubscribeBlurStream = std::pin::Pin<
            Box<
                dyn futures::Stream<Item = std::result::Result<SubscribeUpdateBlur, tonic::Status>>
                    + Send,
            >,
        >;
        async fn subscribe_blur(
            &self,
            request: tonic::Request<tonic::Streaming<SubscribeBlurRequest>>,
        ) -> std::result::Result<tonic::Response<Self::SubscribeBlurStream>, tonic::Status> {
            let mut inbound = request.into_inner();
            let first = inbound
                .message()
                .await?
                .ok_or_else(|| tonic::Status::invalid_argument("no request"))?;
            let echoed = first.event_type.first().cloned().unwrap_or_else(|| "swap".into());
            let stream = async_stream::stream! {
                yield Ok(SubscribeUpdateBlur {
                    event_type: echoed,
                    slot: 42,
                    block_time: 7,
                    mint: "MINT".into(),
                    pool: "POOL".into(),
                    json: "{\"type\":\"swap\"}".into(),
                });
                while let Ok(Some(_)) = inbound.message().await {}
            };
            Ok(tonic::Response::new(Box::pin(stream)))
        }

        type SubscribeWebhookStream = std::pin::Pin<
            Box<
                dyn futures::Stream<
                        Item = std::result::Result<SubscribeUpdateWebhook, tonic::Status>,
                    > + Send,
            >,
        >;
        async fn subscribe_webhook(
            &self,
            request: tonic::Request<tonic::Streaming<SubscribeWebhookRequest>>,
        ) -> std::result::Result<tonic::Response<Self::SubscribeWebhookStream>, tonic::Status> {
            let mut inbound = request.into_inner();
            let first = inbound
                .message()
                .await?
                .ok_or_else(|| tonic::Status::invalid_argument("no request"))?;
            let id = first.webhook_id.first().cloned().unwrap_or_default();
            let stream = async_stream::stream! {
                yield Ok(SubscribeUpdateWebhook {
                    webhook_id: id,
                    received_at: 9,
                    json: "{\"kind\":\"enriched\"}".into(),
                });
                while let Ok(Some(_)) = inbound.message().await {}
            };
            Ok(tonic::Response::new(Box::pin(stream)))
        }

        type SubscribeDeshredStream = std::pin::Pin<
            Box<
                dyn futures::Stream<
                        Item = std::result::Result<SubscribeUpdateDeshred, tonic::Status>,
                    > + Send,
            >,
        >;
        async fn subscribe_deshred(
            &self,
            _request: tonic::Request<tonic::Streaming<SubscribeDeshredRequest>>,
        ) -> std::result::Result<tonic::Response<Self::SubscribeDeshredStream>, tonic::Status>
        {
            let stream = async_stream::stream! {
                yield Ok(SubscribeUpdateDeshred {
                    filters: vec!["d".into()],
                    update_oneof: Some(crate::geyser::subscribe_update_deshred::UpdateOneof::Ping(SubscribeUpdatePing {})),
                    created_at: None,
                });
            };
            Ok(tonic::Response::new(Box::pin(stream)))
        }

        async fn subscribe_replay_info(
            &self,
            _r: tonic::Request<SubscribeReplayInfoRequest>,
        ) -> std::result::Result<tonic::Response<SubscribeReplayInfoResponse>, tonic::Status>
        {
            Err(tonic::Status::unimplemented(""))
        }
        async fn ping(
            &self,
            _r: tonic::Request<PingRequest>,
        ) -> std::result::Result<tonic::Response<PongResponse>, tonic::Status> {
            Err(tonic::Status::unimplemented(""))
        }
        async fn get_latest_blockhash(
            &self,
            _r: tonic::Request<GetLatestBlockhashRequest>,
        ) -> std::result::Result<tonic::Response<GetLatestBlockhashResponse>, tonic::Status>
        {
            Err(tonic::Status::unimplemented(""))
        }
        async fn get_block_height(
            &self,
            _r: tonic::Request<GetBlockHeightRequest>,
        ) -> std::result::Result<tonic::Response<GetBlockHeightResponse>, tonic::Status>
        {
            Err(tonic::Status::unimplemented(""))
        }
        async fn get_slot(
            &self,
            _r: tonic::Request<GetSlotRequest>,
        ) -> std::result::Result<tonic::Response<GetSlotResponse>, tonic::Status> {
            Err(tonic::Status::unimplemented(""))
        }
        async fn is_blockhash_valid(
            &self,
            _r: tonic::Request<IsBlockhashValidRequest>,
        ) -> std::result::Result<tonic::Response<IsBlockhashValidResponse>, tonic::Status>
        {
            Err(tonic::Status::unimplemented(""))
        }
        async fn get_version(
            &self,
            _r: tonic::Request<GetVersionRequest>,
        ) -> std::result::Result<tonic::Response<GetVersionResponse>, tonic::Status> {
            Err(tonic::Status::unimplemented(""))
        }
    }

    pub(crate) async fn spawn_grpc_server() -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let incoming = tokio_stream::wrappers::TcpListenerStream::new(listener);
        tokio::spawn(async move {
            let _ = Server::builder()
                .add_service(GeyserServer::new(MockGeyser))
                .serve_with_incoming(incoming)
                .await;
        });
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        format!("http://{addr}")
    }


    #[tokio::test(flavor = "multi_thread")]
    async fn subscribe_blur_streams_events() {
        let url = spawn_grpc_server().await;
        let mut client = connect("tok", &url).await.unwrap();
        let mut stream = client
            .subscribe_blur_events(vec!["token_create".into()])
            .await
            .unwrap();
        let update = stream.updates.message().await.unwrap().expect("an update");
        assert_eq!(update.event_type, "token_create", "the server saw our filter");
        assert_eq!(update.slot, 42);
        assert_eq!(update.mint, "MINT");
        assert!(update.json.contains("swap"), "raw event json is passed through");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn subscribe_blur_filters_are_resendable() {
        let url = spawn_grpc_server().await;
        let mut client = connect("tok", &url).await.unwrap();
        let stream = client.subscribe_blur_mints(vec!["MINT".into()]).await.unwrap();
        stream
            .filters
            .send(SubscribeBlurRequest { mint: vec!["OTHER".into()], ..Default::default() })
            .await
            .expect("filters can be updated without reconnecting");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn subscribe_webhook_streams_deliveries() {
        let url = spawn_grpc_server().await;
        let mut client = connect("tok", &url).await.unwrap();
        let mut stream = client.subscribe_webhooks(vec!["wh_123".into()]).await.unwrap();
        let update = stream.updates.message().await.unwrap().expect("a delivery");
        assert_eq!(update.webhook_id, "wh_123");
        assert_eq!(update.received_at, 9);
        assert!(update.json.contains("enriched"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn connect_and_url() {
        let url = spawn_grpc_server().await;
        let client = connect("tok", &url).await.unwrap();
        assert_eq!(client.url(), url);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn connect_fails_on_dead_port() {
        let r = connect("tok", "http://127.0.0.1:1").await;
        assert!(matches!(r, Err(SolamiError::Grpc(_))));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn connect_fails_on_invalid_url() {
        let r = connect("tok", "::::not a url::::").await;
        assert!(matches!(r, Err(SolamiError::Grpc(_))));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn subscribe_via_builder() {
        use futures::StreamExt;
        let url = spawn_grpc_server().await;
        let mut client = connect("tok", &url).await.unwrap();
        let req = SubscribeRequest {
            accounts: HashMap::default(),
            slots: HashMap::default(),
            transactions: HashMap::default(),
            transactions_status: HashMap::default(),
            blocks: HashMap::default(),
            blocks_meta: HashMap::default(),
            entry: HashMap::default(),
            commitment: None,
            accounts_data_slice: vec![],
            ping: None,
            from_slot: None,
        };
        let (_sink, mut stream) = client.subscribe(req).await.unwrap();
        let msg = stream.next().await.unwrap().unwrap();
        assert_eq!(msg.filters, vec!["t".to_string()]);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn subscribe_transactions_helper() {
        use futures::StreamExt;
        let url = spawn_grpc_server().await;
        let mut client = connect("tok", &url).await.unwrap();
        let (mut sink, mut stream) = client
            .subscribe_transactions("foo", vec!["acct".into()], CommitmentLevel::Processed)
            .await
            .unwrap();
        let msg = stream.next().await.unwrap().unwrap();
        assert!(!msg.filters.is_empty());
        let _ = sink
            .send(SubscribeRequest {
                accounts: HashMap::default(),
                slots: HashMap::default(),
                transactions: HashMap::default(),
                transactions_status: HashMap::default(),
                blocks: HashMap::default(),
                blocks_meta: HashMap::default(),
                entry: HashMap::default(),
                commitment: None,
                accounts_data_slice: vec![],
                ping: None,
                from_slot: None,
            })
            .await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn subscribe_deshred_via_builder() {
        use futures::StreamExt;
        let url = spawn_grpc_server().await;
        let mut client = connect("tok", &url).await.unwrap();
        let req = SubscribeDeshredRequest {
            deshred_transactions: HashMap::default(),
            ping: None,
            slots: HashMap::default(),
        };
        let (_sink, mut stream) = client.subscribe_deshred(req).await.unwrap();
        let msg = stream.next().await.unwrap().unwrap();
        assert_eq!(msg.filters, vec!["d".to_string()]);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn subscribe_deshred_transactions_helper() {
        use futures::StreamExt;
        let url = spawn_grpc_server().await;
        let mut client = connect("tok", &url).await.unwrap();
        let (mut sink, mut stream) = client
            .subscribe_deshred_transactions("d", vec!["acct".into()])
            .await
            .unwrap();
        let msg = stream.next().await.unwrap().unwrap();
        assert!(!msg.filters.is_empty());
        let _ = sink
            .send(SubscribeDeshredRequest {
                deshred_transactions: HashMap::default(),
                ping: None,
                slots: HashMap::default(),
            })
            .await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn https_url_path_triggers_tls() {
        rustls::crypto::ring::default_provider()
            .install_default()
            .ok();
        let r = connect("tok", "https://127.0.0.1:1").await;
        assert!(matches!(r, Err(SolamiError::Grpc(_))));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn mock_unimplemented_methods_are_covered() {
        let svc = MockGeyser;
        assert!(svc
            .subscribe_replay_info(tonic::Request::new(SubscribeReplayInfoRequest {}))
            .await
            .is_err());
        assert!(svc
            .ping(tonic::Request::new(PingRequest { count: 1 }))
            .await
            .is_err());
        assert!(svc
            .get_latest_blockhash(tonic::Request::new(GetLatestBlockhashRequest {
                commitment: None
            }))
            .await
            .is_err());
        assert!(svc
            .get_block_height(tonic::Request::new(GetBlockHeightRequest { commitment: None }))
            .await
            .is_err());
        assert!(svc
            .get_slot(tonic::Request::new(GetSlotRequest { commitment: None }))
            .await
            .is_err());
        assert!(svc
            .is_blockhash_valid(tonic::Request::new(IsBlockhashValidRequest {
                blockhash: String::new(),
                commitment: None
            }))
            .await
            .is_err());
        assert!(svc
            .get_version(tonic::Request::new(GetVersionRequest {}))
            .await
            .is_err());
    }
}
