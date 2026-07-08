//! Audio command seam.

use nara_asset::Handle;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AudioClip;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AudioCommand {
    Play {
        clip: Handle<AudioClip>,
        volume: f32,
        looped: bool,
    },
    StopAll,
}

pub trait AudioSink {
    fn submit(&mut self, command: AudioCommand);
}
