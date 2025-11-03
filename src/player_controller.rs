use crate::util::{get_own_voice_channel, leave};
use crate::*;
use chrono::{DateTime, TimeDelta, Utc};
use parking_lot::Mutex;
use poise::serenity_prelude::{ChannelId, Http};
use songbird::Songbird;
use std::num::NonZeroU64;
use std::ops::Sub;
use std::sync::Arc;

pub struct PlayerData {
    pub lavalink: LavalinkClient,
    pub text_channel: ChannelId,
    pub http: Arc<Http>,
    pub cache: Arc<SerenityCache>,
    pub songbird: Arc<Songbird>,
    pub guild_id: GuildId,
    pub alone_since: Mutex<Option<DateTime<Utc>>>,
}

impl PlayerData {
    pub fn from(ctx: &PlayerContext) -> Arc<PlayerData> {
        ctx.data().expect("Failed to get PlayerContextData")
    }
    pub fn mark_alone(&self) {
        let mut guard = self.alone_since.lock();
        if guard.is_none() {
            debug!("Marking player as alone");
            *guard = Some(Utc::now())
        }
    }

    pub fn reset_alone(&self) {
        debug!("Resetting player's alone marker");
        self.alone_since.lock().take();
    }

    pub fn is_alone_for(&self, delta: TimeDelta) -> bool {
        self.alone_since
            .lock()
            .is_some_and(|ts| delta < Utc::now().sub(ts))
    }

    async fn player_watchdog(self: Arc<Self>) {
        loop {
            // Give the player time to initialize
            tokio::time::sleep(Duration::from_secs(10)).await;

            if self.lavalink.get_player_context(self.guild_id).is_none() {
                break; // Player has quit
            };

            let channel = get_own_voice_channel(&self.cache, self.guild_id.0).unwrap();
            let members = channel.members(&self.cache).unwrap();

            if members.len() > 1 {
                self.reset_alone();
            } else if self.is_alone_for(TimeDelta::minutes(3)) {
                leave(&self.lavalink, &self.songbird, self.guild_id)
                    .await
                    .unwrap();
            } else {
                self.mark_alone();
            }
        }
    }
}

pub struct PlayerController {
    pub data: Arc<PlayerData>,
    pub ctx: PlayerContext,
}

impl PlayerController {
    pub async fn init(
        ctx: &Context<'_>,
        lavalink: &LavalinkClient,
        songbird: Arc<Songbird>,
        vc_id: ChannelId,
    ) -> Result<PlayerController> {
        let data = Arc::new(PlayerData {
            lavalink: lavalink.clone(),
            text_channel: ctx.channel_id(),
            http: ctx.serenity_context().http.clone(),
            cache: ctx.serenity_context().cache.clone(),
            songbird,
            guild_id: ctx.guild_id().unwrap().into(),
            alone_since: Mutex::new(None),
        });
        let guild_id = data.guild_id;

        let (connection_info, _) = data
            .songbird
            .join_gateway(NonZeroU64::new(guild_id.0).unwrap(), vc_id)
            .await
            .with_context(|| "Failed to join voice channel")?;

        let player_context = data
            .lavalink
            .create_player_context_with_data(guild_id, connection_info, data.clone())
            .await?;

        tokio::spawn(data.clone().player_watchdog());

        Ok(Self {
            ctx: player_context,
            data,
        })
    }

    pub fn from(ctx: PlayerContext) -> PlayerController {
        Self {
            data: PlayerData::from(&ctx),
            ctx,
        }
    }
}
