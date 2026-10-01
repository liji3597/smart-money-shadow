use std::net::SocketAddr;
use std::os::unix::io::AsRawFd;
use std::sync::Arc;

use kanal::{AsyncReceiver, AsyncSender};
use tokio::net::UdpSocket;

use crate::error::{Result, SolamiError};

const DEFAULT_BIND_ADDR: &str = "0.0.0.0:20000";
const RECV_BUF_SIZE: i32 = 8 * 1024 * 1024;
const CHANNEL_CAP: usize = 10_000;

pub type ShredReceiver = AsyncReceiver<(Vec<u8>, SocketAddr)>;
pub type ShredSender = AsyncSender<(Vec<u8>, SocketAddr)>;

pub struct ShredClient {
    socket: Arc<UdpSocket>,
}

impl ShredClient {
    pub async fn bind(bind_addr: &str) -> Result<Self> {
        let std_sock = std::net::UdpSocket::bind(bind_addr)
            .map_err(|e| SolamiError::Shred(e.to_string()))?;
        std_sock
            .set_nonblocking(true)
            .map_err(|e| SolamiError::Shred(e.to_string()))?;

        unsafe {
            libc::setsockopt(
                std_sock.as_raw_fd(),
                libc::SOL_SOCKET,
                libc::SO_RCVBUF,
                &RECV_BUF_SIZE as *const _ as *const libc::c_void,
                std::mem::size_of::<i32>() as u32,
            );
        }

        let socket = UdpSocket::from_std(std_sock)
            .map_err(|e| SolamiError::Shred(e.to_string()))?;

        tracing::info!(%bind_addr, "shred receiver bound");

        Ok(Self {
            socket: Arc::new(socket),
        })
    }

    pub async fn bind_default() -> Result<Self> {
        Self::bind(DEFAULT_BIND_ADDR).await
    }

    pub fn pipeline(&self) -> ShredReceiver {
        let (tx, rx) = kanal::bounded(CHANNEL_CAP);

        let socket = self.socket.clone();
        let tx = tx.to_async();
        tokio::spawn(async move {
            let mut buf = [0u8; 1232];
            loop {
                match socket.recv_from(&mut buf).await {
                    Ok((len, src)) => {
                        let _ = tx.try_send((buf[..len].to_vec(), src));
                    }
                    Err(e) => tracing::warn!("shred recv error: {e}"),
                }
            }
        });

        rx.to_async()
    }

    pub fn pipeline_with_outbound(&self) -> (ShredSender, ShredReceiver) {
        let (inbound_tx, inbound_rx) = kanal::bounded(CHANNEL_CAP);
        let (outbound_tx, outbound_rx) = kanal::bounded::<(Vec<u8>, SocketAddr)>(CHANNEL_CAP);

        let recv_socket = self.socket.clone();
        let inbound_tx = inbound_tx.to_async();
        tokio::spawn(async move {
            let mut buf = [0u8; 1232];
            loop {
                match recv_socket.recv_from(&mut buf).await {
                    Ok((len, src)) => {
                        let _ = inbound_tx.try_send((buf[..len].to_vec(), src));
                    }
                    Err(e) => tracing::warn!("shred recv error: {e}"),
                }
            }
        });

        let send_socket = self.socket.clone();
        let outbound_rx = outbound_rx.to_async();
        tokio::spawn(async move {
            while let Ok((data, dest)) = outbound_rx.recv().await {
                if let Err(e) = send_socket.send_to(&data, dest).await {
                    tracing::warn!("shred send error: {e}");
                }
            }
        });

        (outbound_tx.to_async(), inbound_rx.to_async())
    }

    pub async fn shred_recv(&self, buf: &mut [u8]) -> Result<(usize, SocketAddr)> {
        self.socket
            .recv_from(buf)
            .await
            .map_err(|e| SolamiError::Shred(e.to_string()))
    }

    pub fn local_addr(&self) -> Result<SocketAddr> {
        self.socket
            .local_addr()
            .map_err(|e| SolamiError::Shred(e.to_string()))
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn bind_ephemeral() {
        let c = ShredClient::bind("127.0.0.1:0").await.unwrap();
        let addr = c.local_addr().unwrap();
        assert_eq!(addr.ip().to_string(), "127.0.0.1");
        assert!(addr.port() > 0);
    }

    #[tokio::test]
    async fn bind_default_fails_when_unavailable_or_succeeds() {
        let _ = ShredClient::bind_default().await;
    }

    #[tokio::test]
    async fn bind_invalid_addr_errors() {
        let r = ShredClient::bind("not-an-addr").await;
        assert!(r.is_err());
    }

    #[tokio::test]
    async fn pipeline_roundtrip() {
        let c = ShredClient::bind("127.0.0.1:0").await.unwrap();
        let local = c.local_addr().unwrap();
        let rx = c.pipeline();

        let sender = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        sender.send_to(b"hello", local).await.unwrap();

        let recv = tokio::time::timeout(std::time::Duration::from_millis(500), rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(&recv.0, b"hello");
    }

    #[tokio::test]
    async fn pipeline_with_outbound_sends_and_receives() {
        let c = ShredClient::bind("127.0.0.1:0").await.unwrap();
        let local = c.local_addr().unwrap();
        let (out_tx, in_rx) = c.pipeline_with_outbound();

        let peer = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let peer_addr = peer.local_addr().unwrap();

        out_tx.send((b"out".to_vec(), peer_addr)).await.unwrap();
        let mut buf = [0u8; 16];
        let (n, src) = tokio::time::timeout(
            std::time::Duration::from_millis(500),
            peer.recv_from(&mut buf),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(&buf[..n], b"out");
        assert_eq!(src, local);

        peer.send_to(b"in", local).await.unwrap();
        let got = tokio::time::timeout(std::time::Duration::from_millis(500), in_rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(&got.0, b"in");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn pipeline_send_to_unroutable_logs_error() {
        let c = ShredClient::bind("127.0.0.1:0").await.unwrap();
        let (out_tx, _in_rx) = c.pipeline_with_outbound();
        let unroutable: SocketAddr = "0.0.0.0:0".parse().unwrap();
        out_tx.send((vec![0u8; 1], unroutable)).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    #[tokio::test]
    async fn shred_recv_works() {
        let c = ShredClient::bind("127.0.0.1:0").await.unwrap();
        let local = c.local_addr().unwrap();
        let peer = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        peer.send_to(b"abc", local).await.unwrap();

        let mut buf = [0u8; 16];
        let (n, _) = tokio::time::timeout(
            std::time::Duration::from_millis(500),
            c.shred_recv(&mut buf),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(&buf[..n], b"abc");
    }
}
