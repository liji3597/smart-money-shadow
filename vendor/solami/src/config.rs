pub const DEFAULT_RPC_HOST: &str = "rpc.solami.dev";
pub const DEFAULT_GRPC_HOST: &str = "grpc.solami.dev";
pub const DEFAULT_BEAM_HOST: &str = "beam.solami.dev";
pub const DEFAULT_BEAM_PORT: u16 = 11000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Region {
    #[default]
    Global,
    Ams,
    Fra,
    Nyc,
}

impl Region {
    pub fn prefix(&self) -> &'static str {
        match self {
            Region::Global => "",
            Region::Ams => "ams.",
            Region::Fra => "fra.",
            Region::Nyc => "nyc.",
        }
    }

    pub fn rpc_base(&self) -> String {
        format!("https://{}{}/sol", self.prefix(), DEFAULT_RPC_HOST)
    }

    pub fn ws_base(&self) -> String {
        format!("wss://{}{}/ws/sol", self.prefix(), DEFAULT_RPC_HOST)
    }

    pub fn grpc_url(&self) -> String {
        format!("https://{}{}", self.prefix(), DEFAULT_GRPC_HOST)
    }

    pub fn beam_endpoint(&self) -> String {
        format!("{}{}:{}", self.prefix(), DEFAULT_BEAM_HOST, DEFAULT_BEAM_PORT)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefixes() {
        assert_eq!(Region::Global.prefix(), "");
        assert_eq!(Region::Ams.prefix(), "ams.");
        assert_eq!(Region::Fra.prefix(), "fra.");
        assert_eq!(Region::Nyc.prefix(), "nyc.");
    }

    #[test]
    fn all_rpc_bases() {
        assert_eq!(Region::Global.rpc_base(), "https://rpc.solami.dev/sol");
        assert_eq!(Region::Ams.rpc_base(), "https://ams.rpc.solami.dev/sol");
        assert_eq!(Region::Fra.rpc_base(), "https://fra.rpc.solami.dev/sol");
        assert_eq!(Region::Nyc.rpc_base(), "https://nyc.rpc.solami.dev/sol");
    }

    #[test]
    fn all_ws_bases() {
        assert_eq!(Region::Global.ws_base(), "wss://rpc.solami.dev/ws/sol");
        assert_eq!(Region::Ams.ws_base(), "wss://ams.rpc.solami.dev/ws/sol");
        assert_eq!(Region::Fra.ws_base(), "wss://fra.rpc.solami.dev/ws/sol");
        assert_eq!(Region::Nyc.ws_base(), "wss://nyc.rpc.solami.dev/ws/sol");
    }

    #[test]
    fn all_grpc_urls() {
        assert_eq!(Region::Global.grpc_url(), "https://grpc.solami.dev");
        assert_eq!(Region::Ams.grpc_url(), "https://ams.grpc.solami.dev");
        assert_eq!(Region::Fra.grpc_url(), "https://fra.grpc.solami.dev");
        assert_eq!(Region::Nyc.grpc_url(), "https://nyc.grpc.solami.dev");
    }

    #[test]
    fn all_beam_endpoints() {
        assert_eq!(Region::Global.beam_endpoint(), "beam.solami.dev:11000");
        assert_eq!(Region::Ams.beam_endpoint(), "ams.beam.solami.dev:11000");
        assert_eq!(Region::Fra.beam_endpoint(), "fra.beam.solami.dev:11000");
        assert_eq!(Region::Nyc.beam_endpoint(), "nyc.beam.solami.dev:11000");
    }

    #[test]
    fn default_is_global() {
        assert_eq!(Region::default(), Region::Global);
    }

    #[test]
    fn copy_eq_debug() {
        let a = Region::Ams;
        let b = a;
        assert_eq!(a, b);
        assert!(format!("{a:?}").contains("Ams"));
    }
}
