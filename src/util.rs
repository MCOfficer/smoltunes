use crate::*;
use lavalink_rs::player_context::PlayerContext;
use lavalink_rs::prelude::TrackInQueue;
use poise::serenity_prelude::{ChannelId, Color, Colour, EmojiIdentifier, Http};
use poise_error::UserError;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::ops::Deref;
use std::str::FromStr;
use std::sync::Arc;

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
}

pub(crate) async fn _join(
    ctx: &Context<'_>,
    guild_id: serenity::GuildId,
    channel_id: Option<serenity::ChannelId>,
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

#[derive(Serialize, Deserialize)]
pub struct TrackUserData {
    pub requester_id: UserId,
}

impl From<Context<'_>> for TrackUserData {
    fn from(ctx: Context) -> Self {
        Self {
            requester_id: ctx.author().id.into(),
        }
    }
}

pub fn enqueue_tracks<I, T>(
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
