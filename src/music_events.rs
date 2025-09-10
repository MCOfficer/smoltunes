use lavalink_rs::{hook, model::events, prelude::*};

// The #[hook] macro transforms:
// ```rs
// #[hook]
// async fn foo(a: A) -> T {
//     ...
// }
// ```
// into
// ```rs
// fn foo<'a>(a: A) -> Pin<Box<dyn Future<Output = T> + Send + 'a>> {
//     Box::pin(async move {
//         ...
//     })
// }
// ```
//
// This allows the asynchronous function to be stored in a structure.

#[hook]
pub async fn raw_event(_: LavalinkClient, session_id: String, event: &serde_json::Value) {
    if event["op"].as_str() == Some("event") || event["op"].as_str() == Some("playerUpdate") {
        info!("{:?} -> {:?}", session_id, event);
    }
}

#[hook]
pub async fn ready_event(client: LavalinkClient, session_id: String, event: &events::Ready) {
    client.delete_all_player_contexts().await.unwrap();
    info!("{:?} -> {:?}", session_id, event);
}
