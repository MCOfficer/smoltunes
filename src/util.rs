use crate::*;
use lavalink_rs::player_context::PlayerContext;
use lavalink_rs::prelude::TrackInQueue;
use poise::serenity_prelude::{Color, Colour, EmojiIdentifier};
use poise_error::UserError;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::str::FromStr;

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

#[derive(Serialize, Deserialize)]
pub struct TrackUserData {
    pub requester_id: u64,
    pub text_channel: u64,
}

impl From<Context<'_>> for TrackUserData {
    fn from(ctx: Context) -> Self {
        Self {
            requester_id: ctx.author().id.into(),
            text_channel: ctx.channel_id().into(),
        }
    }
}

pub fn enqueue_tracks<I>(player: PlayerContext, tracks: I, user_data: TrackUserData) -> Result<()>
where
    I: Into<VecDeque<TrackInQueue>>,
{
    let mut tracks: VecDeque<TrackInQueue> = tracks.into();
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
