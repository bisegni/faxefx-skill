//! Own output-device lifetime and playback events on a dedicated thread.
use rodio::Source;
use std::{
    path::PathBuf,
    sync::{Arc, Mutex, mpsc},
    time::Duration,
};
#[derive(Debug, Clone)]
pub enum PlaybackEvent {
    Started,
    Paused,
    Stopped,
    Position(f64),
    Error(String),
}
pub enum Playback {
    Play {
        path: PathBuf,
        start: f64,
        duration: f64,
        gain: f32,
    },
    Pause,
    Resume,
    Stop,
    Volume(f32),
    Shutdown,
}
pub struct Player {
    tx: mpsc::SyncSender<Playback>,
}
impl Player {
    pub fn start() -> (Self, Arc<Mutex<mpsc::Receiver<PlaybackEvent>>>) {
        let (tx, rx) = mpsc::sync_channel(16);
        let (events, out) = mpsc::sync_channel(64);
        std::thread::spawn(move || {
            let stream = match rodio::OutputStreamBuilder::open_default_stream() {
                Ok(s) => s,
                Err(_) => {
                    let _ = events.send(PlaybackEvent::Error(
                        "No audio output device is available".into(),
                    ));
                    return;
                }
            };
            let sink = rodio::Sink::connect_new(stream.mixer());
            let mut playing = false;
            loop {
                let command = if playing {
                    match rx.recv_timeout(Duration::from_millis(100)) {
                        Ok(c) => Some(c),
                        Err(mpsc::RecvTimeoutError::Timeout) => {
                            if sink.empty() {
                                playing = false;
                                let _ = events.send(PlaybackEvent::Stopped);
                            } else {
                                let _ = events.try_send(PlaybackEvent::Position(
                                    sink.get_pos().as_secs_f64(),
                                ));
                            }
                            None
                        }
                        Err(_) => break,
                    }
                } else {
                    match rx.recv() {
                        Ok(c) => Some(c),
                        Err(_) => break,
                    }
                };
                let Some(command) = command else { continue };
                match command {
                    Playback::Play {
                        path,
                        start,
                        duration,
                        gain,
                    } => {
                        if !start.is_finite()
                            || !(0.0..=86400.).contains(&start)
                            || !duration.is_finite()
                            || duration <= 0.
                            || duration > 120.
                            || !gain.is_finite()
                            || !(0.0..=1.0).contains(&gain)
                        {
                            let _ = events
                                .send(PlaybackEvent::Error("Invalid playback excerpt".into()));
                            continue;
                        }
                        match std::fs::File::open(path)
                            .map_err(|e| e.to_string())
                            .and_then(|f| rodio::Decoder::try_from(f).map_err(|e| e.to_string()))
                        {
                            Ok(decoder) => {
                                if decoder
                                    .total_duration()
                                    .is_some_and(|d| start >= d.as_secs_f64())
                                {
                                    let _ = events.send(PlaybackEvent::Error(
                                        "Excerpt starts after audio ends".into(),
                                    ));
                                    continue;
                                }
                                sink.stop();
                                sink.append(
                                    decoder
                                        .skip_duration(Duration::from_secs_f64(start))
                                        .take_duration(Duration::from_secs_f64(duration))
                                        .amplify(gain),
                                );
                                sink.play();
                                playing = true;
                                let _ = events.send(PlaybackEvent::Started);
                            }
                            Err(_) => {
                                let _ = events.send(PlaybackEvent::Error(
                                    "Cannot decode the selected audio".into(),
                                ));
                            }
                        }
                    }
                    Playback::Pause => {
                        sink.pause();
                        playing = false;
                        let _ = events.send(PlaybackEvent::Paused);
                    }
                    Playback::Resume => {
                        sink.play();
                        playing = true;
                        let _ = events.send(PlaybackEvent::Started);
                    }
                    Playback::Stop => {
                        sink.stop();
                        playing = false;
                        let _ = events.send(PlaybackEvent::Stopped);
                    }
                    Playback::Volume(v) => {
                        if v.is_finite() {
                            sink.set_volume(v.clamp(0., 1.));
                        }
                    }
                    Playback::Shutdown => break,
                }
            }
            sink.stop();
        });
        (Self { tx }, Arc::new(Mutex::new(out)))
    }
    pub fn send(&self, command: Playback) -> Result<(), String> {
        self.tx
            .try_send(command)
            .map_err(|_| "Audio output unavailable or busy".into())
    }
}
impl Drop for Player {
    fn drop(&mut self) {
        let _ = self.tx.try_send(Playback::Shutdown);
    }
}
