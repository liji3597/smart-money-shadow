use std::net::{SocketAddr, ToSocketAddrs as _};
use std::sync::Arc;
use std::time::Duration;

use arc_swap::ArcSwap;
use quinn::{
    crypto::rustls::QuicClientConfig, ClientConfig, Connection, Endpoint, IdleTimeout,
    TransportConfig,
};
use rand::seq::IndexedRandom as _;
use solana_instruction::Instruction;
use solana_keypair::Keypair;
use solana_pubkey::{Pubkey, pubkey};
use solana_signature::Signature;
use solana_transaction::versioned::VersionedTransaction;
use solana_tls_utils::{SkipServerVerification, new_dummy_x509_certificate};
use tokio::sync::Mutex;
use tracing::{info, warn};

use crate::error::{Result, SolamiError};

const ALPN_TPU_PROTOCOL_ID: &[u8] = b"solana-tpu";
const SOLAMI_SERVER: &str = "solami-beam";
const KEEP_ALIVE_INTERVAL: Duration = Duration::from_secs(25);
const MAX_IDLE_TIMEOUT: Duration = Duration::from_secs(5 * 60);

pub const TIP_ACCOUNTS: &[Pubkey] = &[
    pubkey!("15qWd4huAkoxvhDsHMfpUn27TW1YBYMMJJ2jkAkbeam"),
    pubkey!("9XuGciSwr5wb7dLTQm91JhuBTvj3GG8WjuRDc3obeam"),
    pubkey!("kiQioJNyFG7pU36ELLsRKXkeT48kFbk3b6rSgrWbeam"),
    pubkey!("kjmVhW1UzJrW2sU5bY5NtZ79jpvjSStsj37Pzmabeam"),
    pubkey!("kREnjPWFpt4AHeY5pijPmyXaCrMnbatUQJo7d3Xbeam"),
    pubkey!("praRZG6N6MdbsT4EFpKgZJWReZGXQhAMFcH68oCbeam"),
    pubkey!("SqoKQKU5uwBxovq3R7yEBxFwptc4z7vwoghU3M9beam"),
    pubkey!("sV72TY66T1RfmDSeHPPbwX6wwJ3bBv5hd4ehJ8tbeam"),
    pubkey!("swf8MyEeLo7gtRUo27UuJj6naCASUrypU7dbteSbeam"),
    pubkey!("uiuaQsxA47JybQAVN4FTfYuoEDkMiXV1r591Aewbeam"),
];

const TIP_API_URL: &str = "https://api.solami.dev/onchain/tip-addresses";

pub async fn fetch_tip_accounts() -> Result<Vec<Pubkey>> {
    let resp: Vec<String> = reqwest::get(TIP_API_URL)
        .await
        .map_err(|e| SolamiError::Api(e.to_string()))?
        .json()
        .await
        .map_err(|e| SolamiError::Api(e.to_string()))?;

    resp.iter()
        .map(|s| s.parse::<Pubkey>().map_err(|e| SolamiError::Api(e.to_string())))
        .collect()
}

pub fn build_tip_ix(payer: &Pubkey, sol: f64) -> Instruction {
    let sol = if sol < 0.0 { 0.0 } else { sol };
    let lamports = (sol * 1_000_000_000.0) as u64;
    let tip_account = *TIP_ACCOUNTS
        .choose(&mut rand::rng())
        .unwrap_or(&TIP_ACCOUNTS[0]);

    solana_system_interface::instruction::transfer(payer, &tip_account, lamports)
}

#[tracing::instrument(skip(tx, tip_pubkeys))]
pub(crate) fn extract_tip(tx: &VersionedTransaction, tip_pubkeys: &[Pubkey]) -> Option<(u64, Pubkey)> {
    let keys = tx.message.static_account_keys();
    let system_program = solana_system_interface::program::id();

    for ix in tx.message.instructions() {
        let Some(program_key) = keys.get(ix.program_id_index as usize) else { continue };
        if *program_key != system_program {
            continue;
        }
        if ix.data.len() < 12 {
            continue;
        }
        let Ok(ix_type) = ix.data[0..4].try_into().map(u32::from_le_bytes) else { continue };
        if ix_type != 2 {
            continue;
        }
        let Ok(lamports) = ix.data[4..12].try_into().map(u64::from_le_bytes) else { continue };
        let Some(to_index) = ix.accounts.get(1).map(|i| *i as usize) else { continue };
        let Some(to_pubkey) = keys.get(to_index) else { continue };
        if tip_pubkeys.contains(to_pubkey) {
            return Some((lamports, *to_pubkey));
        }
    }
    None
}

pub struct SwqosClient {
    endpoint: Endpoint,
    client_config: ClientConfig,
    addr: SocketAddr,
    connection: ArcSwap<Connection>,
    reconnect: Mutex<()>,
    pub(crate) skip_precheck: bool,
}

impl SwqosClient {
    pub(crate) async fn connect(swqos_key: &str, beam: &str) -> Result<Self> {
        let keypair = Keypair::from_base58_string(swqos_key);
        let (cert, key) = new_dummy_x509_certificate(&keypair);

        let mut crypto = rustls::ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(SkipServerVerification::new())
            .with_client_auth_cert(vec![cert], key)
            .map_err(|e| SolamiError::Swqos(e.to_string()))?;

        crypto.alpn_protocols = vec![ALPN_TPU_PROTOCOL_ID.to_vec()];

        let client_crypto = QuicClientConfig::try_from(crypto)
            .map_err(|e| SolamiError::Swqos(format!("quinn crypto config: {e}")))?;

        let mut client_config = ClientConfig::new(Arc::new(client_crypto));
        let mut transport = TransportConfig::default();
        transport.keep_alive_interval(Some(KEEP_ALIVE_INTERVAL));
        transport.max_idle_timeout(Some(
            IdleTimeout::try_from(MAX_IDLE_TIMEOUT)
                .map_err(|e| SolamiError::Swqos(e.to_string()))?,
        ));
        client_config.transport_config(Arc::new(transport));

        let mut endpoint = Endpoint::client("0.0.0.0:0".parse().unwrap())
            .map_err(|e| SolamiError::Swqos(e.to_string()))?;
        endpoint.set_default_client_config(client_config.clone());

        let addr = beam
            .to_socket_addrs()
            .map_err(|e| SolamiError::Swqos(e.to_string()))?
            .next()
            .ok_or_else(|| SolamiError::Swqos("failed to resolve solami endpoint".into()))?;

        info!(%beam, "connecting to solami beam");
        let connection = endpoint
            .connect(addr, SOLAMI_SERVER)
            .map_err(|e| SolamiError::Swqos(e.to_string()))?
            .await
            .map_err(|e| SolamiError::Swqos(e.to_string()))?;
        info!("solami QUIC connection established");

        Ok(Self {
            endpoint,
            client_config,
            addr,
            connection: ArcSwap::from_pointee(connection),
            reconnect: Mutex::new(()),
            skip_precheck: false,
        })
    }

    pub(crate) async fn reconnect(&self) -> Result<()> {
        let _guard = self
            .reconnect
            .try_lock()
            .map_err(|_| SolamiError::Swqos("reconnect already in progress".into()))?;

        let connection = self
            .endpoint
            .connect_with(self.client_config.clone(), self.addr, SOLAMI_SERVER)
            .map_err(|e| SolamiError::Swqos(e.to_string()))?
            .await
            .map_err(|e| SolamiError::Swqos(e.to_string()))?;

        self.connection.store(Arc::new(connection));
        info!("solami reconnected");
        Ok(())
    }

    async fn try_send_bytes(connection: &Connection, payload: &[u8]) -> Result<()> {
        let mut stream = connection
            .open_uni()
            .await
            .map_err(|e| SolamiError::Swqos(e.to_string()))?;
        stream
            .write_all(payload)
            .await
            .map_err(|e| SolamiError::Swqos(e.to_string()))?;
        stream
            .finish()
            .map_err(|e| SolamiError::Swqos(e.to_string()))?;
        Ok(())
    }

    pub async fn send_transaction(&self, tx: &VersionedTransaction) -> Result<Signature> {
        let sig = *tx
            .signatures
            .first()
            .ok_or_else(|| SolamiError::Swqos("transaction has no signature".into()))?;

        let serialized =
            bincode::serialize(tx).map_err(|e| SolamiError::Swqos(e.to_string()))?;

        let connection = self.connection.load_full();
        if Self::try_send_bytes(&connection, &serialized).await.is_err() {
            warn!("solami send failed, reconnecting");
            self.reconnect().await?;
            let connection = self.connection.load_full();
            Self::try_send_bytes(&connection, &serialized)
                .await
                .map_err(|e| SolamiError::Swqos(format!("send after reconnect: {e}")))?;
        }

        info!(%sig, "tx sent via solami QUIC");
        Ok(sig)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use solana_message::Message;
    use solana_signer::Signer;
    use solana_transaction::Transaction;

    fn keypair() -> Keypair {
        Keypair::new()
    }

    #[test]
    fn build_tip_ix_zero_is_zero() {
        let payer = keypair().pubkey();
        let ix = build_tip_ix(&payer, 0.0);
        let lamports = u64::from_le_bytes(ix.data[4..12].try_into().unwrap());
        assert_eq!(lamports, 0);
    }

    #[test]
    fn build_tip_ix_negative_clamps_to_zero() {
        let payer = keypair().pubkey();
        let ix = build_tip_ix(&payer, -1.0);
        let lamports = u64::from_le_bytes(ix.data[4..12].try_into().unwrap());
        assert_eq!(lamports, 0);
    }

    #[test]
    fn build_tip_ix_positive() {
        let payer = keypair().pubkey();
        let ix = build_tip_ix(&payer, 0.5);
        let lamports = u64::from_le_bytes(ix.data[4..12].try_into().unwrap());
        assert_eq!(lamports, 500_000_000);
    }

    #[test]
    fn build_tip_ix_program_and_accounts() {
        let payer = keypair().pubkey();
        let ix = build_tip_ix(&payer, 0.001);
        assert_eq!(ix.program_id, solana_system_interface::program::id());
        assert_eq!(ix.accounts.len(), 2);
        assert_eq!(ix.accounts[0].pubkey, payer);
        assert!(TIP_ACCOUNTS.contains(&ix.accounts[1].pubkey));
    }

    fn make_tx(ixs: Vec<solana_instruction::Instruction>) -> VersionedTransaction {
        let payer = keypair();
        let msg = Message::new(&ixs, Some(&payer.pubkey()));
        VersionedTransaction::from(Transaction::new_unsigned(msg))
    }

    #[test]
    fn extract_tip_finds_tip_to_known_account() {
        let payer = keypair().pubkey();
        let tip_ix = build_tip_ix(&payer, 0.01);
        let tx = make_tx(vec![tip_ix]);
        let result = extract_tip(&tx, TIP_ACCOUNTS);
        assert!(result.is_some());
        let (lamports, dest) = result.unwrap();
        assert_eq!(lamports, 10_000_000);
        assert!(TIP_ACCOUNTS.contains(&dest));
    }

    #[test]
    fn extract_tip_ignores_transfer_to_unknown_account() {
        let payer = keypair();
        let stranger = keypair().pubkey();
        let ix = solana_system_interface::instruction::transfer(&payer.pubkey(), &stranger, 1_000);
        let tx = make_tx(vec![ix]);
        assert!(extract_tip(&tx, TIP_ACCOUNTS).is_none());
    }

    #[test]
    fn extract_tip_ignores_non_system_program() {
        let payer = keypair();
        let ix = solana_instruction::Instruction {
            program_id: solana_pubkey::Pubkey::new_unique(),
            accounts: vec![],
            data: vec![2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        };
        let msg = Message::new(&[ix], Some(&payer.pubkey()));
        let tx = VersionedTransaction::from(Transaction::new_unsigned(msg));
        assert!(extract_tip(&tx, TIP_ACCOUNTS).is_none());
    }

    #[test]
    fn extract_tip_ignores_short_data() {
        let payer = keypair();
        let ix = solana_instruction::Instruction {
            program_id: solana_system_interface::program::id(),
            accounts: vec![],
            data: vec![2, 0, 0, 0],
        };
        let msg = Message::new(&[ix], Some(&payer.pubkey()));
        let tx = VersionedTransaction::from(Transaction::new_unsigned(msg));
        assert!(extract_tip(&tx, TIP_ACCOUNTS).is_none());
    }

    #[test]
    fn extract_tip_ignores_non_transfer_ix_type() {
        let payer = keypair();
        let dest = TIP_ACCOUNTS[0];
        let ix = solana_instruction::Instruction {
            program_id: solana_system_interface::program::id(),
            accounts: vec![
                solana_instruction::AccountMeta::new(payer.pubkey(), true),
                solana_instruction::AccountMeta::new(dest, false),
            ],
            data: vec![1, 0, 0, 0, 100, 0, 0, 0, 0, 0, 0, 0],
        };
        let msg = Message::new(&[ix], Some(&payer.pubkey()));
        let tx = VersionedTransaction::from(Transaction::new_unsigned(msg));
        assert!(extract_tip(&tx, TIP_ACCOUNTS).is_none());
    }

    #[test]
    fn extract_tip_finds_among_multiple_ixs() {
        let payer = keypair();
        let stranger = keypair().pubkey();
        let bogus = solana_system_interface::instruction::transfer(&payer.pubkey(), &stranger, 1);
        let tip = build_tip_ix(&payer.pubkey(), 0.005);
        let msg = Message::new(&[bogus, tip], Some(&payer.pubkey()));
        let tx = VersionedTransaction::from(Transaction::new_unsigned(msg));
        let (lamports, _) = extract_tip(&tx, TIP_ACCOUNTS).unwrap();
        assert_eq!(lamports, 5_000_000);
    }

    #[test]
    fn extract_tip_returns_none_when_no_instructions() {
        let payer = keypair();
        let msg = Message::new(&[], Some(&payer.pubkey()));
        let tx = VersionedTransaction::from(Transaction::new_unsigned(msg));
        assert!(extract_tip(&tx, TIP_ACCOUNTS).is_none());
    }

    #[test]
    fn tip_accounts_constant_shape() {
        assert_eq!(TIP_ACCOUNTS.len(), 10);
        for acc in TIP_ACCOUNTS {
            assert!(acc.to_string().to_lowercase().ends_with("beam"));
        }
    }

    use quinn::ServerConfig;
    use solana_tls_utils::SkipClientVerification;

    pub(crate) async fn spawn_quic_beam() -> (String, tokio::sync::oneshot::Receiver<Vec<u8>>) {
        rustls::crypto::ring::default_provider()
            .install_default()
            .ok();
        let server_kp = Keypair::new();
        let (cert, key) = new_dummy_x509_certificate(&server_kp);

        let mut crypto = rustls::ServerConfig::builder()
            .with_client_cert_verifier(SkipClientVerification::new())
            .with_single_cert(vec![cert], key)
            .unwrap();
        crypto.alpn_protocols = vec![ALPN_TPU_PROTOCOL_ID.to_vec()];

        let server_crypto =
            quinn::crypto::rustls::QuicServerConfig::try_from(crypto).unwrap();
        let mut server_config = ServerConfig::with_crypto(Arc::new(server_crypto));
        let mut transport = TransportConfig::default();
        transport.keep_alive_interval(Some(Duration::from_secs(10)));
        server_config.transport_config(Arc::new(transport));

        let endpoint =
            quinn::Endpoint::server(server_config, "127.0.0.1:0".parse().unwrap()).unwrap();
        let addr = endpoint.local_addr().unwrap();

        let (tx, rx) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            let mut tx_holder = Some(tx);
            while let Some(incoming) = endpoint.accept().await {
                let tx = tx_holder.take();
                tokio::spawn(async move {
                    let conn = match incoming.await {
                        Ok(c) => c,
                        Err(_) => return,
                    };
                    if let Ok(mut stream) = conn.accept_uni().await {
                        let payload = stream.read_to_end(64 * 1024).await.unwrap_or_default();
                        if let Some(t) = tx {
                            let _ = t.send(payload);
                        }
                    }
                });
            }
        });

        (format!("127.0.0.1:{}", addr.port()), rx)
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn swqos_connect_and_send() {
        let kp = Keypair::new().to_base58_string();
        let (endpoint, recv) = spawn_quic_beam().await;
        let client = SwqosClient::connect(&kp, &endpoint).await.unwrap();

        let payer = Keypair::new();
        let tip_ix = build_tip_ix(&payer.pubkey(), 0.001);
        let msg = Message::new(&[tip_ix], Some(&payer.pubkey()));
        let tx = VersionedTransaction::from(Transaction::new_unsigned(msg));
        let sig = client.send_transaction(&tx).await.unwrap();
        assert_eq!(sig, *tx.signatures.first().unwrap());

        let payload = tokio::time::timeout(Duration::from_secs(2), recv)
            .await
            .unwrap()
            .unwrap();
        assert!(!payload.is_empty());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn swqos_send_tx_without_signature_fails() {
        let kp = Keypair::new().to_base58_string();
        let (endpoint, _recv) = spawn_quic_beam().await;
        let client = SwqosClient::connect(&kp, &endpoint).await.unwrap();

        let payer = Keypair::new();
        let msg = Message::new(&[], Some(&payer.pubkey()));
        let mut tx = VersionedTransaction::from(Transaction::new_unsigned(msg));
        tx.signatures.clear();
        let result = client.send_transaction(&tx).await;
        assert!(matches!(result, Err(SolamiError::Swqos(_))));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn swqos_connect_fails_on_unresolvable_endpoint() {
        let kp = Keypair::new().to_base58_string();
        let r = SwqosClient::connect(&kp, "not-a-valid-host\0:9").await;
        assert!(matches!(r, Err(SolamiError::Swqos(_))));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn swqos_connect_fails_on_dead_endpoint() {
        let kp = Keypair::new().to_base58_string();
        let r = tokio::time::timeout(
            Duration::from_secs(2),
            SwqosClient::connect(&kp, "127.0.0.1:1"),
        )
        .await;
        assert!(r.is_err() || matches!(r.unwrap(), Err(SolamiError::Swqos(_))));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn fetch_tip_accounts_network_error() {
        let result = fetch_tip_accounts().await;
        let _ = result;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn reconnect_succeeds_against_running_server() {
        let kp = Keypair::new().to_base58_string();
        let (endpoint, _recv) = spawn_quic_beam().await;
        let client = SwqosClient::connect(&kp, &endpoint).await.unwrap();
        client.reconnect().await.unwrap();
    }

    async fn spawn_quic_close_first_then_accept(
    ) -> (String, tokio::sync::mpsc::UnboundedReceiver<Vec<u8>>) {
        rustls::crypto::ring::default_provider()
            .install_default()
            .ok();
        let server_kp = Keypair::new();
        let (cert, key) = new_dummy_x509_certificate(&server_kp);
        let mut crypto = rustls::ServerConfig::builder()
            .with_client_cert_verifier(SkipClientVerification::new())
            .with_single_cert(vec![cert], key)
            .unwrap();
        crypto.alpn_protocols = vec![ALPN_TPU_PROTOCOL_ID.to_vec()];
        let server_crypto =
            quinn::crypto::rustls::QuicServerConfig::try_from(crypto).unwrap();
        let server_config = quinn::ServerConfig::with_crypto(Arc::new(server_crypto));
        let endpoint =
            quinn::Endpoint::server(server_config, "127.0.0.1:0".parse().unwrap()).unwrap();
        let addr = endpoint.local_addr().unwrap();

        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        tokio::spawn(async move {
            let mut nth = 0usize;
            while let Some(incoming) = endpoint.accept().await {
                let tx = tx.clone();
                nth += 1;
                let n = nth;
                tokio::spawn(async move {
                    let conn = match incoming.await {
                        Ok(c) => c,
                        Err(_) => return,
                    };
                    if n == 1 {
                        conn.close(0u32.into(), b"bye");
                    } else if let Ok(mut stream) = conn.accept_uni().await {
                        let payload =
                            stream.read_to_end(64 * 1024).await.unwrap_or_default();
                        let _ = tx.send(payload);
                    }
                });
            }
        });

        (format!("127.0.0.1:{}", addr.port()), rx)
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn send_triggers_reconnect_then_succeeds() {
        let kp = Keypair::new().to_base58_string();
        let (endpoint, mut rx) = spawn_quic_close_first_then_accept().await;
        let client = SwqosClient::connect(&kp, &endpoint).await.unwrap();

        tokio::time::sleep(Duration::from_millis(200)).await;

        let payer = Keypair::new();
        let tip_ix = build_tip_ix(&payer.pubkey(), 0.001);
        let msg = Message::new(&[tip_ix], Some(&payer.pubkey()));
        let tx = VersionedTransaction::from(Transaction::new_unsigned(msg));
        let _ = client.send_transaction(&tx).await;

        let payload = tokio::time::timeout(Duration::from_secs(3), rx.recv())
            .await
            .ok()
            .flatten();
        assert!(payload.is_some());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn reconnect_lock_contention() {
        let kp = Keypair::new().to_base58_string();
        let (endpoint, _recv) = spawn_quic_beam().await;
        let client = Arc::new(SwqosClient::connect(&kp, &endpoint).await.unwrap());
        let guard = client.reconnect.try_lock().unwrap();
        let result = client.reconnect().await;
        assert!(matches!(result, Err(SolamiError::Swqos(_))));
        drop(guard);
    }
}
