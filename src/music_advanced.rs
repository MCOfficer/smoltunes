use std::time::Duration;

use crate::messages::added_to_queue;
use crate::util::source_to_emoji;
use crate::Error;
use crate::{messages, Context};
use futures::future;
use futures::future::join_all;
use futures::stream::StreamExt;
use lavalink_rs::prelude::{SearchEngines, TrackLoadData};
use poise::serenity_prelude::{ButtonStyle, CreateActionRow, CreateButton, CreateMessage};
use poise::CreateReply;
use rand::seq::SliceRandom;
use rand::thread_rng;

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
    crate::music_basic::_join(&ctx, guild_id, None).await?;
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
        let mut rng = thread_rng();
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
