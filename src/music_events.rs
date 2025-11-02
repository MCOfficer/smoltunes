use crate::player_controller::PlayerController;
use crate::util::find_alternative_tracks;
use crate::*;
use lavalink_rs::model::events::TrackException;
use lavalink_rs::model::http::UpdatePlayer;
use lavalink_rs::{hook, model::events};
use poise::serenity_prelude::{
    Cache, Colour, CreateEmbed, CreateEmbedAuthor, CreateMessage, VoiceServerUpdateEvent,
    VoiceState,
};
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
        debug!("{:?} -> {:?}", session_id, event);
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
    let player = client.get_player_context(exception.guild_id).unwrap();

    // This is not ideal, but we need to stop the player before it skips to the next track.
    // At least it was reliable in testing..?
    if let Err(e) = player.stop_now().await {
        error!("Failed to stop player on track exception, skipping recovery: {e:#?}");
        return;
    }

    if let Err(e) = _track_exception(client, &player, session_id, exception).await {
        error!("Failed to notify about exception: {e:#?}")
    }

    // At this point the player is stopped with no track, skipping resumes playback from the queue
    if let Err(e) = player.skip() {
        error!("Failed to skip after recovering from exception: {e:#?}");
    };
}

async fn _track_exception(
    client: LavalinkClient,
    player: &PlayerContext,
    _session_id: String,
    exception: &TrackException,
) -> Result<()> {
    error!(
        "Failed to playback {}: {:?}",
        exception.track.info.identifier, exception.exception
    );

    let TrackException {
        track, exception, ..
    } = exception;
    debug!(
        "player: {:#?}\nqueue: {:#?}",
        player.get_player().await?,
        player.get_queue().get_queue().await?
    );
    let player_data = PlayerController::from(player);

    let alternatives = find_alternative_tracks(client, track).await;
    dbg!(&alternatives);
    if !alternatives.is_empty() {
        let best = alternatives.first().unwrap().1.clone();
        let embed = messages::recovered_with_alternative(track, exception, &alternatives);
        info!("Queueing alternative track");
        player.get_queue().push_to_front(best)?;
        player_data
            .text_channel
            .send_message(player_data.http.clone(), CreateMessage::new().embed(embed))
            .await?;
        return Ok(());
    }

    let embed = CreateEmbed::new()
        .author(CreateEmbedAuthor::new("Error during Playback"))
        .color(Colour::GOLD)
        .title(format!("{} - {}", track.info.author, track.info.title))
        .description(format!(
            "{} exception during playback:\n```identifier: {}\nmessage: {}\ncause: {}```",
            exception.severity, track.info.identifier, exception.message, exception.cause
        ));
    player_data
        .text_channel
        .send_message(player_data.http.clone(), CreateMessage::new().embed(embed))
        .await?;

    Ok(())
}

pub enum VoiceChange<'a> {
    State(&'a VoiceState),
    Server(&'a VoiceServerUpdateEvent),
}

pub async fn handle_voice_changes(
    lavalink: &LavalinkClient,
    change: VoiceChange<'_>,
    cache: &Arc<Cache>,
) -> Result<()> {
    let guild_id = match change {
        VoiceChange::State(VoiceState {
            session_id,
            channel_id,
            guild_id: Some(guild_id),
            user_id,
            ..
        }) => {
            if user_id != &cache.current_user().id {
                return Ok(());
            }
            lavalink.handle_voice_state_update(
                *guild_id,
                *channel_id,
                *user_id,
                session_id.clone(),
            );
            *guild_id
        }
        VoiceChange::Server(VoiceServerUpdateEvent {
            guild_id: Some(guild_id),
            endpoint,
            token,
            ..
        }) => {
            lavalink.handle_voice_server_update(*guild_id, token.clone(), endpoint.clone());
            *guild_id
        }
        _ => {
            return Ok(());
        }
    };

    let conn = lavalink
        .get_connection_info(guild_id, Duration::from_secs(3))
        .await?;
    lavalink
        .update_player(
            guild_id,
            &UpdatePlayer {
                voice: Some(conn),
                ..Default::default()
            },
            false,
        )
        .await?;

    Ok(())
}
