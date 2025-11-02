use crate::util::{get_own_voice_channel, leave};
use crate::*;
use chrono::{DateTime, TimeDelta, Utc};
use parking_lot::Mutex;
use poise::serenity_prelude::{ChannelId, Http};
use songbird::Songbird;
use std::num::NonZeroU64;
use std::ops::Sub;
use std::sync::Arc;

pub struct PlayerController {
    pub lavalink: LavalinkClient,
    pub text_channel: ChannelId,
    pub http: Arc<Http>,
    pub cache: Arc<SerenityCache>,
    pub songbird: Arc<Songbird>,
    pub guild_id: GuildId,
    pub alone_since: Mutex<Option<DateTime<Utc>>>,
}

impl PlayerController {
    pub fn new(ctx: &Context, lavalink: &LavalinkClient, songbird: Arc<Songbird>) -> Arc<Self> {
        Arc::new(Self {
            lavalink: lavalink.clone(),
            text_channel: ctx.channel_id(),
            http: ctx.serenity_context().http.clone(),
            cache: ctx.serenity_context().cache.clone(),
            songbird,
            guild_id: ctx.guild_id().unwrap().into(),
            alone_since: Mutex::new(None),
        })
    }

    pub async fn init(self: Arc<Self>, vc_id: ChannelId) -> Result<PlayerContext> {
        let (connection_info, _) = self
            .songbird
            .join_gateway(NonZeroU64::new(self.guild_id.0).unwrap(), vc_id)
            .await
            .with_context(|| "Failed to join voice channel")?;

        tokio::spawn(self.clone().player_watchdog());

        let ctx = self
            .lavalink
            .create_player_context_with_data(self.guild_id, connection_info, self.clone())
            .await?;
        Ok(ctx)
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

    pub fn from(ctx: &PlayerContext) -> Arc<PlayerController> {
        ctx.data().expect("Failed to get PlayerContextData")
    }

    async fn player_watchdog(self: Arc<Self>) {
        // Give the player time to initialize
        tokio::time::sleep(Duration::from_secs(10)).await;

        loop {
            if self.lavalink.get_player_context(self.guild_id).is_none() {
                break; // Player has quit
            };

            let channel = get_own_voice_channel(&self.cache, self.guild_id.0).unwrap();
            let members = channel.members(&self.cache).unwrap();

            if members.len() > 1 {
                self.reset_alone();
            } else if self.is_alone_for(TimeDelta::seconds(10)) {
                leave(&self.lavalink, &self.songbird, self.guild_id)
                    .await
                    .unwrap();
            } else {
                self.mark_alone();
            }

            tokio::time::sleep(Duration::from_secs(3)).await;
        }
    }
}
