//! 两个桌面 Host 共用的本地音频输出；播放器及资源字节不进入 Runtime。
use std::{collections::HashMap, io::Cursor};

use narrava_loom_core::resource::{ResourceCatalog, ResourcePath};
use narrava_loom_protocol::{AudioEffect, HostErrorDto, RuntimeUpdate};
use rodio::{Decoder, DeviceSinkBuilder, MixerDeviceSink, Player, Source};

struct Playing {
    resource: String,
    looping: bool,
    player: Player,
}

pub(crate) struct AudioOutput {
    resources: ResourceCatalog,
    channels: HashMap<String, Playing>,
    device: Option<MixerDeviceSink>,
}

impl AudioOutput {
    pub(crate) fn new(resources: ResourceCatalog) -> Self {
        Self {
            resources,
            channels: HashMap::new(),
            device: None,
        }
    }

    /// effect 只消费一次；播放失败作为 Host 提示，不能回滚已提交的故事。
    pub(crate) fn consume(&mut self, update: RuntimeUpdate) -> (RuntimeUpdate, Vec<HostErrorDto>) {
        let RuntimeUpdate::Audio { effects, update } = update else {
            return (update, Vec::new());
        };
        let mut errors: Vec<HostErrorDto> = Vec::new();
        for effect in effects {
            if let Err(error) = self.apply(effect) {
                errors.push(error);
            }
        }
        (
            update.map_or(RuntimeUpdate::Applied, |update| RuntimeUpdate::Ready {
                update,
            }),
            errors,
        )
    }

    fn apply(&mut self, effect: AudioEffect) -> Result<(), HostErrorDto> {
        match effect {
            AudioEffect::Stop { channel } => {
                if let Some(player) = self.channels.remove(&channel) {
                    player.player.stop();
                }
            }
            AudioEffect::Play {
                resource,
                channel,
                looping,
                volume,
            } => {
                ResourcePath::parse(&resource).map_err(|_| audio_error("资源逻辑路径无效"))?;
                if resource.contains(':')
                    || channel.trim().is_empty()
                    || !volume.is_finite()
                    || !(0.0..=1.0).contains(&volume)
                {
                    return Err(audio_error("Audio effect 参数无效"));
                }
                if let Some(current) = self.channels.get(&channel)
                    && current.resource == resource
                    && current.looping == looping
                {
                    current.player.set_volume(volume as f32);
                    return Ok(());
                }
                let bytes = self
                    .resources
                    .read(&resource)
                    .map_err(|_| audio_error("音频资源读取失败"))?
                    .ok_or_else(|| audio_error("音频资源不存在"))?;
                let source: Decoder<Cursor<Vec<u8>>> =
                    Decoder::try_from(Cursor::new(bytes.to_vec()))
                        .map_err(|_| audio_error("音频格式不支持或文件损坏"))?;
                if self.device.is_none() {
                    let mut device: MixerDeviceSink = DeviceSinkBuilder::open_default_sink()
                        .map_err(|_| audio_error("无法打开本地音频设备"))?;
                    device.log_on_drop(false);
                    self.device = Some(device);
                }
                let mixer: rodio::mixer::Mixer =
                    self.device.as_ref().expect("设备已打开").mixer().clone();
                self.play(resource, channel, source, looping, volume, &mixer);
            }
        }
        Ok(())
    }
    fn play(
        &mut self,
        resource: String,
        channel: String,
        source: Decoder<Cursor<Vec<u8>>>,
        looping: bool,
        volume: f64,
        mixer: &rodio::mixer::Mixer,
    ) {
        let player: Player = Player::connect_new(mixer);
        player.set_volume(volume as f32);
        if looping {
            player.append(source.repeat_infinite());
        } else {
            player.append(source);
        }
        if let Some(previous) = self.channels.insert(
            channel,
            Playing {
                resource,
                looping,
                player,
            },
        ) {
            previous.player.stop();
        }
    }
}

impl Drop for AudioOutput {
    fn drop(&mut self) {
        for player in self.channels.values() {
            player.player.stop();
        }
        self.channels.clear();
    }
}

fn audio_error(message: &str) -> HostErrorDto {
    HostErrorDto::new("host.audio", message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decoded_audio_loops_replaces_and_stops_per_channel() {
        let bytes: Vec<u8> = include_bytes!("../examples/resources/audio/chime.wav").to_vec();
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
}
