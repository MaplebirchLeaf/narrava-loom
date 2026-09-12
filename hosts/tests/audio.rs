//! 两个 Host 共用的离线音频回归，无需本地音频设备。

use crate::audio::AudioOutput;
use narrava_loom_core::resource::ResourceCatalog;
use narrava_loom_protocol::{AudioEffect, RuntimeUpdate};
use rodio::Decoder;
use std::io::Cursor;

#[test]
fn decoded_audio_loops_replaces_and_stops_per_channel() {
    let bytes: Vec<u8> = include_bytes!("../../examples/resources/audio/chime.wav").to_vec();
    let mut audio = AudioOutput::new(ResourceCatalog::default());
    let (mixer, mut rendered) =
        rodio::mixer::mixer(1.try_into().unwrap(), 8000.try_into().unwrap());
    audio.play(
        "audio/chime.wav".into(),
        "bgm".into(),
        Decoder::try_from(Cursor::new(bytes.clone())).unwrap(),
        true,
        0.25,
        &mixer,
    );
    assert_eq!(audio.channels["bgm"].player.volume(), 0.25);
    assert!(rendered.by_ref().take(16000).any(|sample| sample != 0.0));
    assert!(
        !audio.channels["bgm"].player.empty(),
        "loop must continue beyond the source duration"
    );
    let position = audio.channels["bgm"].player.get_pos();
    audio
        .apply(AudioEffect::Play {
            resource: "audio/chime.wav".into(),
            channel: "bgm".into(),
            looping: true,
            volume: 0.5,
        })
        .unwrap();
    assert_eq!(
        audio.channels["bgm"].player.get_pos(),
        position,
        "volume changes must not restart audio"
    );
    assert_eq!(audio.channels["bgm"].player.volume(), 0.5);
    assert!(
        audio.device.is_none(),
        "updating an existing player must not open another device"
    );
    audio.play(
        "audio/chime.wav".into(),
        "bgm".into(),
        Decoder::try_from(Cursor::new(bytes.clone())).unwrap(),
        false,
        0.5,
        &mixer,
    );
    assert_eq!(audio.channels.len(), 1);
    audio.play(
        "audio/chime.wav".into(),
        "voice".into(),
        Decoder::try_from(Cursor::new(bytes)).unwrap(),
        true,
        1.0,
        &mixer,
    );
    let (update, errors) = audio.consume(RuntimeUpdate::Audio {
        effects: vec![AudioEffect::Stop {
            channel: "bgm".into(),
        }],
        update: None,
    });
    assert!(errors.is_empty());
    assert!(matches!(update, RuntimeUpdate::Applied));
    assert!(!audio.channels.contains_key("bgm"));
    assert!(audio.channels.contains_key("voice"));
    audio
        .apply(AudioEffect::Stop {
            channel: "absent".into(),
        })
        .unwrap();
}

#[test]
fn missing_resource_reports_host_error_without_opening_a_device() {
    let mut audio = AudioOutput::new(ResourceCatalog::default());
    let (update, errors) = audio.consume(RuntimeUpdate::Audio {
        effects: vec![AudioEffect::Play {
            resource: "audio/missing.wav".into(),
            channel: "bgm".into(),
            looping: false,
            volume: 1.0,
        }],
        update: None,
    });
    assert!(matches!(update, RuntimeUpdate::Applied));
    assert_eq!(errors.len(), 1);
    assert!(audio.device.is_none());
}
