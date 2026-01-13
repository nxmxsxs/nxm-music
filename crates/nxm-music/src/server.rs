use std::{num::NonZeroUsize, sync::Arc};

use collections::FxIndexMap;
use creek::{ReadDiskStream, SymphoniaDecoder};
use crossbeam_channel::Receiver;
use uuid::Uuid;

use crate::{
    FetchLibraryRes, MainIoMsg, Track,
    audio_handle::{AudioOutput, AudioProcessor, StreamMainMsg},
    queue::Queue,
};

pub enum MainPreloaderMsg {
    PreloadTrack { id: Uuid, src: String },
}

pub enum PreloaderMainMsg {
    PreloadedTrack {
        id: Uuid,
        stream: ReadDiskStream<SymphoniaDecoder>,
    },
}

pub enum TrackMsg {
    Play(Uuid),
}

pub enum ControllerMsg {
    PlayPause,
    PlayNext,
    PlayPrev,
    Seek(u64),
}

pub enum UserMainMsg {
    Track(TrackMsg),
    Controller(ControllerMsg),
}

pub enum MainStreamMsg {
    NewProcessor(Box<AudioProcessor>),
    Seek(u64),
}

pub enum Event {}

#[derive(Debug, PartialEq, Eq)]
pub enum TrackPreloaderState {
    NotPreloaded,
    Preloading(Uuid),
    Preloaded(Uuid),
}

pub struct Preloader {
    curr: TrackPreloaderState,
    next: TrackPreloaderState,
    tx: crossbeam_channel::Sender<MainPreloaderMsg>,
}

pub struct State {
    audio_output: AudioOutput,
    queue: Queue,
    library: FxIndexMap<Uuid, Arc<Track>>,
    preloader: Preloader,
}

impl State {
    pub fn handle_user_main_msg(&mut self, msg: UserMainMsg) -> anyhow::Result<()> {
        log::info!("got UserMainMsg");

        match msg {
            UserMainMsg::Track(msg) => match self.handle_track_msg(msg) {
                Ok(_) => {}
                Err(e) => log::error!("handle_track_msg error: {:#?}", e),
            },
            UserMainMsg::Controller(msg) => match self.handle_controller_msg(msg) {
                Ok(_) => {}
                Err(e) => log::error!("handle_controller_msg error: {:#?}", e),
            },
        }

        Ok(())
    }

    pub fn handle_pause(&mut self) -> anyhow::Result<()> {
        self.audio_output.pause_stream()?;

        Ok(())
    }

    pub fn handle_resume(&mut self) -> anyhow::Result<()> {
        Ok(())
    }

    pub fn handle_seek(&mut self, pos: u64) -> anyhow::Result<()> {
        self.audio_output.seek(pos)?;
        Ok(())
    }

    pub fn handle_stream_stopped(
        &mut self,
        maybe_audio_processor: Option<Box<AudioProcessor>>,
    ) -> anyhow::Result<()> {
        self.audio_output.pause_stream()?;

        // TODO:
        //  - Propagate `StreamStopped` event to the UI

        let _ = maybe_audio_processor;

        Ok(())
    }

    pub fn handle_stream_main_msg(&mut self, msg: StreamMainMsg) -> anyhow::Result<()> {
        match msg {
            StreamMainMsg::StreamStopped(maybe_audio_processor) => {
                match self.handle_stream_stopped(maybe_audio_processor) {
                    Ok(_) => {}
                    Err(e) => log::error!("handle_stream_stopped error: {:#?}", e),
                };
            }
        }

        Ok(())
    }

    pub fn send_new_processor(&self, audio_processor: AudioProcessor) -> anyhow::Result<()> {
        self.audio_output.send_new_processor(audio_processor)?;

        Ok(())
    }

    fn handle_track_msg(&mut self, msg: TrackMsg) -> anyhow::Result<()> {
        match msg {
            TrackMsg::Play(id) => {
                if !matches!(self.preloader.curr, TrackPreloaderState::Preloaded(..)) {
                    self.preloader
                        .tx
                        .try_send(MainPreloaderMsg::PreloadTrack {
                            id,
                            src: self.library[&id].filepath.clone(),
                        })
                        .map_err(|_| {
                            anyhow::anyhow!("Unable to send MainPreloaderMsg::PreloadCurr(..)")
                        })?;
                    self.preloader.curr = TrackPreloaderState::Preloading(id);
                }
            }
        }

        Ok(())
    }

    fn handle_controller_msg(&mut self, msg: ControllerMsg) -> anyhow::Result<()> {
        log::info!("Got ControllerMsg");

        match msg {
            ControllerMsg::PlayPause => match self.handle_play_pause() {
                Ok(_) => {}
                Err(e) => log::error!("handle_play_pause error: {:#?}", e),
            },
            ControllerMsg::PlayNext => self.handle_play_next()?,
            ControllerMsg::PlayPrev => self.handle_play_prev()?,
            ControllerMsg::Seek(pos) => self.handle_seek(pos)?,
        }

        Ok(())
    }

    fn handle_play_pause(&mut self) -> anyhow::Result<()> {
        log::info!("handle_play_pause");

        if let Some(id) = self.queue.curr()
            && (matches!(self.preloader.curr, TrackPreloaderState::NotPreloaded)
                || matches!(self.preloader.curr, TrackPreloaderState::Preloading(curr_preloading_id) if curr_preloading_id != id))
        {
            self.preloader
                .tx
                .try_send(MainPreloaderMsg::PreloadTrack {
                    id,
                    src: self.library[&id].filepath.clone(),
                })
                .map_err(|_| anyhow::anyhow!("Unable to send MainPreloaderMsg::PreloadCurr(..)"))?;
            self.preloader.curr = TrackPreloaderState::Preloading(id);
        }

        Ok(())
    }

    fn handle_play_next(&mut self) -> anyhow::Result<()> {
        if let Some(id) = self.queue.next() {
            if let TrackPreloaderState::Preloading(next_preloading_id) = self.preloader.next {
                if id == next_preloading_id {
                    self.preloader.curr = TrackPreloaderState::Preloading(next_preloading_id);
                    self.preloader.next = TrackPreloaderState::NotPreloaded;
                } else {
                    //
                }
            }

            println!("next: {:#?}", id);
            match self.handle_track_msg(TrackMsg::Play(id)) {
                Ok(_) => {}
                Err(e) => log::error!("handle_track_msg error: {:#?}", e),
            };
        }

        Ok(())
    }

    fn handle_play_prev(&self) -> anyhow::Result<()> {
        Ok(())
    }

    fn handle_preloader_main_msg(&mut self, msg: PreloaderMainMsg) -> anyhow::Result<()> {
        match msg {
            PreloaderMainMsg::PreloadedTrack {
                id: preloaded_id,
                stream: read_disk_stream,
            } => {
                match self.preloader.curr {
                    TrackPreloaderState::NotPreloaded => {}
                    TrackPreloaderState::Preloading(curr_preloading_id) => {
                        if preloaded_id == curr_preloading_id {
                            self.preloader.curr = TrackPreloaderState::Preloaded(preloaded_id);

                            let read_disk_stream_info = read_disk_stream.info();

                            let supported_stream_config = self.audio_output.default_output_config();
                            let stream_config = supported_stream_config.config();

                            let audio_processor = AudioProcessor::new(
                                fixed_resample::FixedResampler::new(
                                    unsafe {
                                        NonZeroUsize::new_unchecked(stream_config.channels as usize)
                                    },
                                    read_disk_stream_info
                                        .sample_rate
                                        .unwrap_or(stream_config.sample_rate),
                                    stream_config.sample_rate,
                                    fixed_resample::ResampleQuality::Low,
                                    false,
                                ),
                                read_disk_stream,
                            );

                            self.send_new_processor(audio_processor)?;
                            self.audio_output.play_stream()?;

                            println!("started stream: {:#?}", preloaded_id);

                            if let Some(next) = self.queue.peek_next() {
                                self.preloader
                                    .tx
                                    .try_send(MainPreloaderMsg::PreloadTrack {
                                        id: next,
                                        src: self.library[&next].filepath.clone(),
                                    })
                                    .map_err(|_| {
                                        anyhow::anyhow!(
                                            "Unable to send MainPreloaderMsg::PreloadNext"
                                        )
                                    })?;
                                self.preloader.next = TrackPreloaderState::Preloading(next);
                            }
                        }
                    }
                    TrackPreloaderState::Preloaded(curr_preloaded_id) => {}
                }

                match self.preloader.next {
                    TrackPreloaderState::NotPreloaded => {}
                    TrackPreloaderState::Preloading(next_preloading_id) => {
                        if preloaded_id == next_preloading_id {
                            self.preloader.next = TrackPreloaderState::Preloaded(preloaded_id);
                            log::info!("preloaded next: {:#?}", preloaded_id);
                        }
                    }
                    TrackPreloaderState::Preloaded(next_preloaded_id) => {}
                }
            }
        }

        Ok(())
    }
}

pub fn main_thread(
    user_main_rx: crossbeam_channel::Receiver<UserMainMsg>,
    main_io_tx: flume::Sender<MainIoMsg>,
) -> anyhow::Result<()> {
    let (main_preloader_tx, main_preloader_rx) = crossbeam_channel::unbounded();
    let (preloader_main_tx, preloader_main_rx) = crossbeam_channel::unbounded();

    let preloader_join_handle = std::thread::spawn({
        move || {
            preloader_thread(main_preloader_rx, preloader_main_tx)?;
            anyhow::Ok(())
        }
    });
    let (tx, rx) = flume::bounded(1);

    main_io_tx
        .try_send(MainIoMsg::FetchLibrary { reply: tx })
        .unwrap();

    let FetchLibraryRes::Snapshot(library) = rx.recv().unwrap() else {
        unreachable!()
    };

    let (stream_main_tx, stream_main_rx) = crossbeam_channel::unbounded();

    let mut state = State {
        audio_output: AudioOutput::new(stream_main_tx)?,
        queue: Queue::new(&library),
        library,
        preloader: Preloader {
            curr: TrackPreloaderState::NotPreloaded,
            next: TrackPreloaderState::NotPreloaded,
            tx: main_preloader_tx,
        },
    };

    loop {
        log::info!("waiting for msgs");

        crossbeam_channel::select_biased! {
            recv(user_main_rx) -> msg => {
                match msg {
                    Ok(msg) => state.handle_user_main_msg(msg)?,
                    Err(e) => {
                        log::error!("UserMainMsg: {:#?}", e);
                    }
                }
            }

            recv(stream_main_rx) -> msg => {
                match msg {
                    Ok(msg) => state.handle_stream_main_msg(msg)?,
                    Err(e) => {
                        log::error!("StreamMainMsg: {:#?}", e);
                    }
                }
            }

            recv(preloader_main_rx) -> msg => {
                match msg {
                    Ok(msg) => state.handle_preloader_main_msg(msg)?,
                    Err(e) => {
                        log::error!("StreamMainMsg: {:#?}", e);
                    }
                }
            }
        }
    }
}

pub fn preloader_thread(
    main_preloader_rx: Receiver<MainPreloaderMsg>,
    preloader_main_tx: crossbeam_channel::Sender<PreloaderMainMsg>,
) -> anyhow::Result<()> {
    fn preload_source(src: String) -> anyhow::Result<ReadDiskStream<SymphoniaDecoder>> {
        let mut read_disk_stream = match creek::ReadDiskStream::<creek::SymphoniaDecoder>::new(
            &src,
            0,
            creek::ReadStreamOptions {
                num_cache_blocks: 20,
                num_caches: 2,
                ..Default::default()
            },
        ) {
            Ok(o) => o,
            Err(e) => panic!("error loading from disk: {:#?}", e),
        };
        read_disk_stream.seek(0, Default::default())?;
        // Cache the start of the file into cache with index `0`.
        let _ = read_disk_stream.cache(0, 0);

        // Tell the stream to seek to the beginning of file. This will also alert the stream to the existence
        // of the cache with index `0`.
        if let Err(e) = read_disk_stream.block_until_ready() {
            log::error!("{e:#?}");
        }

        Ok(read_disk_stream)
    }

    loop {
        match main_preloader_rx.recv()? {
            MainPreloaderMsg::PreloadTrack { id, src } => {
                preloader_main_tx
                    .send(PreloaderMainMsg::PreloadedTrack {
                        id,
                        stream: preload_source(src)?,
                    })
                    .map_err(|_| {
                        anyhow::anyhow!("unable to send PreloaderMainMsg::PreloadedCurr(..)")
                    })?;
            }
        }
    }
}
