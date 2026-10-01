#[cfg(feature = "grpc")]
pub mod geyser {
    #![allow(clippy::all)]
    tonic::include_proto!("geyser");
}
#[cfg(feature = "grpc")]
pub mod solana {
    pub mod storage {
        pub mod confirmed_block {
            #![allow(clippy::all)]
            tonic::include_proto!("solana.storage.confirmed_block");
        }
    }
}

pub mod client;
pub mod config;
pub mod error;
#[cfg(feature = "grpc")]
pub mod grpc;
#[cfg(feature = "rpc")]
pub mod rpc;
#[cfg(feature = "shred")]
pub mod shred;
#[cfg(feature = "beam")]
pub mod swqos;
#[cfg(feature = "rpc")]
pub mod ws;

pub use client::{builder, Builder, Off, On, Solami};
pub use config::Region;
pub use error::{Result, SolamiError};

#[cfg(feature = "rpc")]
pub use client::{RpcCfg, RpcKit};
#[cfg(feature = "grpc")]
pub use client::GrpcCfg;
#[cfg(feature = "beam")]
pub use client::SwqosCfg;

#[cfg(feature = "grpc")]
pub use grpc::{
    BlurStream, DeshredTxFilter, GrpcClient, GrpcDeshredUpdate, GrpcDeshredUpdateKind, GrpcUpdate,
    GrpcUpdateKind, SubscribeRequestFilterAccounts, SubscribeRequestFilterBlocks,
    SubscribeRequestFilterSlots, SubscriptionBuilder, TxFilter, WebhookStream,
};
#[cfg(feature = "rpc")]
pub use rpc::{
    AccountPages, AccountsConfig, AddressHistoryConfig, AddressHistoryPage, Commitment, DataSlice,
    Encoding, Filter, Memcmp, RpcClient,
};
#[cfg(feature = "shred")]
pub use shred::ShredClient;
#[cfg(feature = "beam")]
pub use swqos::{build_tip_ix, fetch_tip_accounts, SwqosClient, TIP_ACCOUNTS};
#[cfg(feature = "rpc")]
pub use ws::WsClient;

pub use solana_instruction::Instruction;
pub use solana_pubkey::Pubkey;
pub use solana_keypair::Keypair;
pub use solana_signature::Signature;
pub use solana_transaction::{Transaction, versioned::VersionedTransaction};
pub use solana_system_interface::instruction as system_instruction;
#[cfg(feature = "grpc")]
pub use geyser::CommitmentLevel;
#[cfg(feature = "grpc")]
pub use geyser::{
    SubscribeBlurRequest, SubscribeDeshredRequest, SubscribeRequest, SubscribeUpdateBlur,
    SubscribeUpdateWebhook, SubscribeWebhookRequest,
};
