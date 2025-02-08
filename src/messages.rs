use crate::util::{format_millis, source_to_color, source_to_emoji};
use lavalink_rs::model::track::TrackData;
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

pub fn search_results(results: &Vec<Vec<TrackData>>) -> CreateEmbed {
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
