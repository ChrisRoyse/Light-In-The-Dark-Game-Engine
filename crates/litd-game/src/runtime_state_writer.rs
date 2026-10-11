//! Off-render-thread runtime-state persistence (#1460).
//!
//! The runtime host used to build the full `RuntimeHostState`, serialize it,
//! write it into the Aster vault, read it back, byte-compare, and SHA-256 it on
//! the winit/render thread on EVERY frame. That put synchronous disk I/O and
//! hashing on the frame-pacing critical path.
//!
//! This owns a single background thread that is the sole writer of the Aster
//! vault (so there is never concurrent access to the vault):
//!   - state TRANSITIONS (accepted/running/closed/failed/exited) go through
//!     [`RuntimeStateWriter::persist_blocking`], which keeps the exact
//!     write→readback→hash FSV discipline and returns the verified readback;
//!   - the per-frame heartbeat goes through [`RuntimeStateWriter::persist_async`]
//!     at a cadence (fire-and-forget), so a frame never blocks on the vault.
//!
//! Async write failures are never swallowed: they are captured and surfaced by
//! [`RuntimeStateWriter::take_error`] (checked each frame) and by
//! [`RuntimeStateWriter::shutdown`] (checked at exit), so the host still fails
//! loud if the vault becomes unavailable mid-run.

use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        mpsc::{Receiver, Sender, SyncSender, channel, sync_channel},
    },
    thread::JoinHandle,
};

use litd_engine::EngineError;

use crate::runtime_state::{
    RuntimeHostState, RuntimeHostStateReadback, runtime_state_error,
    write_runtime_state_with_readback,
};

enum WriterCommand {
    Persist {
        state: Box<RuntimeHostState>,
        ack: Option<SyncSender<Result<RuntimeHostStateReadback, EngineError>>>,
    },
    Shutdown,
}

pub struct RuntimeStateWriter {
    tx: Sender<WriterCommand>,
    handle: Option<JoinHandle<()>>,
    async_error: Arc<Mutex<Option<String>>>,
}

impl RuntimeStateWriter {
    pub fn spawn(path: PathBuf) -> Result<Self, EngineError> {
        let (tx, rx) = channel::<WriterCommand>();
        let async_error = Arc::new(Mutex::new(None));
        let thread_error = Arc::clone(&async_error);
        let handle = std::thread::Builder::new()
            .name("litd-game-runtime-state-writer".to_string())
            .spawn(move || writer_loop(path, rx, thread_error))
            .map_err(|error| runtime_state_error("state.writer_spawn", error.to_string()))?;
        Ok(Self {
            tx,
            handle: Some(handle),
            async_error,
        })
    }

    /// Synchronous transition write: keeps full write→readback→hash discipline and
    /// returns the verified readback. Used off the per-frame path (rare).
    pub fn persist_blocking(
        &self,
        state: RuntimeHostState,
    ) -> Result<RuntimeHostStateReadback, EngineError> {
        let (ack_tx, ack_rx) = sync_channel(1);
        self.tx
            .send(WriterCommand::Persist {
                state: Box::new(state),
                ack: Some(ack_tx),
            })
            .map_err(|_| writer_gone())?;
        ack_rx.recv().map_err(|_| writer_gone())?
    }

    /// Fire-and-forget cadence write. The frame never blocks; any failure is
    /// captured and surfaced by [`Self::take_error`].
    pub fn persist_async(&self, state: RuntimeHostState) -> Result<(), EngineError> {
        self.tx
            .send(WriterCommand::Persist {
                state: Box::new(state),
                ack: None,
            })
            .map_err(|_| writer_gone())
    }

    /// Non-blocking check of the async-writer error slot (called each frame).
    pub fn take_error(&self) -> Option<EngineError> {
        self.async_error
            .lock()
            .ok()
            .and_then(|mut slot| slot.take())
            .map(|reason| runtime_state_error("state.async_write", reason))
    }

    /// Stop the writer thread and surface any residual async error.
    pub fn shutdown(&mut self) -> Result<(), EngineError> {
        let _ = self.tx.send(WriterCommand::Shutdown);
        if let Some(handle) = self.handle.take() {
            handle
                .join()
                .map_err(|_| runtime_state_error("state.writer_join", "writer thread panicked"))?;
        }
        match self.take_error() {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}

impl Drop for RuntimeStateWriter {
    fn drop(&mut self) {
        if self.handle.is_some() {
            let _ = self.shutdown();
        }
    }
}

fn writer_loop(
    path: PathBuf,
    rx: Receiver<WriterCommand>,
    async_error: Arc<Mutex<Option<String>>>,
) {
    while let Ok(command) = rx.recv() {
        match command {
            WriterCommand::Persist { state, ack } => {
                let result = write_runtime_state_with_readback(&path, &state);
                match ack {
                    Some(ack) => {
                        let _ = ack.send(result);
                    }
                    None => {
                        if let Err(error) = result
                            && let Ok(mut slot) = async_error.lock()
                            && slot.is_none()
                        {
                            *slot = Some(error.to_string());
                        }
                    }
                }
            }
            WriterCommand::Shutdown => break,
        }
    }
}

fn writer_gone() -> EngineError {
    runtime_state_error(
        "state.writer",
        "runtime state writer thread is no longer running",
    )
}
