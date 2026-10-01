use std::fmt;

#[derive(Debug)]
pub enum SolamiError {
    Rpc(String),
    Grpc(String),
    Ws(String),
    Shred(String),
    Swqos(String),
    Api(String),
}

impl fmt::Display for SolamiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Rpc(msg) => write!(f, "RPC error: {msg}"),
            Self::Grpc(msg) => write!(f, "gRPC error: {msg}"),
            Self::Ws(msg) => write!(f, "WebSocket error: {msg}"),
            Self::Shred(msg) => write!(f, "Shred error: {msg}"),
            Self::Swqos(msg) => write!(f, "SWQOS error: {msg}"),
            Self::Api(msg) => write!(f, "API error: {msg}"),
        }
    }
}

impl std::error::Error for SolamiError {}

pub type Result<T> = std::result::Result<T, SolamiError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_all_variants() {
        assert_eq!(SolamiError::Rpc("x".into()).to_string(), "RPC error: x");
        assert_eq!(SolamiError::Grpc("x".into()).to_string(), "gRPC error: x");
        assert_eq!(SolamiError::Ws("x".into()).to_string(), "WebSocket error: x");
        assert_eq!(SolamiError::Shred("x".into()).to_string(), "Shred error: x");
        assert_eq!(SolamiError::Swqos("x".into()).to_string(), "SWQOS error: x");
        assert_eq!(SolamiError::Api("x".into()).to_string(), "API error: x");
    }

    #[test]
    fn debug_derive() {
        let err = SolamiError::Rpc("boom".into());
        assert!(format!("{err:?}").contains("Rpc"));
    }

    #[test]
    fn error_trait() {
        fn _accept<E: std::error::Error>(_: E) {}
        _accept(SolamiError::Api("x".into()));
    }
}
