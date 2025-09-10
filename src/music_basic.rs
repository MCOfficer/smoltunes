use lavalink_rs::prelude::*;
use poise::{serenity_prelude as serenity, CreateReply};
use serenity::{model::id::ChannelId, Http};
use std::ops::Deref;

use crate::messages::added_to_queue;
use crate::Context;
use crate::Error;

pub(crate) async fn _join(
    ctx: &Context<'_>,
    guild_id: serenity::GuildId,
    channel_id: Option<serenity::ChannelId>,
) -> Result<bool, Error> {
    let lava_client = ctx.data().lavalink.clone();

    let manager = songbird::get(ctx.serenity_context()).await.unwrap().clone();

    if lava_client.get_player_context(guild_id).is_some() {
        // We are already connected to a channel
        // TODO: double check after connection lost
        return Ok(false);
    }

    let channel_id_from_user = || {
        let guild = ctx.guild().unwrap().deref().clone();
        guild
            .voice_states
            .get(&ctx.author().id)
            .and_then(|voice_state| voice_state.channel_id)
    };
    let channel_id = channel_id.or_else(channel_id_from_user);
    let connect_to = match channel_id {
        None => {
            ctx.say("Not in a voice channel").await?;
            return Err("Not in a voice channel".into());
        }
        Some(id) => id,
    };

    let handler = manager.join_gateway(guild_id, connect_to).await;

    match handler {
        Ok((connection_info, _)) => {
            lava_client
                // The turbofish here is Optional, but it helps to figure out what type to
                // provide in `PlayerContext::data()`
                //
                // While a tuple is used here as an example, you are free to use a custom
                // public structure with whatever data you wish.
                // This custom data is also present in the Client if you wish to have the
                // shared data be more global, rather than centralized to each player.
                .create_player_context_with_data::<(ChannelId, std::sync::Arc<Http>)>(
                    guild_id,
                    connection_info,
                    std::sync::Arc::new((ctx.channel_id(), ctx.serenity_context().http.clone())),
                )
                .await?;

            let tracks = lava_client
                .load_tracks(guild_id, "https://youtube.com/watch?v=WTWyosdkx44")
                .await?;
            if let Some(TrackLoadData::Track(data)) = tracks.data {
                lava_client
                    .get_player_context(guild_id)
                    .unwrap()
                    .play(&data)
                    .await?;
            }

            Ok(true)
        }
        Err(why) => {
            ctx.say(format!("Error joining the channel: {}", why))
                .await?;
            Err(why.into())
        }
    }
}

/// Play a song in the voice channel you are connected in.
#[poise::command(slash_command, prefix_command)]
pub async fn play(
    ctx: Context<'_>,
    #[description = "Search term or URL"]
    #[rest]
    term: Option<String>,
) -> Result<(), Error> {
    let guild_id = ctx.guild_id().unwrap();

    let has_joined = _join(&ctx, guild_id, None).await?;

    let lava_client = ctx.data().lavalink.clone();

    let Some(player) = lava_client.get_player_context(guild_id) else {
        ctx.say("Join the bot to a voice channel first.").await?;
        return Ok(());
    };

    let query = if let Some(term) = term {
        if term.starts_with("http") {
            term
        } else {
            SearchEngines::YouTube.to_query(&term)?
        }
    } else {
        if let Ok(player_data) = player.get_player().await {
            let queue = player.get_queue();

            if player_data.track.is_none() && queue.get_track(0).await.is_ok_and(|x| x.is_some()) {
                player.skip()?;
            } else {
                ctx.say("The queue is empty.").await?;
            }
        }

        return Ok(());
    };

    let loaded_tracks = lava_client.load_tracks(guild_id, &query).await?;

    let mut playlist_info = None;

    let mut tracks: Vec<TrackInQueue> = match loaded_tracks.data {
        Some(TrackLoadData::Track(x)) => vec![x.into()],
        Some(TrackLoadData::Search(x)) => vec![x[0].clone().into()],
        Some(TrackLoadData::Playlist(x)) => {
            playlist_info = Some(x.info);
            x.tracks.iter().map(|x| x.clone().into()).collect()
        }

        _ => {
            ctx.say(format!("{:?}", loaded_tracks)).await?;
            return Ok(());
        }
    };

    if let Some(info) = playlist_info {
        ctx.say(format!("Added playlist to queue: {}", info.name,))
            .await?;
    } else {
        let track = &tracks[0].track;
        ctx.send(CreateReply::default().embed(added_to_queue(track)))
            .await?;
    }

    for i in &mut tracks {
        i.track.user_data = Some(serde_json::json!({"requester_id": ctx.author().id.get()}));
    }

    let queue = player.get_queue();
    queue.append(tracks.into())?;

    if has_joined {
        return Ok(());
    }

    if let Ok(player_data) = dbg!(player.get_player().await) {
        dbg!(&player_data);
        if player_data.track.is_none() && queue.get_track(0).await.is_ok_and(|x| x.is_some()) {
            player.skip()?;
        }
    }

    Ok(())
}

/// Join the specified voice channel or the one you are currently in.
#[poise::command(slash_command, prefix_command)]
pub async fn join(
    ctx: Context<'_>,
    #[description = "The channel ID to join to."]
    #[channel_types("Voice")]
    channel_id: Option<serenity::ChannelId>,
) -> Result<(), Error> {
    let guild_id = ctx.guild_id().unwrap();

    _join(&ctx, guild_id, channel_id).await?;

    Ok(())
}

/// Leave the current voice channel.
#[poise::command(slash_command, prefix_command)]
pub async fn leave(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = ctx.guild_id().unwrap();

    let manager = songbird::get(ctx.serenity_context()).await.unwrap().clone();
    let lava_client = ctx.data().lavalink.clone();

    lava_client.delete_player(guild_id).await?;

    if manager.get(guild_id).is_some() {
        manager.remove(guild_id).await?;
    }

    ctx.say("Left voice channel.").await?;

    Ok(())
}
