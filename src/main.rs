#[macro_use]
extern crate tracing;

use lavalink_rs::{model::events, prelude::*};
use poise::serenity_prelude as serenity;
use songbird::SerenityInit;

pub mod commands;
mod messages;
pub mod music_events;
mod util;

pub struct Data {
    pub lavalink: LavalinkClient,
}

pub type Error = Box<dyn std::error::Error + Send + Sync>;
pub type Context<'a> = poise::Context<'a, Data, Error>;

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing_subscriber::fmt::init();
    // init

    let framework = poise::Framework::builder()
        .options(poise::FrameworkOptions {
            commands: vec![
                commands::clear(),
                commands::join(),
                commands::leave(),
                commands::pause(),
                commands::play(),
                commands::queue(),
                commands::remove(),
                commands::resume(),
                commands::search(),
                commands::seek(),
                commands::shuffle(),
                commands::skip(),
                commands::stop(),
                commands::swap(),
            ],
            prefix_options: poise::PrefixFrameworkOptions {
                prefix: Some("!".to_string()),
                ..Default::default()
            },
            ..Default::default()
        })
        .setup(|ctx, _ready, framework| {
            Box::pin(async move {
                poise::builtins::register_globally(ctx, &framework.options().commands).await?;

                let events = events::Events {
                    raw: Some(music_events::raw_event),
                    ready: Some(music_events::ready_event),
                    track_start: Some(music_events::track_start),
                    ..Default::default()
                };

                let node_local = NodeBuilder {
                    hostname: std::env::var("LAVALINK_URL")
                        .expect("missing $LAVALINK_URL")
                        .to_string(),
                    is_ssl: false,
                    events: events::Events::default(),
                    password: std::env::var("LAVALINK_PASSWORD")
                        .expect("missing $LAVALINK_PASSWORD")
                        .to_string(),
                    user_id: ctx.cache.current_user().id.into(),
                    session_id: None,
                };

                let client = LavalinkClient::new(
                    events,
                    vec![node_local],
                    NodeDistributionStrategy::round_robin(),
                )
                .await;

                Ok(Data { lavalink: client })
            })
        })
        .build();

    let mut client = serenity::ClientBuilder::new(
        std::env::var("DISCORD_TOKEN").expect("missing DISCORD_TOKEN"),
        serenity::GatewayIntents::GUILD_MESSAGES
            | serenity::GatewayIntents::GUILD_VOICE_STATES
            | serenity::GatewayIntents::MESSAGE_CONTENT
            | serenity::GatewayIntents::GUILD_MESSAGE_REACTIONS
            | serenity::GatewayIntents::GUILD_INTEGRATIONS
            | serenity::GatewayIntents::GUILDS,
    )
    .register_songbird()
    .framework(framework)
    .await?;

    client.start().await?;

    Ok(())
}
