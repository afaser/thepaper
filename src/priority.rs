use std::collections::HashSet;
use std::sync::{LazyLock, Mutex, PoisonError};

static TIDS: LazyLock<Mutex<HashSet<libc::pid_t>>> = LazyLock::new(|| Mutex::new(HashSet::new()));

#[derive(Debug, Clone, Copy)]
pub enum GameScheduling {
    Idle,
    Normal,
}

impl GameScheduling {
    pub fn from_active(active: bool) -> Self {
        match active {
            true => Self::Idle,
            false => Self::Normal,
        }
    }
}

fn current_tid() -> libc::pid_t {
    unsafe { libc::syscall(libc::SYS_gettid) as libc::pid_t }
}

pub fn low_priority() {
    unsafe {
        let r = libc::setpriority(libc::PRIO_PROCESS, 0, 19);
        if r != 0 {
            tracing::warn!(
                error = %std::io::Error::last_os_error(),
                "setpriority(nice 19) failed"
            );
        }
        const IOPRIO_WHO_PROCESS: libc::c_int = 1;
        const IOPRIO_CLASS_IDLE: libc::c_int = 3;
        const IOPRIO_CLASS_SHIFT: libc::c_int = 13;
        let r = libc::syscall(
            libc::SYS_ioprio_set,
            IOPRIO_WHO_PROCESS,
            0,
            IOPRIO_CLASS_IDLE << IOPRIO_CLASS_SHIFT,
        );
        if r != 0 {
            tracing::warn!(
                error = %std::io::Error::last_os_error(),
                "ioprio_set(idle) failed"
            );
        }
    }
    tracing::info!("nice 19 and idle I/O priority applied");
}

pub fn register_current_thread() {
    let tid = current_tid();
    TIDS.lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(tid);
    tracing::debug!(tid, "registered worker thread");
}

pub fn unregister_current_thread() {
    let tid = current_tid();
    TIDS.lock()
        .unwrap_or_else(PoisonError::into_inner)
        .remove(&tid);
    tracing::debug!(tid, "unregistered worker thread");
}

fn set_policy(tid: libc::pid_t, policy: libc::c_int) -> bool {
    let param = libc::sched_param { sched_priority: 0 };
    let r = unsafe { libc::sched_setscheduler(tid, policy, &param) };
    if r != 0 {
        tracing::warn!(
            tid,
            policy,
            error = %std::io::Error::last_os_error(),
            "sched_setscheduler failed"
        );
        false
    } else {
        true
    }
}

pub fn set_game_active(scheduling: GameScheduling) {
    let tids = TIDS.lock().unwrap_or_else(PoisonError::into_inner).clone();
    match scheduling {
        GameScheduling::Idle => {
            for tid in &tids {
                const SCHED_IDLE: libc::c_int = 5;
                const SCHED_BATCH: libc::c_int = 3;
                if !set_policy(*tid, SCHED_IDLE) {
                    let _ = set_policy(*tid, SCHED_BATCH);
                }
            }
            tracing::info!(threads = ?tids, policy = "SCHED_IDLE", "worker scheduling updated");
        }
        GameScheduling::Normal => {
            for tid in &tids {
                let _ = set_policy(*tid, libc::SCHED_OTHER);
            }
            tracing::info!(threads = ?tids, policy = "SCHED_OTHER", "worker scheduling updated");
        }
    }
}
