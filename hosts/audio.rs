//! 两个桌面 Host 共用的本地音频输出；播放器及资源字节不进入 Runtime。
use std::{collections::HashMap, io::Cursor};

use narrava_loom_core::resource::{ResourceCatalog, ResourcePath};
use narrava_loom_protocol::{AudioEffect, HostErrorDto, RuntimeUpdate};
use rodio::{Decoder, DeviceSinkBuilder, MixerDeviceSink, Player, Source};

pub(crate) struct Playing {
    resource: String,
    looping: bool,
    pub(crate) player: Player,
}

pub(crate) struct AudioOutput {
    resources: ResourceCatalog,
    pub(crate) channels: HashMap<String, Playing>,
    pub(crate) device: Option<MixerDeviceSink>,
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

    pub(crate) fn apply(&mut self, effect: AudioEffect) -> Result<(), HostErrorDto> {
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
    pub(crate) fn play(
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
