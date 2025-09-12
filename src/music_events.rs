use crate::util::{PlayerContextData, TrackUserData};
use crate::*;
use lavalink_rs::model::events::TrackException;
use lavalink_rs::{hook, model::events};
use poise::serenity_prelude::{Colour, CreateEmbed, CreateEmbedAuthor, CreateMessage};
use std::sync::Arc;
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
pub async fn raw(_: LavalinkClient, session_id: String, event: &serde_json::Value) {
    if event["op"].as_str() == Some("event") || event["op"].as_str() == Some("playerUpdate") {
        info!("{:?} -> {:?}", session_id, event);
    }
}

#[hook]
pub async fn ready(client: LavalinkClient, session_id: String, event: &events::Ready) {
    client.delete_all_player_contexts().await.unwrap();
    info!("{:?} -> {:?}", session_id, event);
}

#[hook]
pub async fn track_exception(
    client: LavalinkClient,
    session_id: String,
    exception: &TrackException,
) {
    if let Err(e) = _track_exception(client, session_id, exception).await {
        error!("Failed to notify about exception: {e:#?}")
    }
}

async fn _track_exception(
    client: LavalinkClient,
    _session_id: String,
    exception: &TrackException,
) -> Result<()> {
    let TrackException {
        track,
        guild_id,
        exception,
        ..
    } = exception;
    let player = client.get_player_context(*guild_id).unwrap();
    let _user_data: TrackUserData = serde_json::from_value(
        track
            .user_data
            .clone()
            .with_context(|| "player context without user data")?,
    )?;
    let player_data: Arc<PlayerContextData> = player.data()?;

    let embed = CreateEmbed::new()
        .author(CreateEmbedAuthor::new("Error during Playback"))
        .color(Colour::GOLD)
        .title(format!("{} - {}", track.info.author, track.info.title))
        .description(format!(
            "{} exception during playback:\n```identifier: {}\nmessage: {}\ncause: {}```",
            track.info.identifier, exception.severity, exception.message, exception.cause
        ));
    player_data
        .text_channel
        .send_message(player_data.http.clone(), CreateMessage::new().embed(embed))
        .await
        .ok();

    Ok(())
}
