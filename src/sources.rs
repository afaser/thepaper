use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};

use crate::app::App;
use crate::gamemode::GameMsg;
use crate::niri::NiriMsg;

pub fn handle_niri_event(app: &mut App, event: calloop::channel::Event<NiriMsg>) {
    match event {
        calloop::channel::Event::Msg(NiriMsg::Overview(open)) => app.on_overview(open),
        calloop::channel::Event::Msg(NiriMsg::Connected(connected)) => {
            app.on_niri_connected(connected)
        }
        calloop::channel::Event::Closed => {}
    }
}

pub fn handle_game_event(app: &mut App, event: calloop::channel::Event<GameMsg>) {
    match event {
        calloop::channel::Event::Msg(GameMsg::Active(active)) => app.on_game(active),
        calloop::channel::Event::Closed => {}
    }
}

pub fn install_sources(
    handle: &calloop::LoopHandle<'static, App>,
    niri_rx: calloop::channel::Channel<NiriMsg>,
    game_rx: calloop::channel::Channel<GameMsg>,
) -> Result<()> {
    handle
        .insert_source(niri_rx, |event, _, app: &mut App| {
            handle_niri_event(app, event)
        })
        .map_err(|e| anyhow!("cannot insert niri channel: {e}"))?;
    handle
        .insert_source(game_rx, |event, _, app: &mut App| {
            handle_game_event(app, event)
        })
        .map_err(|e| anyhow!("cannot insert gamemode channel: {e}"))?;
    Ok(())
}

pub fn dispatch_loop(
    event_loop: &mut calloop::EventLoop<'static, App>,
    app: &mut App,
) -> Result<()> {
    loop {
        event_loop
            .dispatch(Some(Duration::from_millis(250)), app)
            .context("Wayland event loop failed")?;
        app.poll_gpu();
        if let Some(err) = app.take_fatal() {
            bail!("{err}");
        }
    }
}
