use crate::*;
use futures::future::join_all;
use lavalink_rs::model::track::{Track, TrackData, TrackLoadType};
use poise_error::anyhow::bail;

static DEFAULT_SEARCH_ENGINE: SearchEngines = SearchEngines::YouTube;

pub async fn load_or_search(
    lavalink: LavalinkClient,
    guild_id: impl Into<GuildId>,
    query: String,
) -> Result<TrackLoadData> {
    let has_prefix = query.split_ascii_whitespace().next().unwrap().contains(":");
    let known_prefix = query.starts_with("http") || query.starts_with("mix:");
    let is_search_query = has_prefix && !known_prefix;

    if is_search_query {
        let vec = search_single(lavalink, guild_id, &query, &DEFAULT_SEARCH_ENGINE).await?;
        Ok(TrackLoadData::Search(vec))
    } else {
        load_direct(lavalink, guild_id, query)
            .await?
            .ok_or_else(|| anyhow!("No matches for identifier"))
    }
}

async fn load_direct(
    lavalink: LavalinkClient,
    guild_id: impl Into<GuildId>,
    query: String,
) -> Result<Option<TrackLoadData>> {
    let track = lavalink.load_tracks(guild_id, &query).await?;
    raise_for_load_type(track)
}

// TODO: cache
pub async fn search_multiple(
    lavalink: LavalinkClient,
    guild_id: impl Into<GuildId>,
    query: String,
    engines: &[SearchEngines],
) -> Vec<Result<Vec<TrackData>>> {
    let guild_id = guild_id.into();

    let futures = engines.iter().map(|e| async {
        let result = search_single(lavalink.clone(), guild_id, &query, e).await;
        if let Err(e) = &result {
            error!("While searching: {e:?}")
        }
        result
    });

    join_all(futures).await
}

pub async fn search_single(
    lavalink: LavalinkClient,
    guild_id: impl Into<GuildId>,
    query: &str,
    engine: &SearchEngines,
) -> Result<Vec<TrackData>> {
    let track = lavalink
        .load_tracks(guild_id, &engine.to_query(query)?)
        .await?;

    match raise_for_load_type(track)? {
        Some(TrackLoadData::Search(results)) => Ok(results),
        None => Ok(vec![]),
        _ => bail!("NotSearchResults"),
    }
}

fn raise_for_load_type(track: Track) -> Result<Option<TrackLoadData>> {
    match track.load_type {
        TrackLoadType::Error => {
            let TrackLoadData::Error(e) = track.data.expect("TrackLoadType::Error") else {
                panic!("expected TrackLoadData::Error due to TrackLoadType::Error")
            };
            bail!(
                "Error loading track ({}): {}\ncaused by: {}",
                e.severity,
                e.message,
                e.cause
            )
        }
        _ => Ok(track.data),
    }
}
