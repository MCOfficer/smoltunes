use crate::title_parse::guess_search_query;
use crate::track_loading::{is_direct_query, search_multiple, PREFERRED_SEARCH_ENGINES};
use crate::*;
use derive_new::new;
use itertools::Itertools;
use lavalink_rs::model::track::{TrackData, TrackInfo};
use lavalink_rs::player_context::PlayerContext;
use lavalink_rs::prelude::TrackInQueue;
use poise::serenity_prelude::{ChannelId, Color, Colour, EmojiIdentifier, Http};
use poise_error::UserError;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::ops::Deref;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tuples::TupleIntoIter;

#[macro_export]
macro_rules! user_error {
    ($msg:literal $(,)?) => {
        poise_error::anyhow::bail!(poise_error::UserError(poise_error::anyhow::anyhow!($msg)))
    };
    ($err:expr $(,)?) => {
        poise_error::anyhow::bail!(poise_error::UserError(poise_error::anyhow::anyhow!($err)))
    };
    ($fmt:expr, $($arg:tt)*) => {
        poise_error::anyhow::bail!(poise_error::UserError(poise_error::anyhow::anyhow!($fmt, $($arg)*)))
    };
}

pub struct PlayerContextData {
    pub text_channel: ChannelId,
    pub http: Arc<Http>,
    pub cache: Arc<SerenityCache>,
}

impl PlayerContextData {
    pub fn from(ctx: &PlayerContext) -> Arc<PlayerContextData> {
        ctx.data().expect("Failed to get PlayerContextData")
    }
}

pub(crate) async fn _join(
    ctx: &Context<'_>,
    guild_id: serenity::GuildId,
    channel_id: Option<ChannelId>,
) -> Result<PlayerContext, Error> {
    let lava_client = ctx.data().lavalink.clone();

    let manager = songbird::get(ctx.serenity_context()).await.unwrap().clone();

    if let Some(ctx) = lava_client.get_player_context(guild_id) {
        // We are already connected to a channel
        // TODO: double check after connection lost
        return Ok(ctx);
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
            user_error!("Not in a voice channel!");
        }
        Some(id) => id,
    };

    let (connection_info, _) = manager
        .join_gateway(guild_id, connect_to)
        .await
        .with_context(|| "Failed to join voice channel")?;

    let data = PlayerContextData {
        text_channel: ctx.channel_id(),
        http: ctx.serenity_context().http.clone(),
        cache: ctx.serenity_context().cache.clone(),
    };
    let ctx = lava_client
        .create_player_context_with_data(guild_id, connection_info, Arc::new(data))
        .await?;

    // TODO more reliable join announcement
    let tracks = lava_client
        .load_tracks(guild_id, "https://youtube.com/watch?v=WTWyosdkx44")
        .await?;
    if let Some(TrackLoadData::Track(data)) = tracks.data {
        ctx.play(&data).await?;
    }

    Ok(ctx)
}

#[derive(Serialize, Deserialize, new)]
pub struct TrackUserData {
    #[new(into)]
    pub requester_id: UserId,
    pub user_query: String,
    #[new(into)]
    pub guild_id: GuildId,
}

impl TryFrom<&TrackData> for TrackUserData {
    type Error = Error;

    fn try_from(td: &TrackData) -> std::result::Result<TrackUserData, Error> {
        serde_json::from_value(
            td.user_data
                .clone()
                .with_context(|| "TrackData without user_data")?,
        )
        .with_context(|| "Failed to deserialize TrackUserData")
    }
}

pub async fn enqueue_tracks<I, T>(
    player: PlayerContext,
    tracks: I,
    user_data: TrackUserData,
) -> Result<()>
where
    I: IntoIterator<Item = T>,
    T: Into<TrackInQueue>,
{
    let mut tracks: VecDeque<TrackInQueue> = tracks.into_iter().map(|t| t.into()).collect();
    for tiq in &mut tracks {
        tiq.track.user_data = Some(serde_json::to_value(&user_data)?);
    }

    if player.get_player().await?.track.is_none() {
        let first = &tracks
            .remove(0)
            .with_context(|| anyhow!("tried to queue empty list"))?
            .track;
        player.play(first).await?;
    }
    player.get_queue().append(tracks)?;

    Ok(())
}

pub fn format_millis(millis: u64) -> String {
    let hours = millis / 1_000 / 60 / 60;
    let minutes = millis / 1_000 / 60 % 60;
    let seconds = millis / 1_000 % 60;

    let hours = if hours > 0 {
        format!("{:0>2}:", hours)
    } else {
        "".into()
    };

    format!("{hours}{:0>2}:{:0>2}", minutes, seconds)
}

pub fn source_to_emoji(source: &str) -> EmojiIdentifier {
    if source == "youtube" {
        EmojiIdentifier::from_str("<:youtube:1290422789899157546>").unwrap()
    } else if source == "deezer" {
        EmojiIdentifier::from_str("<:deezer:1290423677913006090>").unwrap()
    } else if source == "soundcloud" {
        EmojiIdentifier::from_str("<:soundcloud:1290423857336811612>").unwrap()
    } else if source == "spotify" {
        EmojiIdentifier::from_str("<:spotify:1366886498170961992>").unwrap()
    } else {
        EmojiIdentifier::from_str("<:thonk:464380571628339210>").unwrap()
    }
}

pub fn source_to_color(source: &str) -> Color {
    if source == "youtube" {
        Colour::from(0xff0000)
    } else if source == "deezer" {
        Colour::from(0xa238ff)
    } else if source == "soundcloud" {
        Colour::from(0xf15e22)
    } else if source == "spotify" {
        Colour::from(0x1ED760)
    } else {
        Colour::from(0x23272A)
    }
}

pub async fn check_if_in_channel(ctx: Context<'_>) -> Result<PlayerContext, Error> {
    ctx.data()
        .lavalink
        .get_player_context(ctx.guild_id().unwrap())
        .ok_or_else(|| anyhow!(UserError(anyhow!("Not in a voice channel!"))))
}

pub async fn find_alternative_tracks(
    lavalink: LavalinkClient,
    track: &TrackData,
) -> Vec<(f32, TrackData)> {
    let original_info = &track.info;
    let original_user_data = TrackUserData::try_from(track).unwrap();

    let is_direct_query = is_direct_query(&original_user_data.user_query);

    let mut queries = if is_direct_query {
        search_queries_from_track(original_info)
    } else {
        vec![original_user_data.user_query]
    };

    // Keep searching until we get decent results (score >= -5)
    let mut scored = vec![];
    while scored.iter().all(|(score, _)| *score < -5.) {
        let Some(query) = queries.pop() else { break };
        let search_results: Vec<_> = search_multiple(
            lavalink.clone(),
            original_user_data.guild_id,
            &query,
            &PREFERRED_SEARCH_ENGINES,
        )
        .await
        .into_iter()
        .filter_map(|r| r.ok())
        .collect();
        scored.extend(score_alternatives(search_results, original_info));
    }

    scored
        .into_iter()
        .unique_by(|(_, t)| {
            t.info
                .uri
                .clone()
                .unwrap_or_else(|| t.info.identifier.clone())
        })
        .collect()
}

fn search_queries_from_track(info: &TrackInfo) -> Vec<String> {
    if info.source_name == "deezer" || info.source_name == "spotify" {
        // TODO: spotify source plays from youtube. How do we prevent failing on the same track again?
        return vec![format!("{} {}", info.author, info.title)];
    }

    let guesses = guess_search_query(&info.author, &info.title, info.length as usize).guesses;

    let guesses_fmt = guesses
        .iter()
        .map(|g| {
            format!(
                "{:07.3} {: >32} - {}",
                g.confidence, g.components.0, g.components.1,
            )
        })
        .join("\n");
    debug!(
        "Guessed query from info \"{} ||| {}\":\n{}",
        info.author, &info.title, guesses_fmt
    );

    guesses
        .into_iter()
        .filter(|g| g.confidence > -100.0) // TODO
        .map(|g| g.components.into_iter().join(" "))
        .take(3)
        .collect()
}

fn score_alternatives(
    search_results: Vec<Vec<TrackData>>,
    original_info: &TrackInfo,
) -> Vec<(f32, TrackData)> {
    let mut scored_tracks: Vec<(f32, TrackData)> = vec![];

    for results in search_results {
        let mut scored = results
            .into_iter()
            .filter(|t| &t.info != original_info)
            .enumerate()
            .map(|(i, t)| (score_track(&t.info, original_info, i), t))
            .collect();
        scored_tracks.append(&mut scored)
    }

    // Reverse sort (higher score = first)
    scored_tracks.sort_by(|(a, _), (b, _)| b.total_cmp(a));

    let format_scored = |score: f32, info: &TrackInfo| {
        format!(
            "{score:07.3} {: >9} {:݁>12} {: >32} - {}",
            format_millis(info.length),
            info.source_name,
            info.author,
            info.title
        )
    };
    let scored_debug: Vec<_> = scored_tracks
        .iter()
        .map(|(score, track)| format_scored(*score, &track.info))
        .collect();
    debug!(
        "Scored search results:\n{}\n{}",
        format_scored(0_f32, original_info),
        scored_debug.join("\n")
    );

    scored_tracks
}

fn score_track(info: &TrackInfo, original: &TrackInfo, position: usize) -> f32 {
    let mut score = 50.;

    if info.isrc.is_some() && info.isrc == original.isrc {
        score += 20.;
    }

    if info.source_name == original.source_name {
        score -= 3.;
    }

    // Source bias
    score += match info.source_name.as_str() {
        "deezer" => 0.5,
        _ => 0.,
    };

    // Penalize Tracks that differ in duration from the original. This is the main factor in reliable matching,
    // since I've found no string comparison algorithm that works reliably for all query/title combinations.
    // See desmos.com: 0.5∴~1, 1∴~2, 2∴~3.8, 3∴~5.4, 5∴~8.2 10∴~13, 20∴~17.5, 40∴~19.7, y->20
    let penalize_duration = |seconds| (-20. * 0.9_f32.powf(seconds)) + 20.;
    let delta = Duration::from_millis(info.length)
        .abs_diff(Duration::from_millis(original.length))
        .as_secs_f32();
    if delta > 0.3 {
        score -= penalize_duration(delta);
    }

    // How much search results should be penalized for being lower in the list.
    // This basically correlates with how many "correct" results we expect to get from a platform
    let position_multiplier = match info.source_name.as_str() {
        "youtube" => 1,
        "soundcloud" => 2,
        "deezer" => 3,
        _ => 3,
    };
    score -= position as f32 * position_multiplier as f32 * 0.5;

    score
}
