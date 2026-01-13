use std::time::Duration;

use nxm_music::AudioHandle;

pub fn main() -> anyhow::Result<()> {
    let audio_handle = AudioHandle::new();
    audio_handle.play()?;

    audio_handle.next()?;
    // audio_handle.next()?;
    // audio_handle.next()?;
    // audio_handle.next()?;
    // audio_handle.next()?;
    // audio_handle.next()?;
    // audio_handle.next()?;
    // audio_handle.next()?;
    // audio_handle.next()?;
    // audio_handle.next()?;

    std::thread::sleep(Duration::from_secs(512));

    Ok(())
}
