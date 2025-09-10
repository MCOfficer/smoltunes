use std::ops::Deref;
use std::time::Duration;

use crate::messages::added_to_queue;
use crate::util::source_to_emoji;
use crate::Error;
use crate::{messages, Context};
use futures::future;
use futures::future::join_all;
use futures::stream::StreamExt;
use lavalink_rs::prelude::*;
use poise::serenity_prelude as serenity;
use poise::serenity_prelude::{ButtonStyle, CreateActionRow, CreateButton, CreateMessage};
use poise::CreateReply;
use rand::seq::SliceRandom;

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
                .create_player_context_with_data::<(serenity::ChannelId, std::sync::Arc<serenity::Http>)>(
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

/// Add a song to the queue
#[poise::command(slash_command, prefix_command)]
pub async fn queue(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = ctx.guild_id().unwrap();

    let lava_client = ctx.data().lavalink.clone();

    let Some(player) = lava_client.get_player_context(guild_id) else {
        ctx.say("Join the bot to a voice channel first.").await?;
        return Ok(());
    };

    let queue = player.get_queue();
    let player_data = player.get_player().await?;

    let max = queue.get_count().await?.min(9);

    let queue_message = queue
        .enumerate()
        .take_while(|(idx, _)| future::ready(*idx < max))
        .map(|(idx, x)| {
            if let Some(uri) = &x.track.info.uri {
                format!(
                    "{} -> [{} - {}](<{}>) | Requested by <@!{}>",
                    idx + 1,
                    x.track.info.author,
                    x.track.info.title,
                    uri,
                    x.track.user_data.unwrap()["requester_id"]
                )
            } else {
                format!(
                    "{} -> {} - {} | Requested by <@!{}",
                    idx + 1,
                    x.track.info.author,
                    x.track.info.title,
                    x.track.user_data.unwrap()["requester_id"]
                )
            }
        })
        .collect::<Vec<_>>()
        .await
        .join("\n");

    let now_playing_message = if let Some(track) = player_data.track {
        let time_s = player_data.state.position / 1000 % 60;
        let time_m = player_data.state.position / 1000 / 60;
        let time = format!("{:02}:{:02}", time_m, time_s);

        if let Some(uri) = &track.info.uri {
            format!(
                "Now playing: [{} - {}](<{}>) | {}, Requested by <@!{}>",
                track.info.author,
                track.info.title,
                uri,
                time,
                track.user_data.unwrap()["requester_id"]
            )
        } else {
            format!(
                "Now playing: {} - {} | {}, Requested by <@!{}>",
                track.info.author,
                track.info.title,
                time,
                track.user_data.unwrap()["requester_id"]
            )
        }
    } else {
        "Now playing: nothing".to_string()
    };

    ctx.say(format!("{}\n\n{}", now_playing_message, queue_message))
        .await?;

    Ok(())
}

/// Add a song to the queue
#[poise::command(slash_command, prefix_command)]
pub async fn search(
    ctx: Context<'_>,
    #[description = "Search term "]
    #[rest]
    term: String,
) -> Result<(), Error> {
    let guild_id = ctx.guild_id().unwrap();
    let lava_client = ctx.data().lavalink.clone();
    _join(&ctx, guild_id, None).await?;
    let Some(player) = lava_client.get_player_context(guild_id) else {
        ctx.say("Join the bot to a voice channel first.").await?;
        return Ok(());
    };

    let queries: Vec<String> = [
        SearchEngines::YouTube,
        SearchEngines::Deezer,
        SearchEngines::SoundCloud,
    ]
    .iter()
    .map(|e| e.to_query(&term).unwrap())
    .collect();

    let results: Vec<_> = join_all(queries.iter().map(|q| lava_client.load_tracks(guild_id, q)))
        .await
        .iter()
        .filter(|r| r.is_ok() && r.as_ref().unwrap().data.is_some())
        .map(|r| {
            if let Some(TrackLoadData::Search(results)) = &r.as_ref().unwrap().data {
                results.iter().take(3).cloned().collect()
            } else {
                vec![]
            }
        })
        .collect();

    let mut action_rows = vec![];
    let mut i = 0;
    for source in results.iter() {
        let mut buttons = vec![];
        for r in source {
            buttons.push(
                CreateButton::new(i.to_string())
                    .label((i + 1).to_string())
                    .emoji(source_to_emoji(&r.info.source_name))
                    .style(ButtonStyle::Secondary),
            );
            i += 1;
        }
        if !buttons.is_empty() {
            action_rows.push(CreateActionRow::Buttons(buttons));
        }
    }

    let m = ctx
        .channel_id()
        .send_message(
            &ctx,
            CreateMessage::new()
                .embed(messages::search_results(&results))
                .components(action_rows),
        )
        .await?;

    let interaction = match m
        .await_component_interaction(&ctx.serenity_context().shard)
        .timeout(Duration::from_secs(60))
        .await
    {
        Some(x) => x,
        None => {
            m.reply(&ctx, "Timed out").await.unwrap();

            return Ok(());
        }
    };

    let track = results
        .iter()
        .flatten()
        .nth(interaction.data.custom_id.parse::<usize>().unwrap())
        .unwrap();

    m.delete(&ctx).await?;
    ctx.send(CreateReply::default().embed(added_to_queue(track)))
        .await?;
    player.get_queue().push_to_back(track.clone())?;

    Ok(())
}

/// Skip the current song.
#[poise::command(slash_command, prefix_command)]
pub async fn skip(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = ctx.guild_id().unwrap();

    let lava_client = ctx.data().lavalink.clone();

    let Some(player) = lava_client.get_player_context(guild_id) else {
        ctx.say("Join the bot to a voice channel first.").await?;
        return Ok(());
    };

    let now_playing = player.get_player().await?.track;

    if let Some(np) = now_playing {
        player.skip()?;
        ctx.say(format!("Skipped {}", np.info.title)).await?;
    } else {
        ctx.say("Nothing to skip").await?;
    }

    Ok(())
}

/// Pause the current song.
#[poise::command(slash_command, prefix_command)]
pub async fn pause(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = ctx.guild_id().unwrap();

    let lava_client = ctx.data().lavalink.clone();

    let Some(player) = lava_client.get_player_context(guild_id) else {
        ctx.say("Join the bot to a voice channel first.").await?;
        return Ok(());
    };

    player.set_pause(true).await?;

    ctx.say("Paused").await?;

    Ok(())
}

/// Resume playing the current song.
#[poise::command(slash_command, prefix_command)]
pub async fn resume(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = ctx.guild_id().unwrap();

    let lava_client = ctx.data().lavalink.clone();

    let Some(player) = lava_client.get_player_context(guild_id) else {
        ctx.say("Join the bot to a voice channel first.").await?;
        return Ok(());
    };

    player.set_pause(false).await?;

    ctx.say("Resumed playback").await?;

    Ok(())
}

/// Stops the playback of the current song.
#[poise::command(slash_command, prefix_command)]
pub async fn stop(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = ctx.guild_id().unwrap();

    let lava_client = ctx.data().lavalink.clone();

    let Some(player) = lava_client.get_player_context(guild_id) else {
        ctx.say("Join the bot to a voice channel first.").await?;
        return Ok(());
    };

    let now_playing = player.get_player().await?.track;

    if let Some(np) = now_playing {
        player.stop_now().await?;
        ctx.say(format!("Stopped {}", np.info.title)).await?;
    } else {
        ctx.say("Nothing to stop").await?;
    }

    Ok(())
}

/// Jump to a specific time in the song, in seconds.
#[poise::command(slash_command, prefix_command)]
pub async fn seek(
    ctx: Context<'_>,
    #[description = "Time to jump to (in seconds)"] time: u64,
) -> Result<(), Error> {
    let guild_id = ctx.guild_id().unwrap();

    let lava_client = ctx.data().lavalink.clone();

    let Some(player) = lava_client.get_player_context(guild_id) else {
        ctx.say("Join the bot to a voice channel first.").await?;
        return Ok(());
    };

    let now_playing = player.get_player().await?.track;

    if now_playing.is_some() {
        player.set_position(Duration::from_secs(time)).await?;
        ctx.say(format!("Jumped to {}s", time)).await?;
    } else {
        ctx.say("Nothing is playing").await?;
    }

    Ok(())
}

/// Remove a specific song from the queue.
#[poise::command(slash_command, prefix_command)]
pub async fn remove(
    ctx: Context<'_>,
    #[description = "Queue item index to remove"] index: usize,
) -> Result<(), Error> {
    let guild_id = ctx.guild_id().unwrap();

    let lava_client = ctx.data().lavalink.clone();

    let Some(player) = lava_client.get_player_context(guild_id) else {
        ctx.say("Join the bot to a voice channel first.").await?;
        return Ok(());
    };

    player.get_queue().remove(index)?;

    ctx.say("Removed successfully").await?;

    Ok(())
}

/// Shuffles the queue.
#[poise::command(slash_command, prefix_command)]
pub async fn shuffle(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = ctx.guild_id().unwrap();

    let lava_client = ctx.data().lavalink.clone();

    let Some(player) = lava_client.get_player_context(guild_id) else {
        ctx.say("Join the bot to a voice channel first.").await?;
        return Ok(());
    };

    let mut queue = player
        .get_queue()
        .get_queue()
        .await?
        .make_contiguous()
        .to_vec();
    {
        let mut rng = rand::rng();
        queue.shuffle(&mut rng);
    }
    player.get_queue().replace(queue.into())?;

    ctx.say("Queue shuffled").await?;

    Ok(())
}

/// Clear the current queue.
#[poise::command(slash_command, prefix_command)]
pub async fn clear(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = ctx.guild_id().unwrap();

    let lava_client = ctx.data().lavalink.clone();

    let Some(player) = lava_client.get_player_context(guild_id) else {
        ctx.say("Join the bot to a voice channel first.").await?;
        return Ok(());
    };

    player.get_queue().clear()?;

    ctx.say("Queue cleared successfully").await?;

    Ok(())
}

/// Swap between 2 songs in the queue.
#[poise::command(slash_command, prefix_command)]
pub async fn swap(
    ctx: Context<'_>,
    #[description = "Queue item index to swap"] index1: usize,
    #[description = "The other queue item index to swap"] index2: usize,
) -> Result<(), Error> {
    let guild_id = ctx.guild_id().unwrap();

    let lava_client = ctx.data().lavalink.clone();

    let Some(player) = lava_client.get_player_context(guild_id) else {
        ctx.say("Join the bot to a voice channel first.").await?;
        return Ok(());
    };

    let queue = player.get_queue();
    let queue_len = queue.get_count().await?;

    if index1 > queue_len || index2 > queue_len {
        ctx.say(format!("Maximum allowed index: {}", queue_len))
            .await?;
        return Ok(());
    } else if index1 == index2 {
        ctx.say("Can't swap between the same indexes").await?;
        return Ok(());
    }

    let track1 = queue.get_track(index1 - 1).await?.unwrap();
    let track2 = queue.get_track(index1 - 2).await?.unwrap();

    queue.swap(index1 - 1, track2)?;
    queue.swap(index2 - 1, track1)?;

    ctx.say("Swapped successfully").await?;

    Ok(())
}
