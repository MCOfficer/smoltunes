use std::time::Duration;

use crate::status::StatusBuilder;
use crate::track_loading::{load_or_search, search_multiple, PREFERRED_SEARCH_ENGINES};
use crate::util::{check_if_in_channel, enqueue_tracks, source_to_emoji, TrackUserData};
use crate::*;
use crate::{util, Error};
use lavalink_rs::model::track::TrackData;
use poise::serenity_prelude as serenity;
use poise::serenity_prelude::{ButtonStyle, CreateActionRow, CreateButton, CreateMessage};
use poise::CreateReply;
use rand::seq::SliceRandom;

/// Play a song in the voice channel you are connected in.
#[poise::command(slash_command, prefix_command)]
pub async fn play(
    ctx: Context<'_>,
    #[description = "Search term or URL"]
    #[rest]
    term: Option<String>,
) -> Result<(), Error> {
    let guild_id = ctx.guild_id().unwrap();
    let player = util::join(&ctx, guild_id, None).await?;
    let lava_client = ctx.data().lavalink.clone();

    let Some(query) = term else {
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

    let mut playlist_info = None;
    let mut tracks: Vec<TrackData> = vec![];

    match load_or_search(lava_client.clone(), guild_id, &query).await? {
        TrackLoadData::Track(x) => tracks.push(x),
        TrackLoadData::Search(x) => {
            let first = x.first().ok_or_else(|| anyhow!("No search results"))?;
            tracks.push(first.clone())
        }
        TrackLoadData::Playlist(x) => {
            playlist_info = Some(x.info);
            tracks = x.tracks
        }
        TrackLoadData::Error(_) => {
            unreachable!("TrackLoadData::Error should be handled while loading/searching tracks")
        }
    };

    if let Some(info) = playlist_info {
        ctx.say(format!("Added playlist to queue: {}", info.name,))
            .await?;
    } else {
        ctx.send(CreateReply::default().embed(messages::added_to_queue(&tracks[0])))
            .await?;
    }

    let user_data = TrackUserData::new(ctx.author().id, query, guild_id);

    enqueue_tracks(player, tracks, user_data).await?;
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

    util::join(&ctx, guild_id, channel_id).await?;

    Ok(())
}

/// Leave the current voice channel.
#[poise::command(slash_command, prefix_command)]
pub async fn leave(ctx: Context<'_>) -> Result<(), Error> {
    let guild_id = ctx.guild_id().unwrap();
    let songbird = songbird::get(ctx.serenity_context()).await.unwrap().clone();
    let lava_client = ctx.data().lavalink.clone();

    util::leave(&lava_client, &songbird, guild_id).await?;

    ctx.say("BTW: I'm now leaving automatically when left alone :)")
        .await?;

    Ok(())
}

/// Add a song to the queue
#[poise::command(slash_command, prefix_command)]
pub async fn queue(ctx: Context<'_>) -> Result<(), Error> {
    let player = check_if_in_channel(ctx).await?;

    ctx.say(messages::queue_message(player).await?).await?;

    Ok(())
}

/// Print the current status (Playing Song + Queue).
#[poise::command(slash_command, prefix_command)]
pub async fn status(ctx: Context<'_>) -> Result<(), Error> {
    let player = check_if_in_channel(ctx).await?;

    let embeds = StatusBuilder::new(&player).await?.embeds().await;
    let reply = CreateReply {
        embeds,
        ..Default::default()
    };
    ctx.send(ctx.reply_builder(reply)).await?;

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
    util::join(&ctx, guild_id, None).await?;
    let player = check_if_in_channel(ctx).await?;

    let results: Vec<Vec<TrackData>> =
        search_multiple(lava_client, guild_id, &term, &PREFERRED_SEARCH_ENGINES)
            .await
            .into_iter()
            .filter_map(|r| r.ok())
            .map(|v| v.iter().take(3).cloned().collect())
            .collect();

    let mut action_rows = vec![];
    let mut i = 0;
    for source in &results {
        let mut buttons = vec![];
        for track in source {
            buttons.push(
                CreateButton::new(i.to_string())
                    .label((i + 1).to_string())
                    .emoji(source_to_emoji(&track.info.source_name))
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
            m.delete(&ctx).await?;
            return Ok(());
        }
    };

    let track = results
        .into_iter()
        .flatten()
        .nth(interaction.data.custom_id.parse::<usize>()?)
        .unwrap();

    m.delete(&ctx).await?;
    ctx.send(CreateReply::default().embed(messages::added_to_queue(&track)))
        .await?;

    let user_data = TrackUserData::new(ctx.author().id, term, guild_id);
    enqueue_tracks(player, [track], user_data).await?;

    Ok(())
}

/// Skip the current song.
#[poise::command(slash_command, prefix_command)]
pub async fn skip(ctx: Context<'_>) -> Result<(), Error> {
    let player = check_if_in_channel(ctx).await?;

    let now_playing = player.get_player().await?.track;
    if let Some(np) = now_playing {
        player.skip()?;
        ctx.say(format!("Skipped {}", np.info.title)).await?;
    } else {
        user_error!("nothing to skip!")
    }

    Ok(())
}

/// Pause the current song.
#[poise::command(slash_command, prefix_command)]
pub async fn pause(ctx: Context<'_>) -> Result<(), Error> {
    let player = check_if_in_channel(ctx).await?;

    player.set_pause(true).await?;

    ctx.say("Paused").await?;

    Ok(())
}

/// Resume playing the current song.
#[poise::command(slash_command, prefix_command)]
pub async fn resume(ctx: Context<'_>) -> Result<(), Error> {
    let player = check_if_in_channel(ctx).await?;

    player.set_pause(false).await?;
    ctx.say("Resumed playback").await?;

    Ok(())
}

/// Stops the playback of the current song.
#[poise::command(slash_command, prefix_command)]
pub async fn stop(ctx: Context<'_>) -> Result<(), Error> {
    let player = check_if_in_channel(ctx).await?;

    let now_playing = player.get_player().await?.track;
    if let Some(np) = now_playing {
        player.stop_now().await?;
        ctx.say(format!("Stopped {}", np.info.title)).await?;
    } else {
        user_error!("nothing to stop!")
    }

    Ok(())
}

/// Jump to a specific time in the song, in seconds.
#[poise::command(slash_command, prefix_command)]
pub async fn seek(
    ctx: Context<'_>,
    #[description = "Time to jump to (in seconds)"] time: u64,
) -> Result<(), Error> {
    let player = check_if_in_channel(ctx).await?;

    let now_playing = player.get_player().await?.track;
    if now_playing.is_some() {
        player.set_position(Duration::from_secs(time)).await?;
        ctx.say(format!("Jumped to {}s", time)).await?;
    } else {
        user_error!("nothing is playing!")
    }

    Ok(())
}

/// Remove a specific song from the queue.
#[poise::command(slash_command, prefix_command)]
pub async fn remove(
    ctx: Context<'_>,
    #[description = "Queue item index to remove"] index: usize,
) -> Result<(), Error> {
    let player = check_if_in_channel(ctx).await?;

    player.get_queue().remove(index)?;

    ctx.say("Removed successfully").await?;

    Ok(())
}

/// Shuffles the queue.
#[poise::command(slash_command, prefix_command)]
pub async fn shuffle(ctx: Context<'_>) -> Result<(), Error> {
    let player = check_if_in_channel(ctx).await?;

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
    let player = check_if_in_channel(ctx).await?;

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
    let player = check_if_in_channel(ctx).await?;
    let queue = player.get_queue();
    let queue_len = queue.get_count().await?;

    if index1 > queue_len || index2 > queue_len {
        user_error!("Maximum allowed index: {}", queue_len)
    } else if index1 == index2 {
        user_error!("Can't swap between the same indexes")
    }

    let track1 = queue.get_track(index1 - 1).await?.unwrap();
    let track2 = queue.get_track(index2 - 1).await?.unwrap();

    queue.swap(index1 - 1, track2)?;
    queue.swap(index2 - 1, track1)?;

    ctx.say("Swapped successfully").await?;

    Ok(())
}
