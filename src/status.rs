use crate::player_controller::PlayerData;
use crate::util::{format_millis, source_to_color, source_to_emoji, TrackUserData};
use crate::*;
use futures::StreamExt;
use lavalink_rs::model::track::{TrackData, TrackInfo};
use lavalink_rs::player_context::QueueRef;
use lavalink_rs::prelude::PlayerContext;
use poise::serenity_prelude::{CreateEmbed, CreateEmbedAuthor, CreateEmbedFooter, Member};
use std::sync::Arc;

pub struct StatusBuilder {
    cache: Arc<SerenityCache>,
    current_track: Option<TrackData>,
    current_position: u64,
    queue: QueueRef,
    current_member: Member,
}

impl StatusBuilder {
    pub async fn new(ctx: &PlayerContext) -> Result<Self> {
        let data = PlayerData::from(ctx);
        let player = ctx.get_player().await?;

        let guild = data.cache.guild(player.guild_id.0).unwrap().clone();
        let current_id = data.cache.current_user().id;
        let current_member = guild.member(&data.http, current_id).await?.into_owned();

        Ok(Self {
            cache: data.cache.clone(),
            current_track: player.track,
            current_position: player.state.position,
            queue: ctx.get_queue(),
            current_member,
        })
    }

    pub async fn embeds(self) -> Vec<CreateEmbed> {
        vec![self.player_embed(), self.queue_embed().await]
    }

    fn player_embed(&self) -> CreateEmbed {
        let info = self
            .current_track
            .as_ref()
            .map(|t| t.info.clone())
            .unwrap_or_else(|| TrackInfo {
                author: "John Cage".into(),
                title: format!(
                    "4'33\" (performed by {})",
                    self.current_member.display_name()
                ),
                artwork_url: Some("".into()),
                ..Default::default()
            });

        let mut author = CreateEmbedAuthor::new(info.author.clone());
        author = author.icon_url(if self.current_track.is_some() {
            source_to_emoji(&info.source_name).url()
        } else {
            "https://em-content.zobj.net/source/twitter/408/shushing-face_1f92b.png".into()
        });

        // TODO: faux progress bar
        let desc = format!(
            "{} / {}",
            format_millis(self.current_position),
            format_millis(info.length)
        );

        let footer = self.current_track.as_ref().and_then(|track| {
            let data = TrackUserData::try_from(track).ok()?;
            let requester = self.cache.user(data.requester_id.0)?;
            let footer =
                CreateEmbedFooter::new(format!("Requested by {}", requester.display_name()))
                    .icon_url(requester.face());
            Some(footer)
        });

        let mut embed = CreateEmbed::new()
            .title(&info.title)
            .author(author)
            .description(desc)
            .color(source_to_color(&info.source_name));

        if let Some(url) = &info.uri {
            embed = embed.url(url)
        }
        if let Some(url) = &info.artwork_url {
            embed = embed.thumbnail(url)
        }
        if let Some(footer) = footer {
            embed = embed.footer(footer)
        }

        embed
    }

    async fn queue_embed(self) -> CreateEmbed {
        let count = self.queue.get_count().await.unwrap();
        let width = 1 + (count >= 10) as usize;

        let mut lines: Vec<_> = self
            .queue
            .map(|t| t.track.info)
            .enumerate()
            .map(|(i, info)| {
                format!(
                    "{} **{:0>width$}.** `[{}]` {} - {}",
                    source_to_emoji(&info.source_name),
                    i + 1,
                    format_millis(info.length),
                    info.author,
                    info.title,
                    width = width
                )
            })
            .collect()
            .await;

        // Truncate to 15 lines
        if lines.len() > 15 {
            let n = lines.len() - 15;
            lines.truncate(15);
            lines.push(format!("*... {n} more*"))
        }

        let desc = if lines.is_empty() {
            "-# The queue is empty".into() // invisible whitespace
        } else {
            lines.join("\n")
        };

        CreateEmbed::new().title("Queue").description(desc)
    }
}
