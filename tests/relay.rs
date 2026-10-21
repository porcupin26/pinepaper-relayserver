use futures_util::{SinkExt, StreamExt};
use pinepaper_exchange::{router, Relay};
use serde_json::{json, Value};
use std::{net::SocketAddr, time::Duration};
use tokio::net::TcpStream;
use tokio_tungstenite::{connect_async, tungstenite::Message, MaybeTlsStream, WebSocketStream};
type Client = WebSocketStream<MaybeTlsStream<TcpStream>>;
async fn start() -> (String, Relay, tokio::task::JoinHandle<()>) {
    let relay = Relay::default();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("ws://{}/v1/exchange", listener.local_addr().unwrap());
    let app = router(relay.clone()).into_make_service_with_connect_info::<SocketAddr>();
    let task = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (url, relay, task)
}
async fn send(c: &mut Client, value: Value) {
    c.send(Message::Text(value.to_string().into()))
        .await
        .unwrap();
}
async fn recv(c: &mut Client) -> Value {
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            match c.next().await.unwrap().unwrap() {
                Message::Text(s) => return serde_json::from_str(&s).unwrap(),
                Message::Ping(_) => c.flush().await.unwrap(),
                other => panic!("Unexpected {other:?}"),
            }
        }
    })
    .await
    .unwrap()
}
async fn client(url: &str, code: Option<&str>) -> (Client, Value) {
    let (mut c, _) = connect_async(url).await.unwrap();
    send(
        &mut c,
        match code {
            Some(code) => json!({"type":"join","version":1,"code":code}),
            None => json!({"type":"create","version":1}),
        },
    )
    .await;
    let reply = recv(&mut c).await;
    (c, reply)
}
#[tokio::test]
async fn routes_only_to_live_room_peers_and_never_replays_documents() {
    let (url, relay, task) = start().await;
    let (mut a, aw) = client(&url, None).await;
    let code = aw["code"].as_str().unwrap();
    assert_eq!(code.len(), 5);
    assert!(code
        .bytes()
        .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()));
    let (mut b, bw) = client(&url, Some(code)).await;
    assert_eq!(bw["peers"][0], aw["peer"]);
    assert_eq!(recv(&mut a).await["type"], "peer_joined");
    let (mut isolated, _) = client(&url, None).await;
    send(
        &mut a,
        json!({"type":"relay","payload":{"secret":"one stroke"}}),
    )
    .await;
    let received = recv(&mut b).await;
    assert_eq!(received["from"], aw["peer"]);
    assert_eq!(received["payload"]["secret"], "one stroke");
    assert!(
        tokio::time::timeout(Duration::from_millis(80), recv(&mut a))
            .await
            .is_err()
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(80), recv(&mut isolated))
            .await
            .is_err()
    );
    let (mut c, cw) = client(&url, Some(code)).await;
    assert_eq!(cw["peers"].as_array().unwrap().len(), 2);
    recv(&mut a).await;
    recv(&mut b).await;
    assert!(
        tokio::time::timeout(Duration::from_millis(80), recv(&mut c))
            .await
            .is_err(),
        "Relay must not retain or replay prior document content"
    );
    send(
        &mut b,
        json!({"type":"relay","to":cw["peer"],"payload":{"snapshot":"client owned"}}),
    )
    .await;
    assert_eq!(recv(&mut c).await["payload"]["snapshot"], "client owned");
    assert!(
        tokio::time::timeout(Duration::from_millis(80), recv(&mut a))
            .await
            .is_err()
    );
    a.close(None).await.unwrap();
    assert_eq!(recv(&mut b).await["type"], "peer_left");
    recv(&mut c).await;
    send(
        &mut b,
        json!({"type":"relay","payload":"creator can leave"}),
    )
    .await;
    assert_eq!(recv(&mut c).await["payload"], "creator can leave");
    b.close(None).await.unwrap();
    recv(&mut c).await;
    c.close(None).await.unwrap();
    isolated.close(None).await.unwrap();
    for _ in 0..100 {
        if relay.room_count() == 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(relay.room_count(), 0);
    let (_, error) = client(&url, Some(code)).await;
    assert_eq!(error["code"], "room_not_found");
    task.abort();
}
#[tokio::test]
async fn rejects_bad_codes_versions_and_sender_spoofing() {
    let (url, _, task) = start().await;
    for code in ["../xx", "ABCDE1", "abcdE", ""] {
        let (_, reply) = client(&url, Some(code)).await;
        assert_eq!(reply["code"], "invalid_code");
    }
    let (mut c, _) = connect_async(&url).await.unwrap();
    send(&mut c, json!({"type":"create","version":99})).await;
    assert_eq!(recv(&mut c).await["code"], "invalid_hello");
    let (mut a, aw) = client(&url, None).await;
    let (mut b, _) = client(&url, aw["code"].as_str()).await;
    recv(&mut a).await;
    send(
        &mut b,
        json!({"type":"relay","from":aw["peer"],"payload":"forged"}),
    )
    .await;
    assert_eq!(recv(&mut a).await["type"], "peer_left");
    task.abort();
}
#[tokio::test]
async fn oversized_frame_disconnects_sender_without_forwarding() {
    let (url, _, task) = start().await;
    let (mut a, aw) = client(&url, None).await;
    let (mut b, _) = client(&url, aw["code"].as_str()).await;
    recv(&mut a).await;
    let _ = b
        .send(Message::Text(
            json!({"type":"relay","payload":"x".repeat(140*1024)})
                .to_string()
                .into(),
        ))
        .await;
    assert_eq!(recv(&mut a).await["type"], "peer_left");
    task.abort();
}

#[tokio::test]
async fn shutdown_disconnects_live_rooms_and_refuses_new_members() {
    let (url, relay, task) = start().await;
    let (mut a, _) = client(&url, None).await;
    relay.shutdown();
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            match a.next().await {
                Some(Ok(Message::Close(_))) | None => break,
                Some(Ok(Message::Ping(_))) => {
                    let _ = a.flush().await;
                }
                Some(Err(_)) => break,
                _ => {}
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(relay.room_count(), 0);
    let (_, reply) = client(&url, None).await;
    assert_eq!(reply["code"], "server_full");
    task.abort();
}
