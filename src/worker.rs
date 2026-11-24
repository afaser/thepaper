use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};

use crate::control::{Control, FrameMsg, Mailbox};
use crate::decoder::Decoder;
use crate::priority;

pub struct Worker {
    control: Arc<Control>,
    mailbox: Arc<Mailbox>,
    ping: calloop::ping::Ping,
    handoff: Arc<AtomicBool>,
    returned: mpsc::Receiver<Decoder>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl Worker {
    pub fn start(
        video_path: PathBuf,
        control: Arc<Control>,
        mailbox: Arc<Mailbox>,
        ping: calloop::ping::Ping,
    ) -> Result<Self> {
        let decoder = Decoder::open(video_path)?;
        let handoff = Arc::new(AtomicBool::new(false));
        let (returned_tx, returned_rx) = mpsc::channel();
        let handle = spawn_thread(
            decoder,
            control.clone(),
            mailbox.clone(),
            ping.clone(),
            handoff.clone(),
            returned_tx,
        )?;
        Ok(Self {
            control,
            mailbox,
            ping,
            handoff,
            returned: returned_rx,
            handle: Some(handle),
        })
    }

    pub fn resume(&mut self) -> Result<()> {
        if self.control.is_shutdown() {
            return Ok(());
        }
        self.handoff.store(true, Ordering::SeqCst);
        let decoder = self
            .returned
            .recv_timeout(Duration::from_secs(2))
            .context("timed out waiting for the idle decoder worker to hand off state")?;
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
        self.handoff.store(false, Ordering::SeqCst);
        let (returned_tx, returned_rx) = mpsc::channel();
        self.returned = returned_rx;
        self.handle = Some(spawn_thread(
            decoder,
            self.control.clone(),
            self.mailbox.clone(),
            self.ping.clone(),
            self.handoff.clone(),
            returned_tx,
        )?);
        tracing::info!("decoder moved to a fresh SCHED_OTHER worker");
        Ok(())
    }

    pub fn shutdown(mut self) {
        self.control.shutdown();
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn spawn_thread(
    decoder: Decoder,
    control: Arc<Control>,
    mailbox: Arc<Mailbox>,
    ping: calloop::ping::Ping,
    handoff: Arc<AtomicBool>,
    returned: mpsc::Sender<Decoder>,
) -> Result<std::thread::JoinHandle<()>> {
    std::thread::Builder::new()
        .name("vaapi-decoder".into())
        .spawn(move || {
            let mut decoder = decoder;
            priority::register_current_thread();
            let interval = decoder.interval();
            let mut deadline = Instant::now();
            loop {
                if handoff.load(Ordering::SeqCst) {
                    break;
                }
                if !control.wait_if_paused() {
                    break;
                }
                if handoff.load(Ordering::SeqCst) {
                    break;
                }
                match decoder.next_frame() {
                    Ok(frame) => {
                        let now = Instant::now();
                        if deadline > now {
                            std::thread::sleep(deadline - now);
                        }
                        deadline += interval;
                        if deadline < Instant::now() {
                            deadline = Instant::now();
                        }
                        mailbox.push(FrameMsg::Frame(frame));
                        ping.ping();
                    }
                    Err(e) => {
                        mailbox.push(FrameMsg::Error(format!("{e:#}")));
                        ping.ping();
                        break;
                    }
                }
            }
            priority::unregister_current_thread();
            if handoff.load(Ordering::SeqCst) {
                if returned.send(decoder).is_err() {
                    tracing::warn!("decoder handoff receiver disappeared");
                }
            } else {
                tracing::info!(frames = decoder.frames_emitted(), "decoder thread stopped");
            }
        })
        .context("cannot spawn decoder thread")
}
