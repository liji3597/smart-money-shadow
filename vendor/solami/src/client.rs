mod builder;

pub use builder::{builder, ApplyRegion, Builder, Connect};
#[cfg(feature = "grpc")]
pub use builder::GrpcCfg;
#[cfg(feature = "rpc")]
pub use builder::RpcCfg;
#[cfg(feature = "beam")]
pub use builder::SwqosCfg;

#[cfg(feature = "beam")]
use crate::error::Result;
#[cfg(feature = "grpc")]
use crate::grpc::GrpcClient;
#[cfg(feature = "rpc")]
use crate::rpc::RpcClient;
#[cfg(feature = "beam")]
use crate::swqos::SwqosClient;
#[cfg(feature = "rpc")]
use crate::ws::WsClient;
#[cfg(feature = "beam")]
use solana_signature::Signature;
#[cfg(feature = "beam")]
use solana_transaction::versioned::VersionedTransaction;

pub struct Off;
pub struct On<T>(pub(crate) T);

#[cfg(feature = "rpc")]
pub struct RpcKit {
    pub(crate) rpc: RpcClient,
    pub(crate) ws: Option<WsClient>,
}

#[allow(dead_code)]
pub struct Solami<R = Off, G = Off, S = Off> {
    pub(crate) rpc: R,
    pub(crate) grpc: G,
    pub(crate) swqos: S,
}

#[cfg(feature = "rpc")]
impl<G, S> Solami<On<RpcKit>, G, S> {
    pub fn rpc(&self) -> &RpcClient {
        &self.rpc.0.rpc
    }

    pub fn ws(&self) -> Option<&WsClient> {
        self.rpc.0.ws.as_ref()
    }
}

#[cfg(feature = "rpc")]
impl<G, S> std::ops::Deref for Solami<On<RpcKit>, G, S> {
    type Target = solana_client::nonblocking::rpc_client::RpcClient;

    fn deref(&self) -> &Self::Target {
        self.rpc.0.rpc.inner()
    }
}

#[cfg(feature = "grpc")]
impl<R, S> Solami<R, On<GrpcClient>, S> {
    pub fn grpc(&mut self) -> &mut GrpcClient {
        &mut self.grpc.0
    }
}

#[cfg(feature = "beam")]
impl<R, G> Solami<R, G, On<SwqosClient>> {
    pub fn swqos(&self) -> &SwqosClient {
        &self.swqos.0
    }

    pub async fn beam(&self, tx: &VersionedTransaction) -> Result<Signature> {
        if !self.swqos.0.skip_precheck && crate::swqos::extract_tip(tx, crate::swqos::TIP_ACCOUNTS).is_none() {
            return Err(crate::SolamiError::Swqos(
                "transaction must include a tip instruction".into(),
            ));
        }
        self.swqos.0.send_transaction(tx).await
    }

    pub async fn land_transaction(&self, tx: &VersionedTransaction) -> Result<Signature> {
        self.beam(tx).await
    }
}

#[cfg(all(test, feature = "rpc", feature = "grpc", feature = "beam"))]
mod tests {
    use super::*;
    use solana_message::Message;
    use solana_signer::Signer;
    use solana_transaction::Transaction;

    #[tokio::test(flavor = "multi_thread")]
    async fn rpc_only_solami_exposes_rpc_and_deref() {
        let body = r#"{"jsonrpc":"2.0","result":{"blockhash":"H","previousBlockhash":"P","parentSlot":1,"transactions":[],"rewards":[]},"id":1}"#;
        let rpc_base = crate::rpc::tests::spawn_http_server(body.to_string()).await;
        let ws_base = crate::ws::tests::spawn_ws_server().await;

        let s = crate::builder()
            .with_rpc("tok")
            .rpc_base(&rpc_base)
            .ws_base(&ws_base)
            .build()
            .await
            .unwrap();

        assert!(s.rpc().url().contains("tok"));
        assert!(s.ws().unwrap().url().contains("tok"));
        let block = s.rpc().get_block(1).await.unwrap();
        assert_eq!(block.blockhash, "H");
        let _: &solana_client::nonblocking::rpc_client::RpcClient = &s;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn grpc_only_solami_exposes_grpc() {
        let url = crate::grpc::tests::spawn_grpc_server().await;
        let mut s = crate::builder()
            .with_grpc("tok")
            .grpc_url(&url)
            .build()
            .await
            .unwrap();
        assert_eq!(s.grpc().url(), url);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn swqos_only_solami_lands_transaction() {
        let kp = crate::Keypair::new().to_base58_string();
        let (endpoint, _recv) = crate::swqos::tests::spawn_quic_beam().await;
        let s = crate::builder()
            .with_swqos(&kp)
            .beam_endpoint(&endpoint)
            .build()
            .await
            .unwrap();

        let payer = crate::Keypair::new();
        let tip = crate::build_tip_ix(&payer.pubkey(), 0.001);
        let msg = Message::new(&[tip], Some(&payer.pubkey()));
        let tx = VersionedTransaction::from(Transaction::new_unsigned(msg));
        let sig = s.land_transaction(&tx).await.unwrap();
        assert_eq!(sig, *tx.signatures.first().unwrap());

        let _: &SwqosClient = s.swqos();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn land_transaction_rejects_tx_without_tip() {
        let kp = crate::Keypair::new().to_base58_string();
        let (endpoint, _recv) = crate::swqos::tests::spawn_quic_beam().await;
        let s = crate::builder()
            .with_swqos(&kp)
            .beam_endpoint(&endpoint)
            .build()
            .await
            .unwrap();

        let payer = crate::Keypair::new();
        let stranger = crate::Keypair::new();
        let ix = solana_system_interface::instruction::transfer(
            &payer.pubkey(),
            &stranger.pubkey(),
            1,
        );
        let msg = Message::new(&[ix], Some(&payer.pubkey()));
        let tx = VersionedTransaction::from(Transaction::new_unsigned(msg));
        let r = s.land_transaction(&tx).await;
        assert!(matches!(r, Err(crate::SolamiError::Swqos(_))));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn full_kit_all_accessors_work() {
        let body = r#"{"jsonrpc":"2.0","result":{"blockhash":"H","previousBlockhash":"P","parentSlot":1,"transactions":[],"rewards":[]},"id":1}"#;
        let rpc_base = crate::rpc::tests::spawn_http_server(body.to_string()).await;
        let ws_base = crate::ws::tests::spawn_ws_server().await;
        let grpc_url = crate::grpc::tests::spawn_grpc_server().await;
        let kp = crate::Keypair::new().to_base58_string();
        let (beam, _r) = crate::swqos::tests::spawn_quic_beam().await;

        let mut s = crate::builder()
            .with_rpc("tok")
            .rpc_base(&rpc_base)
            .ws_base(&ws_base)
            .with_grpc("tok")
            .grpc_url(&grpc_url)
            .with_swqos(&kp)
            .beam_endpoint(&beam)
            .build()
            .await
            .unwrap();

        assert!(s.rpc().url().contains("tok"));
        assert!(s.ws().unwrap().url().contains("tok"));
        assert_eq!(s.grpc().url(), grpc_url);
        let _: &SwqosClient = s.swqos();
        let _: &solana_client::nonblocking::rpc_client::RpcClient = &s;
    }
}
