use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use futures_util::StreamExt;
use zbus::Connection;

use crate::control::Control;

const SERVICE: &str = "com.feralinteractive.GameMode";
const PATH: &str = "/com/feralinteractive/GameMode";
const INTERFACE: &str = "com.feralinteractive.GameMode";

#[derive(Debug, Clone, Copy)]
pub enum GameMsg {
    Active(bool),
}

pub fn spawn(
    sender: calloop::channel::Sender<GameMsg>,
    control: Arc<Control>,
) -> Result<std::thread::JoinHandle<()>> {
    std::thread::Builder::new()
        .name("gamemode-dbus".into())
        .spawn(move || {
            let rt = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(rt) => rt,
                Err(e) => {
                    tracing::warn!(error = %e, "cannot create gamemode runtime");
                    return;
                }
            };
            rt.block_on(run(sender, control));
        })
        .context("cannot spawn gamemode thread")
}

async fn run(sender: calloop::channel::Sender<GameMsg>, control: Arc<Control>) -> ! {
    let mut active = false;
    loop {
        let connected = run_connection(&sender, &control, &mut active).await;
        if !connected && active {
            active = false;
            let _ = sender.send(GameMsg::Active(false));
        }
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}

async fn run_connection(
    sender: &calloop::channel::Sender<GameMsg>,
    control: &Control,
    active: &mut bool,
) -> bool {
    let conn = match Connection::session().await {
        Ok(c) => c,
        Err(e) => {
            tracing::debug!(error = %e, "no session bus for gamemode");
            return false;
        }
    };
    let proxy = match zbus::Proxy::new(&conn, SERVICE, PATH, INTERFACE).await {
        Ok(p) => p,
        Err(e) => {
            tracing::debug!(error = %e, "cannot create gamemode proxy");
            return false;
        }
    };

    let mut registered = match proxy.receive_signal("GameRegistered").await {
        Ok(s) => s,
        Err(e) => {
            tracing::debug!(error = %e, "cannot subscribe to GameRegistered");
            return false;
        }
    };
    let mut unregistered = match proxy.receive_signal("GameUnregistered").await {
        Ok(s) => s,
        Err(e) => {
            tracing::debug!(error = %e, "cannot subscribe to GameUnregistered");
            return false;
        }
    };

    let mut tick = tokio::time::interval(Duration::from_secs(1));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut games: HashSet<i32> = HashSet::new();

    match proxy.get_property::<i32>("ClientCount").await {
        Ok(count) => {
            set_active(sender, control, active, count > 0);
            tracing::info!(client_count = count, "gamemode connected");
        }
        Err(e) => {
            tracing::debug!(error = %e, "gamemode service unavailable");
            return false;
        }
    }

    loop {
        tokio::select! {
            signal = registered.next() => {
                let Some(msg) = signal else { return true; };
                match msg.body().deserialize::<(i32, zbus::OwnedObjectPath)>() {
                    Ok((pid, _path)) => {
                        games.insert(pid);
                        set_active(sender, control, active, true);
                    }
                    Err(e) => tracing::debug!(error = %e, "bad GameRegistered payload"),
                }
            }
            signal = unregistered.next() => {
                let Some(msg) = signal else { return true; };
                match msg.body().deserialize::<(i32, zbus::OwnedObjectPath)>() {
                    Ok((pid, _path)) => {
                        games.remove(&pid);
                        if games.is_empty() {
                            match proxy.get_property::<i32>("ClientCount").await {
                                Ok(count) => set_active(sender, control, active, count > 0),
                                Err(e) => {
                                    tracing::debug!(error = %e, "gamemode went away");
                                    return true;
                                }
                            }
                        }
                    }
                    Err(e) => tracing::debug!(error = %e, "bad GameUnregistered payload"),
                }
            }
            _ = tick.tick() => {
                match proxy.get_property::<i32>("ClientCount").await {
                    Ok(0) => {
                        games.clear();
                        set_active(sender, control, active, false);
                    }
                    Ok(_) => set_active(sender, control, active, true),
                    Err(e) => {
                        tracing::debug!(error = %e, "gamemode polling failed");
                        return true;
                    }
                }
            }
        }
    }
}

fn set_active(
    sender: &calloop::channel::Sender<GameMsg>,
    control: &Control,
    active: &mut bool,
    next: bool,
) {
    if *active == next {
        return;
    }
    *active = next;
    control.set_game_active(next);
    tracing::info!(active = next, "gamemode state changed");
    let _ = sender.send(GameMsg::Active(next));
}
