use crate::*;
use futures::future::join_all;
use lavalink_rs::model::track::{Track, TrackData, TrackLoadType};
use poise_error::anyhow::bail;
use retainer::Cache;
use std::sync::{Arc, LazyLock};
use std::time::Duration;

static DEFAULT_SEARCH_ENGINE: SearchEngines = SearchEngines::YouTube;

static SEARCH_CACHE: LazyLock<Arc<Cache<String, Vec<TrackData>>>> = LazyLock::new(|| {
    let cache = Arc::new(Cache::new());

    let clone = cache.clone();
    tokio::spawn(async move { clone.monitor(64, 0.01, Duration::from_secs(60)).await });

    cache
});

pub fn is_search_query(term: &str) -> bool {
    let has_prefix = term
        .split_ascii_whitespace()
        .next()
        .unwrap_or_default()
        .contains(":");
    let known_prefix = term.starts_with("http") || term.starts_with("mix:");
    has_prefix && !known_prefix
}

pub async fn load_or_search(
    lavalink: LavalinkClient,
    guild_id: impl Into<GuildId>,
    term: &str,
) -> Result<TrackLoadData> {
    if is_search_query(term) {
        let vec = search_single(lavalink, guild_id, term, &DEFAULT_SEARCH_ENGINE).await?;
        Ok(TrackLoadData::Search(vec))
    } else {
        load_direct(lavalink, guild_id, term)
            .await?
            .ok_or_else(|| anyhow!("No matches for identifier"))
    }
}

async fn load_direct(
    lavalink: LavalinkClient,
    guild_id: impl Into<GuildId>,
    identifier: &str,
) -> Result<Option<TrackLoadData>> {
    let track = lavalink.load_tracks(guild_id, &identifier).await?;
    raise_for_load_type(track)
}

pub async fn search_multiple(
    lavalink: LavalinkClient,
    guild_id: impl Into<GuildId>,
    term: &str,
    engines: &[SearchEngines],
) -> Vec<Result<Vec<TrackData>>> {
    let guild_id = guild_id.into();

    let futures = engines.iter().map(|e| async {
        let result = search_single(lavalink.clone(), guild_id, &term, e).await;
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
    term: &str,
    engine: &SearchEngines,
) -> Result<Vec<TrackData>> {
    let query = engine.to_query(term)?;
    if let Some(guard) = SEARCH_CACHE.get(&query).await {
        return Ok(guard.clone());
    }

    let track = lavalink.load_tracks(guild_id, &query).await?;

    let results = match raise_for_load_type(track)? {
        Some(TrackLoadData::Search(results)) => results,
        None => vec![],
        _ => bail!("NotSearchResults"),
    };
    SEARCH_CACHE
        .insert(query, results.clone(), Duration::from_secs(60 * 60 * 3))
        .await;

    Ok(results)
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
