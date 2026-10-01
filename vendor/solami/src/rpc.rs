use solana_client::nonblocking::rpc_client::RpcClient as SolanaRpcClient;
use solana_client::rpc_config::RpcBlockConfig;
use solana_client::rpc_request::RpcRequest;
use solana_client::rpc_response::RpcKeyedAccount;
use solana_pubkey::Pubkey;
use serde::Serialize;
use serde_json::{json, Value};
use solana_transaction_status_client_types::UiConfirmedBlock;

use crate::error::{Result, SolamiError};

#[derive(Serialize, Clone, Copy)]
#[serde(rename_all = "camelCase")]
pub enum Encoding {
    Base64,
    Base58,
    JsonParsed,
    #[serde(rename = "base64+zstd")]
    Base64Zstd,
}

#[derive(Serialize, Clone, Copy)]
#[serde(rename_all = "camelCase")]
pub enum Commitment {
    Processed,
    Confirmed,
    Finalized,
}

#[derive(Serialize, Clone)]
pub struct DataSlice {
    pub offset: usize,
    pub length: usize,
}

#[derive(Serialize, Clone)]
pub enum Filter {
    #[serde(rename = "dataSize")]
    DataSize(u64),
    #[serde(rename = "memcmp")]
    Memcmp(Memcmp),
}

#[derive(Serialize, Clone)]
pub struct Memcmp {
    pub offset: usize,
    pub bytes: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encoding: Option<String>,
}

#[derive(Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct AccountsConfig {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub filters: Vec<Filter>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encoding: Option<Encoding>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data_slice: Option<DataSlice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commitment: Option<Commitment>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub changed_since_slot: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_context_slot: Option<u64>,
}

pub struct RpcClient {
    url: String,
    inner: SolanaRpcClient,
}

impl RpcClient {
    pub fn new_public(token: &str, base: &str) -> Self {
        Self::new(token, base)
    }

    pub(crate) fn new(token: &str, base: &str) -> Self {
        let url = format!("{base}?api_key={token}");
        let inner = SolanaRpcClient::new(url.clone());
        Self { url, inner }
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    pub fn inner(&self) -> &SolanaRpcClient {
        &self.inner
    }

    pub async fn get_block(&self, slot: u64) -> Result<UiConfirmedBlock> {
        self.inner
            .get_block_with_config(
                slot,
                RpcBlockConfig {
                    max_supported_transaction_version: Some(0),
                    ..Default::default()
                },
            )
            .await
            .map_err(|e| SolamiError::Rpc(e.to_string()))
    }

    pub fn get_program_accounts_v2(&self, program: &Pubkey, config: AccountsConfig) -> AccountPages<'_> {
        AccountPages::new(&self.inner, "getProgramAccountsV2", program, config)
    }

    pub fn get_token_accounts_by_mint(&self, mint: &Pubkey, config: AccountsConfig) -> AccountPages<'_> {
        AccountPages::new(&self.inner, "getTokenAccountsByMint", mint, config)
    }
}


#[derive(Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct AddressHistoryConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_order: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub direction: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub asset: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transaction_details: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encoding: Option<Encoding>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pagination_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commitment: Option<Commitment>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_context_slot: Option<u64>,
}

pub struct AddressHistoryPage {
    pub data: Vec<Value>,
    pub pagination_token: Option<String>,
}

impl RpcClient {
    async fn call(&self, method: &'static str, params: Value) -> Result<Value> {
        self.inner
            .send(RpcRequest::Custom { method }, params)
            .await
            .map_err(|e| SolamiError::Rpc(e.to_string()))
    }

    pub fn get_token_accounts_by_owner_v2(
        &self,
        owner: &Pubkey,
        config: AccountsConfig,
    ) -> AccountPages<'_> {
        AccountPages::new(&self.inner, "getTokenAccountsByOwnerV2", owner, config)
    }

    pub fn get_token_accounts_by_delegate_v2(
        &self,
        delegate: &Pubkey,
        config: AccountsConfig,
    ) -> AccountPages<'_> {
        AccountPages::new(&self.inner, "getTokenAccountsByDelegateV2", delegate, config)
    }

    pub fn get_token_accounts_by_mint_v2(
        &self,
        mint: &Pubkey,
        config: AccountsConfig,
    ) -> AccountPages<'_> {
        AccountPages::new(&self.inner, "getTokenAccountsByMintV2", mint, config)
    }

    pub fn get_token_largest_accounts_v2(
        &self,
        mint: &Pubkey,
        config: AccountsConfig,
    ) -> AccountPages<'_> {
        AccountPages::new(&self.inner, "getTokenLargestAccountsV2", mint, config)
    }

    pub async fn get_transactions_for_address(
        &self,
        address: &Pubkey,
        config: AddressHistoryConfig,
    ) -> Result<AddressHistoryPage> {
        let resp = self
            .call(
                "getTransactionsForAddress",
                json!([address.to_string(), config]),
            )
            .await?;
        Ok(address_history_page(resp))
    }

    pub async fn get_transfers_for_address(
        &self,
        address: &Pubkey,
        config: AddressHistoryConfig,
    ) -> Result<AddressHistoryPage> {
        let resp = self
            .call("getTransfersForAddress", json!([address.to_string(), config]))
            .await?;
        Ok(address_history_page(resp))
    }

    pub async fn get_token_accounts_by_mint_v1(
        &self,
        mint: &Pubkey,
        config: AccountsConfig,
    ) -> Result<Value> {
        self.call("getTokenAccountsByMint", json!([mint.to_string(), config])).await
    }

    pub async fn get_validator_health(&self) -> Result<Value> {
        self.call("getValidatorHealth", json!([])).await
    }
}

fn address_history_page(resp: Value) -> AddressHistoryPage {
    let body = if resp.get("value").is_some() { &resp["value"] } else { &resp };
    AddressHistoryPage {
        data: body["data"].as_array().cloned().unwrap_or_default(),
        pagination_token: body["paginationToken"].as_str().map(String::from),
    }
}

pub struct AccountPages<'a> {
    rpc: &'a SolanaRpcClient,
    method: &'static str,
    key: String,
    config: Value,
    cursor: Option<String>,
    done: bool,
}

impl<'a> AccountPages<'a> {
    fn new(rpc: &'a SolanaRpcClient, method: &'static str, key: &Pubkey, config: AccountsConfig) -> Self {
        let config = serde_json::to_value(config).unwrap_or_else(|_| json!({}));
        Self { rpc, method, key: key.to_string(), config, cursor: None, done: false }
    }

    pub async fn next(&mut self) -> Result<Option<Vec<RpcKeyedAccount>>> {
        if self.done {
            return Ok(None);
        }
        let mut config = self.config.clone();
        if let Some(cursor) = &self.cursor {
            config["paginationKey"] = json!(cursor);
        }
        let resp: Value = self
            .rpc
            .send(RpcRequest::Custom { method: self.method }, json!([self.key, config]))
            .await
            .map_err(|e| SolamiError::Rpc(e.to_string()))?;
        match resp["value"]["paginationKey"].as_str() {
            Some(k) => self.cursor = Some(k.to_string()),
            None => self.done = true,
        }
        let accounts = serde_json::from_value(resp["value"]["accounts"].clone())
            .map_err(|e| SolamiError::Rpc(e.to_string()))?;
        Ok(Some(accounts))
    }

    pub async fn all(mut self) -> Result<Vec<RpcKeyedAccount>> {
        let mut out = Vec::new();
        while let Some(page) = self.next().await? {
            out.extend(page);
        }
        Ok(out)
    }
}

impl std::ops::Deref for RpcClient {
    type Target = SolanaRpcClient;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use hyper::body::Bytes;
    use hyper::service::service_fn;
    use hyper::{Request, Response};
    use hyper_util::rt::TokioIo;

    #[test]
    fn url_format() {
        let c = RpcClient::new("tok", "https://x.test/sol");
        assert_eq!(c.url(), "https://x.test/sol?api_key=tok");
    }

    #[test]
    fn inner_and_deref() {
        let c = RpcClient::new("tok", "https://x.test/sol");
        let inner = c.inner();
        let derefed: &SolanaRpcClient = &c;
        assert!(std::ptr::eq(inner, derefed));
    }

    pub(crate) async fn spawn_http_server(response_body: String) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let (stream, _) = match listener.accept().await {
                    Ok(s) => s,
                    Err(_) => return,
                };
                let io = TokioIo::new(stream);
                let body = response_body.clone();
                tokio::spawn(async move {
                    let svc = service_fn(move |_req: Request<hyper::body::Incoming>| {
                        let body = body.clone();
                        async move {
                            Ok::<_, hyper::Error>(
                                Response::builder()
                                    .header("content-type", "application/json")
                                    .body(http_body_util::Full::new(Bytes::from(body)))
                                    .unwrap(),
                            )
                        }
                    });
                    let _ = hyper::server::conn::http1::Builder::new()
                        .serve_connection(io, svc)
                        .await;
                });
            }
        });
        format!("http://{addr}")
    }

    #[tokio::test]
    async fn get_block_success() {
        let body = r#"{"jsonrpc":"2.0","result":{"blockhash":"H","previousBlockhash":"P","parentSlot":99,"transactions":[],"rewards":[]},"id":1}"#;
        let base = spawn_http_server(body.to_string()).await;
        let c = RpcClient::new("tok", &base);
        let block = c.get_block(100).await.unwrap();
        assert_eq!(block.blockhash, "H");
        assert_eq!(block.parent_slot, 99);
    }

    #[tokio::test]
    async fn get_block_error_on_bad_response() {
        let base = spawn_http_server("not json".to_string()).await;
        let c = RpcClient::new("tok", &base);
        let r = c.get_block(100).await;
        assert!(matches!(r, Err(SolamiError::Rpc(_))));
    }

    #[tokio::test]
    async fn get_block_error_on_dead_port() {
        let c = RpcClient::new("tok", "http://127.0.0.1:1");
        let r = c.get_block(100).await;
        assert!(matches!(r, Err(SolamiError::Rpc(_))));
    }


    #[tokio::test]
    async fn transfers_for_address_parses_page_and_token() {
        let body = r#"{"jsonrpc":"2.0","id":1,"result":{"data":[{"signature":"s1","amount":"1000"}],"paginationToken":"12:1"}}"#;
        let base = spawn_http_server(body.to_string()).await;
        let c = RpcClient::new("tok", &base);
        let page = c
            .get_transfers_for_address(
                &Pubkey::new_unique(),
                AddressHistoryConfig {
                    asset: Some("SOL".into()),
                    direction: Some("in".into()),
                    limit: Some(2),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        assert_eq!(page.data.len(), 1);
        assert_eq!(page.data[0]["signature"], "s1");
        assert_eq!(page.pagination_token.as_deref(), Some("12:1"));
    }

    #[tokio::test]
    async fn address_history_end_of_pages_has_no_token() {
        let body = r#"{"jsonrpc":"2.0","id":1,"result":{"data":[],"paginationToken":null}}"#;
        let base = spawn_http_server(body.to_string()).await;
        let c = RpcClient::new("tok", &base);
        let page = c
            .get_transactions_for_address(&Pubkey::new_unique(), AddressHistoryConfig::default())
            .await
            .unwrap();
        assert!(page.data.is_empty());
        assert!(page.pagination_token.is_none(), "a null token means the walk is done");
    }

    #[tokio::test]
    async fn v2_token_account_methods_page_like_gpa_v2() {
        let bodies = vec![
            r#"{"jsonrpc":"2.0","id":1,"result":{"value":{"accounts":[{"pubkey":"11111111111111111111111111111111","account":{"lamports":1,"data":["","base64"],"owner":"11111111111111111111111111111111","executable":false,"rentEpoch":0,"space":0}}],"paginationKey":"k1"}}}"#.to_string(),
            r#"{"jsonrpc":"2.0","id":1,"result":{"value":{"accounts":[],"paginationKey":null}}}"#.to_string(),
        ];
        let base = spawn_paged_server(bodies).await;
        let c = RpcClient::new("tok", &base);
        let all = c
            .get_token_accounts_by_owner_v2(&Pubkey::new_unique(), AccountsConfig::default())
            .all()
            .await
            .unwrap();
        assert_eq!(all.len(), 1, "pages are concatenated until the key runs out");
    }

    #[tokio::test]
    async fn validator_health_is_passed_through() {
        let body = r#"{"jsonrpc":"2.0","id":1,"result":{"status":"ok","slotLag":3}}"#;
        let base = spawn_http_server(body.to_string()).await;
        let c = RpcClient::new("tok", &base);
        let v = c.get_validator_health().await.unwrap();
        assert_eq!(v["status"], "ok");
        assert_eq!(v["slotLag"], 3);
    }

    async fn spawn_paged_server(bodies: Vec<String>) -> String {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let n = Arc::new(AtomicUsize::new(0));
        tokio::spawn(async move {
            loop {
                let (stream, _) = match listener.accept().await {
                    Ok(s) => s,
                    Err(_) => return,
                };
                let io = TokioIo::new(stream);
                let bodies = bodies.clone();
                let n = n.clone();
                tokio::spawn(async move {
                    let svc = service_fn(move |_req: Request<hyper::body::Incoming>| {
                        let i = n.fetch_add(1, Ordering::SeqCst).min(bodies.len() - 1);
                        let body = bodies[i].clone();
                        async move {
                            Ok::<_, hyper::Error>(
                                Response::builder()
                                    .header("content-type", "application/json")
                                    .body(http_body_util::Full::new(Bytes::from(body)))
                                    .unwrap(),
                            )
                        }
                    });
                    let _ = hyper::server::conn::http1::Builder::new()
                        .serve_connection(io, svc)
                        .await;
                });
            }
        });
        format!("http://{addr}")
    }

    #[tokio::test]
    async fn account_pages_iterates_until_pagination_key_null() {
        let acct = r#"{"lamports":1,"data":["","base64"],"owner":"o","executable":false,"rentEpoch":0,"space":0}"#;
        let page1 = format!(
            r#"{{"jsonrpc":"2.0","id":1,"result":{{"context":{{"slot":1}},"value":{{"accounts":[{{"pubkey":"A","account":{acct}}},{{"pubkey":"B","account":{acct}}}],"paginationKey":"B"}}}}}}"#
        );
        let page2 = format!(
            r#"{{"jsonrpc":"2.0","id":1,"result":{{"context":{{"slot":1}},"value":{{"accounts":[{{"pubkey":"C","account":{acct}}}],"paginationKey":null}}}}}}"#
        );
        let base = spawn_paged_server(vec![page1.clone(), page2.clone()]).await;
        let c = RpcClient::new("tok", &base);

        let config = AccountsConfig { filters: vec![Filter::DataSize(3)], ..Default::default() };
        let mut pages = c.get_program_accounts_v2(&Pubkey::new_unique(), config);
        assert_eq!(pages.next().await.unwrap().unwrap().len(), 2);
        assert_eq!(pages.next().await.unwrap().unwrap().len(), 1);
        assert!(pages.next().await.unwrap().is_none());

        let base = spawn_paged_server(vec![page1.clone(), page2.clone()]).await;
        let all = RpcClient::new("tok", &base)
            .get_token_accounts_by_mint(&Pubkey::new_unique(), AccountsConfig::default())
            .all()
            .await
            .unwrap();
        assert_eq!(all.len(), 3);

        let config = AccountsConfig {
            filters: vec![
                Filter::DataSize(165),
                Filter::Memcmp(Memcmp { offset: 0, bytes: "abc".into(), encoding: Some("base58".into()) }),
            ],
            encoding: Some(Encoding::Base64Zstd),
            commitment: Some(Commitment::Finalized),
            ..Default::default()
        };
        assert_eq!(
            serde_json::to_value(&config).unwrap(),
            json!({
                "filters": [{ "dataSize": 165 }, { "memcmp": { "offset": 0, "bytes": "abc", "encoding": "base58" } }],
                "encoding": "base64+zstd",
                "commitment": "finalized"
            })
        );
    }

}
