use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
    },
    thread::JoinHandle,
};

use anyhow::Context as _;
use cpal::{
    SupportedStreamConfig,
    traits::{DeviceTrait as _, HostTrait as _, StreamTrait as _},
};
use creek::{ReadDiskStream, SymphoniaDecoder};
use fixed_resample::FixedResampler;

use crate::{
    io_thread,
    server::{ControllerMsg, MainStreamMsg, UserMainMsg, main_thread},
};

pub struct AudioHandle {
    user_main_tx: crossbeam_channel::Sender<UserMainMsg>,
    main_join_handle: JoinHandle<anyhow::Result<()>>,
    rt: tokio::runtime::Runtime,
    events: Option<flume::Receiver<()>>,
}

impl AudioHandle {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        if std::env::var("RUST_LOG").is_err() {
            unsafe {
                std::env::set_var("RUST_LOG", "trace");
            }
        }
        env_logger::try_init().unwrap();

        let rt = tokio::runtime::Runtime::new().unwrap();

        let (user_main_tx, user_main_rx) = crossbeam_channel::unbounded();

        let (main_io_tx, main_io_rx) = flume::unbounded();

        let _io_join_handle = rt.spawn(io_thread(main_io_rx));

        let main_join_handle = std::thread::spawn({
            move || {
                main_thread(user_main_rx, main_io_tx)?;
                anyhow::Ok(())
            }
        });

        AudioHandle {
            user_main_tx,
            main_join_handle,
            rt,
            events: None,
        }
    }

    pub fn play(&self) -> anyhow::Result<()> {
        self.user_main_tx
            .try_send(UserMainMsg::Controller(ControllerMsg::PlayPause))
            .map_err(|_| anyhow::anyhow!("ControllerMsg::PlayPause"))?;

        anyhow::Ok(())
    }

    pub fn next(&self) -> anyhow::Result<()> {
        self.user_main_tx
            .try_send(UserMainMsg::Controller(ControllerMsg::PlayNext))
            .map_err(|_| anyhow::anyhow!("ControllerMsg::PlayNext"))?;

        anyhow::Ok(())
    }

    // pub fn pause(&self) {
    //     // let _ = self.user_main_tx.try_send(UserMainMsg::Pause);
    // }
    //
    // pub fn resume(&self) {
    //     // let _ = self.user_main_tx.try_send(UserMainMsg::Resume);
    // }
    //
    // pub fn seek(&self, pos: u64) {
    //     // let _ = self.user_main_tx.try_send(UserMainMsg::Seek(pos));
    // }
    //
    // pub fn fetch_library(&self) -> flume::Receiver<Uuid> {
    //     let (tx, rx) = flume::bounded(1);
    //
    //     // let _ = self
    //     //     .user_main_tx
    //     //     .try_send(UserMainMsg::FetchLibrary { reply: tx });
    //
    //     rx
    // }
}

#[derive(Debug, thiserror::Error)]
pub enum ProcessError {
    #[error("End Of File")]
    Eof,
}

const MAX_CHANNELS: usize = 8;

pub struct AudioProcessor {
    read_disk_stream: creek::ReadDiskStream<creek::SymphoniaDecoder>,
    resampler: fixed_resample::FixedResampler<f32, MAX_CHANNELS>,
}

impl AudioProcessor {
    pub fn new(
        resampler: FixedResampler<f32, MAX_CHANNELS>,
        read_disk_stream: ReadDiskStream<SymphoniaDecoder>,
    ) -> Self {
        Self {
            resampler,
            read_disk_stream,
        }
    }
    pub fn process(&mut self, output: &mut [f32]) -> Result<(), ProcessError> {
        let output_len = output.len();
        let output_num_channels = self.resampler.num_channels();
        let output_num_frames = output_len / output_num_channels.get();

        // log::info!(
        //     "src_sample_rate={}, dst_sample_rate={}, output_num_channels={:#?}, output_len={}",
        //     self.resampler.in_sample_rate(),
        //     self.resampler.out_sample_rate(),
        //     output_num_channels,
        //     output_len,
        // );

        let mut samples_written = 0;

        if self.resampler.in_sample_rate() == self.resampler.out_sample_rate() {
            let mut num_frames_read = 0;

            while num_frames_read < output_num_frames {
                let read_data = match self
                    .read_disk_stream
                    .read(output_num_frames - num_frames_read)
                {
                    Ok(read_data) => read_data,
                    Err(e) => {
                        log::error!("ReadDataError: {:#?}", e);
                        output[samples_written..].fill(0f32);
                        return Err(ProcessError::Eof);
                    }
                };

                let read_data_num_frames = read_data.num_frames();
                num_frames_read += read_data_num_frames;

                let deintl_channel_frames = (0..output_num_channels.get())
                    .map(|ch| read_data.read_channel(ch))
                    .collect::<arrayvec::ArrayVec<&[f32], MAX_CHANNELS>>();
                fast_interleave::interleave_variable(
                    &deintl_channel_frames,
                    0..read_data_num_frames,
                    &mut output[samples_written..],
                    output_num_channels,
                );
                samples_written += read_data_num_frames * output_num_channels.get();

                if read_data.reached_end_of_file() {
                    output[samples_written..].fill(0f32);
                    return Err(ProcessError::Eof);
                }
            }
        } else {
            let desired_input_frames =
                (output_num_frames as f64 / self.resampler.ratio()).ceil() as usize;
            let read_data = self.read_disk_stream.read(desired_input_frames).unwrap();
            let deintl_channel_frames = (0..output_num_channels.get())
                .map(|ch| read_data.read_channel(ch))
                .collect::<arrayvec::ArrayVec<&[f32], MAX_CHANNELS>>();

            // log::info!(
            //     "output_frames={}, read_data_num_frames={}, input_block_frames={}, ratio={}, desired_input_frames={}",
            //     output_num_frames,
            //     read_data.num_frames(),
            //     self.resampler.input_block_frames(),
            //     self.resampler.ratio(),
            //     desired_input_frames
            // );

            self.resampler.process(
                &deintl_channel_frames,
                0..read_data.num_frames(),
                |packet| {
                    let packet_frame_len = packet[0].len();
                    // log::info!("packet_frame_len = {:#?}", packet_frame_len);

                    fast_interleave::interleave_variable(
                        &packet,
                        0..packet_frame_len,
                        &mut output[samples_written..],
                        output_num_channels,
                    );
                    samples_written += packet_frame_len * output_num_channels.get();
                },
                Some(fixed_resample::LastPacketInfo {
                    desired_output_frames: Some(output_num_frames as u64),
                }),
                true,
            );

            if read_data.reached_end_of_file() {
                output[samples_written..].fill(0f32);
                return Err(ProcessError::Eof);
            }
        }

        Ok(())
    }
}

pub enum StreamMainMsg {
    StreamStopped(Option<Box<AudioProcessor>>),
}

impl AudioOutputController {
    pub fn new(
        device: &cpal::Device,
        shared_state: Arc<AudioOutputSharedState>,
        stream_main_tx: crossbeam_channel::Sender<StreamMainMsg>,
    ) -> anyhow::Result<Self> {
        let (main_stream_tx, main_stream_rx) = crossbeam_channel::unbounded();

        let supported_stream_config = device.default_output_config()?;
        let stream_config = supported_stream_config.config();

        let stream = device.build_output_stream(
            &stream_config,
            {
                let mut maybe_audio_processor = None;

                // let mut stream_playback_state = StreamPlaybackState::Idle;

                move |data: &mut [f32], _| {
                    while let Ok(msg) = main_stream_rx.try_recv() {
                        match msg {
                            MainStreamMsg::NewProcessor(new_audio_processor) => {
                                maybe_audio_processor = Some(new_audio_processor);
                            }
                            MainStreamMsg::Seek(pos) => {
                                if let Some(ref mut audio_processor) = maybe_audio_processor {
                                    audio_processor
                                        .read_disk_stream
                                        .seek(pos as usize, creek::SeekMode::Auto)
                                        .unwrap();
                                }
                            }
                        }
                    }

                    if !shared_state
                        .playing
                        .load(std::sync::atomic::Ordering::Relaxed)
                    {
                        data.fill(0f32);
                        return;
                    }

                    let Some(ref mut audio_processor) = maybe_audio_processor else {
                        data.fill(0f32);

                        return;
                    };

                    if let Err(ProcessError::Eof) = audio_processor.process(data) {
                        let _ = stream_main_tx
                            .try_send(StreamMainMsg::StreamStopped(maybe_audio_processor.take()));

                        log::info!("sent EoF");
                    };
                }
            },
            |_| {
                //
            },
            None,
        )?;

        stream.pause()?;

        Ok(AudioOutputController {
            stream,
            main_stream_tx,
        })
    }
}

#[derive(Default)]
pub struct AudioOutputSharedState {
    pub playing: AtomicBool,
    pub position: AtomicU64,
    pub muted: AtomicBool,
    pub volume: AtomicU32, // AtomicF32, -1.0..=1.0
}

pub struct AudioOutputController {
    pub stream: cpal::Stream,
    pub main_stream_tx: crossbeam_channel::Sender<MainStreamMsg>,
}

pub struct AudioDevice {
    device: cpal::Device,
    default_output_config: SupportedStreamConfig,
}

pub struct AudioOutput {
    controller: AudioOutputController,
    device: AudioDevice,
    shared_state: Arc<AudioOutputSharedState>,
}

unsafe impl Send for AudioOutput {}
unsafe impl Sync for AudioOutput {}

impl AudioOutput {
    pub fn new(stream_main_tx: crossbeam_channel::Sender<StreamMainMsg>) -> anyhow::Result<Self> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .context("Unable to get default output device")?;

        let default_output_config = device.default_output_config()?;
        let stream_config = default_output_config.config();

        assert!(
            stream_config.channels > 0,
            "There must be at least 1 channel"
        );
        let shared_state: Arc<AudioOutputSharedState> = Arc::default();

        let controller = AudioOutputController::new(&device, shared_state.clone(), stream_main_tx)?;

        let device = AudioDevice {
            device,
            default_output_config,
        };

        Ok(Self {
            controller,
            device,
            shared_state,
        })
    }

    pub fn default_output_config(&self) -> &SupportedStreamConfig {
        &self.device.default_output_config
    }

    pub fn send_new_processor(&self, audio_processor: AudioProcessor) -> anyhow::Result<()> {
        self.controller
            .main_stream_tx
            .try_send(MainStreamMsg::NewProcessor(Box::new(audio_processor)))
            .map_err(|_| anyhow::anyhow!("MainStreamMsg::NewProcessor(..) message sent failed!"))?;
        Ok(())
    }

    pub fn pause_stream(&self) -> anyhow::Result<()> {
        self.controller.stream.pause()?;
        self.shared_state.playing.store(false, Ordering::Relaxed);

        Ok(())
    }

    pub fn play_stream(&self) -> anyhow::Result<()> {
        self.controller.stream.play()?;
        self.shared_state.playing.store(true, Ordering::Relaxed);

        Ok(())
    }

    pub fn seek(&self, pos: u64) -> anyhow::Result<()> {
        self.controller
            .main_stream_tx
            .try_send(MainStreamMsg::Seek(pos))
            .map_err(|_| anyhow::anyhow!("MainStreamMsg::Seek(..) msg send failed"))?;

        Ok(())
    }
}
