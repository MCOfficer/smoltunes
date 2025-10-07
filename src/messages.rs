use crate::util::{format_millis, source_to_color, source_to_emoji, TrackUserData};
use crate::Error;
use futures::future;
use futures::StreamExt;
use lavalink_rs::model::track::{TrackData, TrackError};
use lavalink_rs::prelude::PlayerContext;
use poise::serenity_prelude::{CreateEmbed, CreateEmbedAuthor};

pub fn added_to_queue(track: &TrackData) -> CreateEmbed {
    let mut embed = CreateEmbed::new()
        .title(&track.info.title)
        .description("Added to queue")
        .author(
            CreateEmbedAuthor::new(&track.info.author)
                .icon_url(source_to_emoji(&track.info.source_name).url()),
        )
        .color(source_to_color(&track.info.source_name));
    if let Some(url) = &track.info.uri {
        embed = embed.url(url)
    }
    if let Some(img) = &track.info.artwork_url {
        embed = embed.image(img)
    }
    embed
}
pub fn recovered_with_alternative(
    track: &TrackData,
    error: &TrackError,
    alternatives: &[(f32, TrackData)],
) -> CreateEmbed {
    let best = &alternatives.first().unwrap().1.info;
    added_to_queue(track)
        .description(format!(
            "Error during playback, using alternative track:\n ** {} [{}] {} - {}**",
            source_to_emoji(&best.source_name),
            format_millis(best.length),
            best.author.replace("*", "\\*"),
            best.title.replace("*", "\\*"),
        ))
        .field(
            "Cause",
            format!(
                "```identifier: {}\nmessage: {}\ncause: {}```",
                track.info.identifier, error.message, error.cause
            ),
            false,
        )
        .field(
            "Top-scoring alternatives",
            alternatives
                .iter()
                .take(3)
                .fold("```".to_string(), |s, (score, t)| {
                    format!(
                        "{s}\n{score:07.3} [{}] {} - {}",
                        format_millis(t.info.length),
                        t.info.author,
                        t.info.title
                    )
                })
                + "```",
            false,
        )
}

pub fn search_results(results: &[Vec<TrackData>]) -> CreateEmbed {
    let mut description = String::default();
    let mut i = 0;
    for source in results.iter() {
        for r in source {
            description.push_str(&format!(
                "{} **{}**. `[{}]` {} - {}\n",
                source_to_emoji(&r.info.source_name),
                i + 1,
                format_millis(r.info.length),
                r.info.author,
                r.info.title
            ));

            i += 1;
        }
        description.push('\n');
    }

    CreateEmbed::new().description(description)
}

pub async fn queue_message(player: PlayerContext) -> Result<String, Error> {
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
                    TrackUserData::try_from(&x.track).unwrap().requester_id.0
                )
            } else {
                format!(
                    "{} -> {} - {} | Requested by <@!{}",
                    idx + 1,
                    x.track.info.author,
                    x.track.info.title,
                    TrackUserData::try_from(&x.track).unwrap().requester_id.0
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
                TrackUserData::try_from(&track)?.requester_id.0
            )
        } else {
            format!(
                "Now playing: {} - {} | {}, Requested by <@!{}>",
                track.info.author,
                track.info.title,
                time,
                TrackUserData::try_from(&track)?.requester_id.0
            )
        }
    } else {
        "Now playing: nothing".to_string()
    };

    Ok(format!("{}\n\n{}", now_playing_message, queue_message))
}
