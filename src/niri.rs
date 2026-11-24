use std::time::Duration;

use anyhow::{Context, Result};
use niri_ipc::socket::Socket;
use niri_ipc::{Event, Request, Response};

#[derive(Debug, Clone, Copy)]
pub enum NiriMsg {
    Connected(bool),
    Overview(bool),
}

enum NiriOutcome {
    Unavailable,
    StreamEnded,
}

pub fn spawn(sender: calloop::channel::Sender<NiriMsg>) -> Result<std::thread::JoinHandle<()>> {
    std::thread::Builder::new()
        .name("niri-ipc".into())
        .spawn(move || loop {
            if matches!(connect_and_stream(&sender), NiriOutcome::Unavailable)
                && sender.send(NiriMsg::Connected(false)).is_err()
            {
                break;
            }
            std::thread::sleep(Duration::from_secs(2));
        })
        .context("cannot spawn niri IPC thread")
}

fn connect_and_stream(sender: &calloop::channel::Sender<NiriMsg>) -> NiriOutcome {
    if std::env::var_os("NIRI_SOCKET").is_none() {
        tracing::debug!("NIRI_SOCKET is not set; niri overview tracking disabled");
        return NiriOutcome::Unavailable;
    }
    let initial = query_overview().ok();
    let mut socket = match Socket::connect() {
        Ok(socket) => socket,
        Err(e) => {
            tracing::debug!(error = %e, "cannot connect to niri IPC");
            return NiriOutcome::Unavailable;
        }
    };
    match socket.send(Request::EventStream) {
        Ok(Ok(Response::Handled)) => {}
        Ok(Ok(other)) => {
            tracing::warn!(reply = ?other, "unexpected reply to niri EventStream");
            return NiriOutcome::Unavailable;
        }
        Ok(Err(msg)) => {
            tracing::warn!(error = %msg, "niri refused event stream");
            return NiriOutcome::Unavailable;
        }
        Err(e) => {
            tracing::debug!(error = %e, "niri event stream request failed");
            return NiriOutcome::Unavailable;
        }
    }
    if sender.send(NiriMsg::Connected(true)).is_err() {
        return NiriOutcome::StreamEnded;
    }
    if let Some(open) = initial {
        let _ = sender.send(NiriMsg::Overview(open));
    }
    tracing::info!("connected to niri IPC event stream");
    let mut read_event = socket.read_events();
    loop {
        match read_event() {
            Ok(Event::OverviewOpenedOrClosed { is_open }) => {
                if sender.send(NiriMsg::Overview(is_open)).is_err() {
                    return NiriOutcome::StreamEnded;
                }
            }
            Ok(_) => {}
            Err(e) => {
                tracing::debug!(error = %e, "niri IPC event stream closed");
                return NiriOutcome::StreamEnded;
            }
        }
    }
}

fn query_overview() -> anyhow::Result<bool> {
    let mut socket = Socket::connect()?;
    match socket.send(Request::OverviewState)? {
        Ok(Response::OverviewState(o)) => Ok(o.is_open),
        Ok(other) => anyhow::bail!("unexpected niri reply to OverviewState: {other:?}"),
        Err(e) => anyhow::bail!("niri OverviewState failed: {e}"),
    }
}
