//! CXX-Qt bridge exposing the pure-Rust audio stack to QML.
//!
//! `AudioManager` is an **instantiable** (non-singleton) QObject — one instance
//! per `RecordingPlaybackItem` (mirroring the per-item Qt `MediaPlayer` +
//! `MediaRecorder` it replaces). It wraps the backend
//! [`Recorder`](simsapa_backend::audio::recorder::Recorder) and
//! [`Player`](simsapa_backend::audio::player::Player).
//!
//! Threading: the cpal output stream is `!Send` on some platforms, so the
//! `Player` itself stays on the Qt (GUI) thread where the QObject lives. A
//! background polling thread reads position/state through the player's shared
//! [`PlaybackCore`](simsapa_backend::audio::player::PlaybackCore) handle and
//! marshals updates back to QML via `crate::queue_or_log(...)` — the same pattern
//! `SuttaBridge::generate_waveform_data` uses. No QObject is ever touched off
//! the Qt thread.
//!
//! Lifecycle: the output stream is created lazily on `load()`/`play()`, not at
//! construction, so idle items hold no audio streams. `Drop` stops the polling
//! thread (and the cpal streams drop with the `Player`/`Recorder`), releasing
//! resources when an item is destroyed.
//!
//! See `docs/pure-rust-audio-backend.md`.

use core::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::QString;

use simsapa_backend::audio::player::{PlaybackCore, Player, PlayerState};
use simsapa_backend::audio::recorder::Recorder;
use simsapa_backend::logger::{error, info};

#[cxx_qt::bridge]
pub mod qobject {

    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qproperty(i32, state)]
        #[qproperty(i32, position_ms)]
        #[qproperty(i32, duration_ms)]
        #[qproperty(bool, loading)]
        #[namespace = "audio_manager"]
        type AudioManager = super::AudioManagerRust;
    }

    impl cxx_qt::Threading for AudioManager {}

    extern "RustQt" {
        // Recording
        #[qinvokable]
        fn start_recording(self: Pin<&mut AudioManager>, output_path: QString);

        #[qinvokable]
        fn stop_recording(self: Pin<&mut AudioManager>);

        // Playback
        #[qinvokable]
        fn load(self: Pin<&mut AudioManager>, path: QString);

        #[qinvokable]
        fn play(self: Pin<&mut AudioManager>);

        #[qinvokable]
        fn pause(self: Pin<&mut AudioManager>);

        #[qinvokable]
        fn stop(self: Pin<&mut AudioManager>);

        #[qinvokable]
        fn seek(self: Pin<&mut AudioManager>, position_ms: i32);

        #[qinvokable]
        fn set_volume(self: Pin<&mut AudioManager>, volume: f32);

        #[qinvokable]
        fn play_range(self: Pin<&mut AudioManager>, start_ms: i32, end_ms: i32, looping: bool);

        #[qinvokable]
        fn clear_range(self: Pin<&mut AudioManager>);

        // Signals carrying data not stored as a property. Position / state /
        // duration changes are delivered through the qproperty notify signals
        // (`positionMsChanged` / `stateChanged` / `durationMsChanged`).
        #[qsignal]
        #[cxx_name = "recordingFinished"]
        fn recording_finished(self: Pin<&mut AudioManager>, file_path: QString);

        #[qsignal]
        #[cxx_name = "errorOccurred"]
        fn error_occurred(self: Pin<&mut AudioManager>, message: QString);
    }
}

/// Backing state for `AudioManager`. The `state`/`position_ms`/`duration_ms`
/// fields are exposed as qproperties; the rest is plain Rust state.
pub struct AudioManagerRust {
    state: i32,
    position_ms: i32,
    duration_ms: i32,
    /// True while `load()` is decoding asynchronously — the UI shows a loading
    /// indicator and disables playback controls until the player is ready.
    loading: bool,
    recorder: Option<Recorder>,
    /// Output path of the in-progress recording, kept so `recording_finished`
    /// can report it after `Recorder::stop` consumes the recorder.
    recording_path: Option<String>,
    player: Option<Player>,
    /// Volume to apply once the player exists. `load()` decodes asynchronously,
    /// so `set_volume`/`seek` called right after `load()` (before the player is
    /// built) are remembered here and applied on player creation.
    pending_volume: f32,
    /// Position (ms) to seek to once the player exists; `0` means none pending.
    pending_seek_ms: i32,
    /// Set when `play()` is called before the async `load()` finishes; the
    /// player auto-starts on creation so the user's first click isn't lost.
    pending_play: bool,
    /// The current player's shared playback core, read by the polling thread.
    /// Updated on every `load()` so a re-load rebinds the poller to the new
    /// player (the thread itself is started once and lives until `Drop`).
    current_core: Arc<Mutex<Option<Arc<Mutex<PlaybackCore>>>>>,
    /// Drives the position-polling thread; cleared on `Drop` to stop it.
    poll_running: Arc<AtomicBool>,
    poll_handle: Option<thread::JoinHandle<()>>,
}

impl Default for AudioManagerRust {
    fn default() -> Self {
        AudioManagerRust {
            state: PlayerState::Stopped.as_i32(),
            position_ms: 0,
            duration_ms: 0,
            loading: false,
            recorder: None,
            recording_path: None,
            player: None,
            pending_volume: 1.0,
            pending_seek_ms: 0,
            pending_play: false,
            current_core: Arc::new(Mutex::new(None)),
            poll_running: Arc::new(AtomicBool::new(false)),
            poll_handle: None,
        }
    }
}

impl Drop for AudioManagerRust {
    fn drop(&mut self) {
        // Stop the polling thread before the player/recorder (and their cpal
        // streams) are dropped, so no queued update can reference a dead object.
        self.poll_running.store(false, Ordering::SeqCst);
        if let Some(handle) = self.poll_handle.take() {
            let _ = handle.join();
        }
    }
}

impl qobject::AudioManager {
    fn start_recording(mut self: Pin<&mut Self>, output_path: QString) {
        let path = output_path.to_string();
        match Recorder::start(&path) {
            Ok(recorder) => {
                let rust = self.as_mut().rust_mut().get_mut();
                rust.recorder = Some(recorder);
                rust.recording_path = Some(path);
                info("AudioManager: recording started");
            }
            Err(e) => {
                let msg = format!("Failed to start recording: {e}");
                error(&msg);
                self.as_mut().error_occurred(QString::from(&msg));
            }
        }
    }

    fn stop_recording(mut self: Pin<&mut Self>) {
        let (recorder, path) = {
            let rust = self.as_mut().rust_mut().get_mut();
            (rust.recorder.take(), rust.recording_path.take())
        };
        let recorder = match recorder {
            Some(r) => r,
            None => return,
        };

        // Surface any asynchronous capture error reported by cpal during the
        // recording before finalizing the file.
        if let Some(err) = recorder.take_error() {
            let msg = format!("Recording error: {err}");
            error(&msg);
            self.as_mut().error_occurred(QString::from(&msg));
        }

        match recorder.stop() {
            Ok(()) => {
                if let Some(p) = path {
                    info("AudioManager: recording finished");
                    self.as_mut().recording_finished(QString::from(&p));
                }
            }
            Err(e) => {
                let msg = format!("Failed to finalize recording: {e}");
                error(&msg);
                self.as_mut().error_occurred(QString::from(&msg));
            }
        }
    }

    fn load(mut self: Pin<&mut Self>, path: QString) {
        let path_str = path.to_string();
        self.as_mut().set_loading(true);
        let qt_thread = self.qt_thread();

        // Decode off the GUI thread — a multi-minute MP3/FLAC decode would
        // otherwise block the UI (slow window open, no waveform). The cheap
        // stream build happens back on the Qt thread, where the `!Send` cpal
        // stream must live.
        thread::spawn(move || {
            let decoded = simsapa_backend::audio::player::decode_to_mono(
                std::path::Path::new(&path_str),
            );
            match decoded {
                Ok((mono, src_rate)) => {
                    crate::queue_or_log(&qt_thread, "audio_manager::load", move |mut qo| {
                        match Player::from_samples(mono, src_rate) {
                            Ok(player) => {
                                player.set_volume(qo.rust().pending_volume);
                                let seek_ms = qo.rust().pending_seek_ms;
                                if seek_ms > 0 {
                                    player.seek_ms(seek_ms as i64);
                                }
                                let duration = player.duration_ms() as i32;
                                let pos = player.position_ms() as i32;
                                // Rebind the poller to this player's core (a
                                // re-load replaces an earlier player + core).
                                *qo.rust()
                                    .current_core
                                    .lock()
                                    .unwrap_or_else(|p| p.into_inner()) =
                                    Some(player.shared_core());
                                qo.as_mut().rust_mut().get_mut().player = Some(player);
                                qo.as_mut().set_duration_ms(duration);
                                qo.as_mut().set_position_ms(pos);
                                qo.as_mut().set_loading(false);
                                qo.as_mut().ensure_polling();
                                // Honour a play() pressed while still decoding.
                                if qo.rust().pending_play {
                                    qo.as_mut().rust_mut().get_mut().pending_play = false;
                                    if let Some(p) = &qo.rust().player {
                                        p.play();
                                    }
                                }
                                qo.as_mut().sync_state_from_player();
                            }
                            Err(e) => {
                                let msg = format!("Failed to load audio: {e}");
                                error(&msg);
                                qo.as_mut().set_loading(false);
                                qo.as_mut().error_occurred(QString::from(&msg));
                            }
                        }
                    });
                }
                Err(e) => {
                    let msg = format!("Failed to load audio: {e}");
                    error(&msg);
                    crate::queue_or_log(&qt_thread, "audio_manager::load", move |mut qo| {
                        qo.as_mut().set_loading(false);
                        qo.as_mut().error_occurred(QString::from(&msg));
                    });
                }
            }
        });
    }

    fn play(mut self: Pin<&mut Self>) {
        if let Some(p) = &self.rust().player {
            p.play();
        } else {
            // Player still decoding — start it as soon as it is ready.
            self.as_mut().rust_mut().get_mut().pending_play = true;
            return;
        }
        self.as_mut().sync_state_from_player();
        self.ensure_polling();
    }

    fn pause(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().get_mut().pending_play = false;
        if let Some(p) = &self.rust().player {
            p.pause();
        }
        self.as_mut().sync_state_from_player();
    }

    fn stop(mut self: Pin<&mut Self>) {
        self.as_mut().rust_mut().get_mut().pending_play = false;
        if let Some(p) = &self.rust().player {
            p.stop();
        }
        let pos = self
            .rust()
            .player
            .as_ref()
            .map(|p| p.position_ms() as i32)
            .unwrap_or(0);
        self.as_mut().set_position_ms(pos);
        self.as_mut().sync_state_from_player();
    }

    fn seek(mut self: Pin<&mut Self>, position_ms: i32) {
        if let Some(p) = &self.rust().player {
            p.seek_ms(position_ms as i64);
            let pos = p.position_ms() as i32;
            self.as_mut().set_position_ms(pos);
        } else {
            // Player still decoding — apply once it exists.
            self.as_mut().rust_mut().get_mut().pending_seek_ms = position_ms;
        }
    }

    fn set_volume(mut self: Pin<&mut Self>, volume: f32) {
        self.as_mut().rust_mut().get_mut().pending_volume = volume;
        if let Some(p) = &self.rust().player {
            p.set_volume(volume);
        }
    }

    fn play_range(mut self: Pin<&mut Self>, start_ms: i32, end_ms: i32, looping: bool) {
        if let Some(p) = &self.rust().player {
            p.play_range(start_ms as i64, end_ms as i64, looping);
        }
        self.as_mut().sync_state_from_player();
        self.ensure_polling();
    }

    /// Clear any active range/loop so playback runs to the end of the file. Used
    /// by the QML when a normal seek/stop interrupts range playback.
    fn clear_range(self: Pin<&mut Self>) {
        if let Some(p) = &self.rust().player {
            p.clear_range();
        }
    }

    /// Update the `state` property from the player's current state, emitting the
    /// notify signal only on change.
    fn sync_state_from_player(mut self: Pin<&mut Self>) {
        let new_state = match &self.rust().player {
            Some(p) => p.state().as_i32(),
            None => PlayerState::Stopped.as_i32(),
        };
        if *self.state() != new_state {
            self.as_mut().set_state(new_state);
        }
    }

    /// Start the background position-polling thread (once). Each tick it reads
    /// position / state from the **current** player's shared [`PlaybackCore`]
    /// (via `current_core`, so a re-load rebinds it) and marshals updates to the
    /// Qt thread. Runs until [`Drop`] clears `poll_running`.
    fn ensure_polling(mut self: Pin<&mut Self>) {
        if self.rust().poll_handle.is_some() {
            return;
        }
        let current_core = self.rust().current_core.clone();
        let running = self.rust().poll_running.clone();
        running.store(true, Ordering::SeqCst);
        let qt_thread = self.qt_thread();

        let handle = thread::spawn(move || {
            let mut last_pos: i32 = -1;
            while running.load(Ordering::SeqCst) {
                let core = current_core
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .clone();

                if let Some(core) = core {
                    let (pos, finished) = {
                        let mut core = core.lock().unwrap_or_else(|p| p.into_inner());
                        (core.position_ms() as i32, core.take_finished())
                    };

                    if pos != last_pos {
                        last_pos = pos;
                        crate::queue_or_log(&qt_thread, "audio_manager::ensure_polling", move |mut qo| {
                            qo.as_mut().set_position_ms(pos);
                        });
                    }

                    if finished {
                        crate::queue_or_log(&qt_thread, "audio_manager::ensure_polling", move |mut qo| {
                            qo.as_mut().set_state(PlayerState::Stopped.as_i32());
                            // Stop the device feeding silence after a non-looping end.
                            if let Some(p) = &qo.rust().player {
                                p.halt();
                            }
                        });
                    }
                }

                thread::sleep(Duration::from_millis(50));
            }
        });

        self.as_mut().rust_mut().get_mut().poll_handle = Some(handle);
    }
}
