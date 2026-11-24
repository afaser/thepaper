use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::Parser;
use smithay_client_toolkit::reexports::calloop::{self, EventLoop};
use smithay_client_toolkit::reexports::calloop_wayland_source::WaylandSource;
use smithay_client_toolkit::reexports::client::{globals::registry_queue_init, Connection};

mod app;
mod codec;
mod config;
mod config_document;
mod control;
mod decoder;
mod frame;
mod gamemode;
mod instance;
mod niri;
mod paths;
mod priority;
mod render;
mod size;
mod sources;
mod surface;
mod transcode;
mod worker;

pub use app::{trim_heap, App};
pub use codec::{available_encoders, choose_target, Codec, VaInfo};
pub use config::Config;
pub use control::{Control, FrameMsg, Mailbox};
pub use decoder::{render_node, Decoder};
pub use frame::{ColorInfo, DmaPlane, PlaneFormat, VideoFrame};
pub use gamemode::{spawn as spawn_gamemode, GameMsg};
pub use instance::{acquire_instance_lock, InstanceLock};
pub use niri::{spawn as spawn_niri, NiriMsg};
pub use paths::{cache_dir, default_config_path};
pub use priority::{
    low_priority, register_current_thread, set_game_active, unregister_current_thread,
    GameScheduling,
};
pub use render::{GpuFrame, Renderer};
pub use size::Size;
pub use sources::{dispatch_loop, handle_game_event, handle_niri_event, install_sources};
pub use surface::{DrawMode, OutputPair, SurfaceEntry, SurfaceFactory};
pub use transcode::{prepare, MediaPlan};
pub use worker::Worker;

#[derive(Debug, Parser)]
#[command(version, about = "Zero-copy live video wallpaper for niri")]
struct Cli {
    #[arg(short, long)]
    config: Option<PathBuf>,
}

fn main() {
    if let Err(e) = real_main() {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}

fn real_main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_env("THEPAPER_LOG")
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .compact()
        .init();

    let cli = Cli::parse();
    let config_path = cli.config.unwrap_or_else(default_config_path);
    let config = Config::load(&config_path)?;
    tracing::info!(
        config = %config_path.display(),
        video = %config.video_path().display(),
        blur_radius = config.blur_radius(),
        app_id_prefix = %config.app_id_prefix(),
        "starting thepaper"
    );

    let _lock = acquire_instance_lock()?;
    low_priority();

    let plan = prepare(config.video_path(), config.transcode_codec())?;
    if plan.transcoded {
        tracing::info!(path = %plan.path.display(), "using cached hardware-decodable transcode");
    }
    let play_path = plan.path.clone();

    let conn = Connection::connect_to_env().context("cannot connect to the Wayland compositor")?;
    let (globals, mut event_queue) =
        registry_queue_init(&conn).context("cannot enumerate Wayland globals")?;
    let qh = event_queue.handle();

    let control = Arc::new(Control::new());
    let mailbox = Arc::new(Mailbox::default());
    let mut app = App::new(
        config,
        &conn,
        &globals,
        &qh,
        control.clone(),
        mailbox.clone(),
    )?;
    event_queue
        .roundtrip(&mut app)
        .context("initial Wayland roundtrip failed")?;
    app.ensure_all_outputs();

    let mut event_loop: EventLoop<'static, App> =
        EventLoop::try_new().context("cannot create event loop")?;
    let handle = event_loop.handle();
    let wayland_source = WaylandSource::new(conn.clone(), event_queue);
    wayland_source
        .insert(handle.clone())
        .map_err(|e| anyhow::anyhow!("cannot insert Wayland source: {e}"))?;

    let (niri_tx, niri_rx) = calloop::channel::channel();
    let (game_tx, game_rx) = calloop::channel::channel();
    install_sources(&handle, niri_rx, game_rx)?;

    let (ping, ping_source) = calloop::ping::make_ping()
        .map_err(|e| anyhow::anyhow!("cannot create decoder wakeup: {e}"))?;
    handle
        .insert_source(ping_source, |_, _, app: &mut App| app.on_ping())
        .map_err(|e| anyhow::anyhow!("cannot insert decoder wakeup: {e}"))?;

    let worker = Worker::start(play_path, control.clone(), mailbox, ping)?;
    app.set_decoder(worker);
    let _niri_thread = spawn_niri(niri_tx)?;
    let _game_thread = spawn_gamemode(game_tx, control.clone())?;

    let result = dispatch_loop(&mut event_loop, &mut app);

    app.shutdown_decoder();
    result
}
