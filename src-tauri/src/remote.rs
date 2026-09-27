//! Remote monitoring server: streams `TrafficTick` JSON over WebSocket.
//!
//! Protocol (deliberately tiny): client connects, sends the token as its
//! first text message, then receives one JSON tick per second. Binds
//! localhost by default; LAN bind is opt-in via settings.

use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio::sync::broadcast;

pub async fn serve(
    listener: tokio::net::TcpListener,
    token: String,
    rx: broadcast::Sender<String>,
    peers: Arc<AtomicUsize>,
) {
    loop {
        let Ok((stream, _)) = listener.accept().await else {
            break;
        };
        let mut rx = rx.subscribe();
        let token = token.clone();
        let peers = peers.clone();
        tokio::spawn(async move {
            let Ok(ws) = tokio_tungstenite::accept_async(stream).await else {
                return;
            };
            let (mut w, mut r) = ws.split();
            let authed =
                match tokio::time::timeout(Duration::from_secs(10), r.next()).await {
                    Ok(Some(Ok(tokio_tungstenite::tungstenite::Message::Text(t)))) => {
                        t.trim() == token
                    }
                    _ => false,
                };
            if !authed {
                let _ = w.close().await;
                return;
            }
            peers.fetch_add(1, Ordering::SeqCst);
            let _ = w
                .send(tokio_tungstenite::tungstenite::Message::Text(
                    "{\"hello\":\"netwire\"}".into(),
                ))
                .await;
            loop {
                match rx.recv().await {
                    Ok(m) => {
                        if w
                            .send(tokio_tungstenite::tungstenite::Message::Text(
                                m.into(),
                            ))
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(_) => break,
                }
            }
            peers.fetch_sub(1, Ordering::SeqCst);
        });
    }
}
