#[cfg(any(feature = "rpc", feature = "grpc", feature = "beam"))]
use crate::client::On;
use crate::client::{Off, Solami};
use crate::config::Region;
use crate::error::Result;

#[cfg(feature = "rpc")]
use crate::client::RpcKit;
#[cfg(feature = "grpc")]
use crate::grpc::{self, GrpcClient};
#[cfg(feature = "rpc")]
use crate::rpc::RpcClient;
#[cfg(feature = "beam")]
use crate::swqos::SwqosClient;
#[cfg(feature = "rpc")]
use crate::ws::WsClient;

#[cfg(feature = "rpc")]
pub struct RpcCfg {
    pub(crate) token: String,
    pub(crate) rpc_base: String,
    pub(crate) ws_base: String,
}

#[cfg(feature = "grpc")]
pub struct GrpcCfg {
    pub(crate) token: String,
    pub(crate) url: String,
}

#[cfg(feature = "beam")]
pub struct SwqosCfg {
    pub(crate) key: String,
    pub(crate) endpoint: String,
    pub(crate) skip_precheck: bool,
}

pub struct Builder<R = Off, G = Off, S = Off> {
    pub(crate) rpc: R,
    pub(crate) grpc: G,
    pub(crate) swqos: S,
}

pub fn builder() -> Builder<Off, Off, Off> {
    Builder {
        rpc: Off,
        grpc: Off,
        swqos: Off,
    }
}

#[cfg(feature = "rpc")]
impl<G, S> Builder<Off, G, S> {
    pub fn with_rpc(self, token: impl Into<String>) -> Builder<RpcCfg, G, S> {
        let region = Region::default();
        Builder {
            rpc: RpcCfg {
                token: token.into(),
                rpc_base: region.rpc_base(),
                ws_base: region.ws_base(),
            },
            grpc: self.grpc,
            swqos: self.swqos,
        }
    }
}

#[cfg(feature = "rpc")]
impl<G, S> Builder<RpcCfg, G, S> {
    pub fn rpc_base(mut self, url: impl Into<String>) -> Self {
        self.rpc.rpc_base = url.into();
        self
    }

    pub fn ws_base(mut self, url: impl Into<String>) -> Self {
        self.rpc.ws_base = url.into();
        self
    }

    pub fn rpc_region(mut self, region: Region) -> Self {
        self.rpc.rpc_base = region.rpc_base();
        self.rpc.ws_base = region.ws_base();
        self
    }
}

#[cfg(feature = "grpc")]
impl<R, S> Builder<R, Off, S> {
    pub fn with_grpc(self, token: impl Into<String>) -> Builder<R, GrpcCfg, S> {
        Builder {
            rpc: self.rpc,
            grpc: GrpcCfg {
                token: token.into(),
                url: Region::default().grpc_url(),
            },
            swqos: self.swqos,
        }
    }
}

#[cfg(feature = "grpc")]
impl<R, S> Builder<R, GrpcCfg, S> {
    pub fn grpc_url(mut self, url: impl Into<String>) -> Self {
        self.grpc.url = url.into();
        self
    }

    pub fn grpc_region(mut self, region: Region) -> Self {
        self.grpc.url = region.grpc_url();
        self
    }
}

#[cfg(feature = "beam")]
impl<R, G> Builder<R, G, Off> {
    #[deprecated]
    pub fn with_swqos(self, key: impl Into<String>) -> Builder<R, G, SwqosCfg> {
        Builder {
            rpc: self.rpc,
            grpc: self.grpc,
            swqos: SwqosCfg {
                key: key.into(),
                endpoint: Region::default().beam_endpoint(),
                skip_precheck: false,
            },
        }
    }
    pub fn with_beam(self, key: impl Into<String>) -> Builder<R, G, SwqosCfg> {
        Builder {
            rpc: self.rpc,
            grpc: self.grpc,
            swqos: SwqosCfg {
                key: key.into(),
                endpoint: Region::default().beam_endpoint(),
                skip_precheck: false,
            },
        }
    }
}

#[cfg(feature = "beam")]
impl<R, G> Builder<R, G, SwqosCfg> {
    pub fn beam_endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.swqos.endpoint = endpoint.into();
        self
    }

    pub fn beam_region(mut self, region: Region) -> Self {
        self.swqos.endpoint = region.beam_endpoint();
        self
    }

    /// Send without the local "has a tip" scan. The scan only reads static account keys,
    /// so a tip account loaded from an address lookup table looks like no tip; the Beam
    /// server still enforces the tip either way.
    pub fn skip_precheck(mut self) -> Self {
        self.swqos.skip_precheck = true;
        self
    }
}

pub trait ApplyRegion {
    fn apply_region(&mut self, region: Region);
}

impl ApplyRegion for Off {
    fn apply_region(&mut self, _: Region) {}
}

#[cfg(feature = "rpc")]
impl ApplyRegion for RpcCfg {
    fn apply_region(&mut self, region: Region) {
        self.rpc_base = region.rpc_base();
        self.ws_base = region.ws_base();
    }
}

#[cfg(feature = "grpc")]
impl ApplyRegion for GrpcCfg {
    fn apply_region(&mut self, region: Region) {
        self.url = region.grpc_url();
    }
}

#[cfg(feature = "beam")]
impl ApplyRegion for SwqosCfg {
    fn apply_region(&mut self, region: Region) {
        self.endpoint = region.beam_endpoint();
    }
}

impl<R: ApplyRegion, G: ApplyRegion, S: ApplyRegion> Builder<R, G, S> {
    pub fn region(mut self, region: Region) -> Self {
        self.rpc.apply_region(region);
        self.grpc.apply_region(region);
        self.swqos.apply_region(region);
        self
    }
}

pub trait Connect {
    type Output;
    fn connect(self) -> impl std::future::Future<Output = Result<Self::Output>> + Send;
}

impl Connect for Off {
    type Output = Off;
    async fn connect(self) -> Result<Off> {
        Ok(Off)
    }
}

#[cfg(feature = "rpc")]
impl Connect for RpcCfg {
    type Output = On<RpcKit>;
    async fn connect(self) -> Result<On<RpcKit>> {
        install_crypto();
        let rpc = RpcClient::new(&self.token, &self.rpc_base);
        // Vendored patch (smart-money-shadow): a failed pubsub WS connect must
        // not take down the whole RPC kit — RPC over HTTPS still works, and
        // callers that only need RPC shouldn't pay for a WS they never use.
        let ws = match WsClient::connect(&self.token, &self.ws_base).await {
            Ok(ws) => Some(ws),
            Err(e) => {
                tracing::warn!(error = %e, "solami: pubsub ws connect failed, continuing rpc-only");
                None
            }
        };
        Ok(On(RpcKit { rpc, ws }))
    }
}

#[cfg(feature = "grpc")]
impl Connect for GrpcCfg {
    type Output = On<GrpcClient>;
    async fn connect(self) -> Result<On<GrpcClient>> {
        install_crypto();
        let client = grpc::connect(&self.token, &self.url).await?;
        Ok(On(client))
    }
}

#[cfg(feature = "beam")]
impl Connect for SwqosCfg {
    type Output = On<SwqosClient>;
    async fn connect(self) -> Result<On<SwqosClient>> {
        install_crypto();
        let mut client = SwqosClient::connect(&self.key, &self.endpoint).await?;
        client.skip_precheck = self.skip_precheck;
        Ok(On(client))
    }
}

impl<R, G, S> Builder<R, G, S>
where
    R: Connect + Send,
    G: Connect + Send,
    S: Connect + Send,
{
    pub async fn build(self) -> Result<Solami<R::Output, G::Output, S::Output>> {
        let rpc = self.rpc.connect().await?;
        let grpc = self.grpc.connect().await?;
        let swqos = self.swqos.connect().await?;
        Ok(Solami { rpc, grpc, swqos })
    }
}

#[cfg(any(feature = "rpc", feature = "grpc", feature = "beam"))]
fn install_crypto() {
    rustls::crypto::ring::default_provider()
        .install_default()
        .ok();
}

#[cfg(all(test, feature = "rpc", feature = "grpc", feature = "beam"))]
mod tests {
    use super::*;

    #[test]
    fn builder_starts_empty() {
        let b = builder();
        let _: Off = b.rpc;
        let _: Off = b.grpc;
        let _: Off = b.swqos;
    }

    #[test]
    fn with_rpc_defaults_to_global() {
        let b = builder().with_rpc("tok");
        assert_eq!(b.rpc.token, "tok");
        assert_eq!(b.rpc.rpc_base, Region::Global.rpc_base());
        assert_eq!(b.rpc.ws_base, Region::Global.ws_base());
    }

    #[test]
    fn with_rpc_then_rpc_base() {
        let b = builder().with_rpc("tok").rpc_base("https://custom/sol");
        assert_eq!(b.rpc.rpc_base, "https://custom/sol");
    }

    #[test]
    fn with_rpc_then_ws_base() {
        let b = builder().with_rpc("tok").ws_base("wss://custom/ws");
        assert_eq!(b.rpc.ws_base, "wss://custom/ws");
    }

    #[test]
    fn with_rpc_then_rpc_region() {
        let b = builder().with_rpc("tok").rpc_region(Region::Ams);
        assert_eq!(b.rpc.rpc_base, Region::Ams.rpc_base());
        assert_eq!(b.rpc.ws_base, Region::Ams.ws_base());
    }

    #[test]
    fn with_grpc_defaults() {
        let b = builder().with_grpc("gt");
        assert_eq!(b.grpc.token, "gt");
        assert_eq!(b.grpc.url, Region::Global.grpc_url());
    }

    #[test]
    fn with_grpc_then_url() {
        let b = builder().with_grpc("gt").grpc_url("https://my.grpc");
        assert_eq!(b.grpc.url, "https://my.grpc");
    }

    #[test]
    fn with_grpc_then_region() {
        let b = builder().with_grpc("gt").grpc_region(Region::Fra);
        assert_eq!(b.grpc.url, Region::Fra.grpc_url());
    }

    #[test]
    fn with_swqos_defaults() {
        let b = builder().with_swqos("k");
        assert_eq!(b.swqos.key, "k");
        assert_eq!(b.swqos.endpoint, Region::Global.beam_endpoint());
    }

    #[test]
    fn skip_precheck_is_off_until_asked() {
        let b = builder().with_beam("k");
        assert!(!b.swqos.skip_precheck);
        let b = b.skip_precheck();
        assert!(b.swqos.skip_precheck);
    }

    #[test]
    fn with_swqos_then_endpoint() {
        let b = builder().with_swqos("k").beam_endpoint("custom:9000");
        assert_eq!(b.swqos.endpoint, "custom:9000");
    }

    #[test]
    fn with_swqos_then_region() {
        let b = builder().with_swqos("k").beam_region(Region::Nyc);
        assert_eq!(b.swqos.endpoint, Region::Nyc.beam_endpoint());
    }

    #[test]
    fn region_applies_to_all_enabled() {
        let b = builder()
            .with_rpc("t1")
            .with_grpc("t2")
            .with_swqos("k")
            .region(Region::Fra);
        assert_eq!(b.rpc.rpc_base, Region::Fra.rpc_base());
        assert_eq!(b.rpc.ws_base, Region::Fra.ws_base());
        assert_eq!(b.grpc.url, Region::Fra.grpc_url());
        assert_eq!(b.swqos.endpoint, Region::Fra.beam_endpoint());
    }

    #[test]
    fn region_on_empty_builder_is_noop() {
        let b = builder().region(Region::Nyc);
        let _: Off = b.rpc;
        let _: Off = b.grpc;
        let _: Off = b.swqos;
    }

    #[test]
    fn region_then_per_subsystem_override() {
        let b = builder()
            .with_rpc("t1")
            .with_grpc("t2")
            .region(Region::Ams)
            .grpc_region(Region::Nyc);
        assert_eq!(b.rpc.rpc_base, Region::Ams.rpc_base());
        assert_eq!(b.grpc.url, Region::Nyc.grpc_url());
    }

    #[test]
    fn apply_region_off_is_noop() {
        let mut off = Off;
        ApplyRegion::apply_region(&mut off, Region::Ams);
    }

    #[test]
    fn apply_region_rpc() {
        let mut cfg = RpcCfg {
            token: "t".into(),
            rpc_base: String::new(),
            ws_base: String::new(),
        };
        cfg.apply_region(Region::Fra);
        assert_eq!(cfg.rpc_base, Region::Fra.rpc_base());
        assert_eq!(cfg.ws_base, Region::Fra.ws_base());
    }

    #[test]
    fn apply_region_grpc() {
        let mut cfg = GrpcCfg {
            token: "t".into(),
            url: String::new(),
        };
        cfg.apply_region(Region::Nyc);
        assert_eq!(cfg.url, Region::Nyc.grpc_url());
    }

    #[test]
    fn apply_region_swqos() {
        let mut cfg = SwqosCfg {
            key: "k".into(),
            endpoint: String::new(),
        skip_precheck: false, };
        cfg.apply_region(Region::Ams);
        assert_eq!(cfg.endpoint, Region::Ams.beam_endpoint());
    }

    #[tokio::test]
    async fn off_connect_returns_off() {
        let result = Off.connect().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn empty_builder_builds_solami_off() {
        let s = builder().build().await.unwrap();
        let _: Off = s.rpc;
        let _: Off = s.grpc;
        let _: Off = s.swqos;
    }

    #[test]
    fn install_crypto_is_idempotent() {
        install_crypto();
        install_crypto();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn rpc_cfg_connect_fails_on_dead_ws() {
        let cfg = RpcCfg {
            token: "tok".into(),
            rpc_base: "http://127.0.0.1:1".into(),
            ws_base: "ws://127.0.0.1:1".into(),
        };
        let r = cfg.connect().await;
        assert!(r.is_err());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn grpc_cfg_connect_fails_on_dead() {
        let cfg = GrpcCfg {
            token: "tok".into(),
            url: "http://127.0.0.1:1".into(),
        };
        let r = cfg.connect().await;
        assert!(r.is_err());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn swqos_cfg_connect_fails_on_dead() {
        let cfg = SwqosCfg {
            key: crate::Keypair::new().to_base58_string(),
            endpoint: "127.0.0.1:1".into(),
        skip_precheck: false, };
        let r = tokio::time::timeout(std::time::Duration::from_secs(2), cfg.connect()).await;
        assert!(r.is_err() || r.unwrap().is_err());
    }
}
