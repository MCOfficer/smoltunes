use poise::serenity_prelude::{Color, Colour, EmojiIdentifier};
use std::str::FromStr;

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
    } else {
        Colour::from(0x23272A)
    }
}
