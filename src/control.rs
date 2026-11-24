use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};

use crate::frame::VideoFrame;

pub struct Control {
    inner: Mutex<ControlState>,
    cv: Condvar,
}

#[derive(Debug, Default, Clone, Copy)]
struct ControlState {
    game_active: bool,
    shutdown: bool,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Default for Control {
    fn default() -> Self {
        Self::new()
    }
}

impl Control {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(ControlState::default()),
            cv: Condvar::new(),
        }
    }

    pub fn set_game_active(&self, active: bool) {
        let mut state = lock(&self.inner);
        let changed = state.game_active != active;
        state.game_active = active;
        drop(state);
        if changed {
            tracing::info!(active, "control game state");
        }
        if !active {
            self.cv.notify_all();
        }
    }

    pub fn shutdown(&self) {
        lock(&self.inner).shutdown = true;
        self.cv.notify_all();
    }

    pub fn is_shutdown(&self) -> bool {
        lock(&self.inner).shutdown
    }

    pub fn wait_if_paused(&self) -> bool {
        let mut state = lock(&self.inner);
        if state.game_active {
            tracing::info!("decoder paused by control");
        }
        while state.game_active && !state.shutdown {
            state = self.cv.wait(state).unwrap_or_else(PoisonError::into_inner);
        }
        !state.shutdown
    }
}

pub enum FrameMsg {
    Frame(VideoFrame),
    Error(String),
}

#[derive(Default)]
pub struct Mailbox {
    slot: Mutex<Option<FrameMsg>>,
}

impl Mailbox {
    pub fn push(&self, msg: FrameMsg) {
        *lock(&self.slot) = Some(msg);
    }

    pub fn take(&self) -> Option<FrameMsg> {
        lock(&self.slot).take()
    }
}
