use anyhow::{Context as _, Result};
use collections::HashMap;
use gpui::{App, BorrowAppContext, Global};

use rodio::{
    Decoder, DeviceSinkBuilder, MixerDeviceSink, Source, nz, source::Buffered,
};
use std::{io::Cursor, num::NonZero};
use util::ResultExt;

mod audio_settings;
pub use audio_settings::AudioSettings;

use crate::audio_settings::LIVE_SETTINGS;

pub const SAMPLE_RATE: NonZero<u32> = nz!(16000);
pub const CHANNEL_COUNT: NonZero<u16> = nz!(1);

pub fn init(cx: &mut App) {
    LIVE_SETTINGS.initialize(cx);
}

#[derive(Debug, Copy, Clone, Eq, Hash, PartialEq)]
pub enum Sound {
    AgentDone,
}

impl Sound {
    fn file(&self) -> &'static str {
        match self {
            Self::AgentDone => "agent_done",
        }
    }
}

pub struct Audio {
    output_handle: Option<MixerDeviceSink>,
    source_cache: HashMap<Sound, Buffered<Decoder<Cursor<Vec<u8>>>>>,
}

impl Default for Audio {
    fn default() -> Self {
        Self {
            output_handle: Default::default(),
            source_cache: Default::default(),
        }
    }
}

impl Global for Audio {}

impl Audio {
    fn ensure_output_exists(&mut self) -> Result<&MixerDeviceSink> {
        #[cfg(debug_assertions)]
        log::warn!(
            "Audio does not sound correct without optimizations. Use a release build to debug audio issues"
        );

        if self.output_handle.is_none() {
            let mut output_handle = DeviceSinkBuilder::open_default_sink()
                .context("Could not open default output stream")?;
            output_handle.log_on_drop(false);
            log::info!("Output stream: {:?}", output_handle);
            output_handle.mixer().add(rodio::source::Zero::new(CHANNEL_COUNT, SAMPLE_RATE));
            self.output_handle = Some(output_handle);
        }

        Ok(self
            .output_handle
            .as_ref()
            .expect("we only get here if opening the outputstream succeeded"))
    }

    pub fn play_sound(sound: Sound, cx: &mut App) {
        cx.update_default_global(|this: &mut Self, cx| {
            let source = this.sound_source(sound, cx).log_err()?;
            let output = this
                .ensure_output_exists()
                .context("Could not get output")
                .log_err()?;

            output.mixer().add(source);
            Some(())
        });
    }

    fn sound_source(&mut self, sound: Sound, cx: &App) -> Result<impl Source + use<>> {
        if let Some(wav) = self.source_cache.get(&sound) {
            return Ok(wav.clone());
        }

        let path = format!("sounds/{}.wav", sound.file());
        let bytes = cx
            .asset_source()
            .load(&path)?
            .map(anyhow::Ok)
            .with_context(|| format!("No asset available for path {path}"))??
            .into_owned();
        let cursor = Cursor::new(bytes);
        let source = Decoder::new(cursor)?.buffered();

        self.source_cache.insert(sound, source.clone());

        Ok(source)
    }
}
