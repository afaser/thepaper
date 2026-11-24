use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use anyhow::{anyhow, bail, Result};
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    delegate_registry,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    shell::{
        wlr_layer::{LayerShell, LayerShellHandler, LayerSurface, LayerSurfaceConfigure},
        WaylandSurface,
    },
};
use wayland_client::{
    backend::ObjectId, globals::GlobalList, protocol::wl_output, Connection, Proxy, QueueHandle,
};

use crate::config::Config;
use crate::control::{Control, FrameMsg, Mailbox};
use crate::frame::VideoFrame;
use crate::priority::{self, GameScheduling};
use crate::render::{GpuFrame, Renderer};
use crate::size::Size;
use crate::surface::{DrawMode, OutputPair, SurfaceFactory};
use crate::worker::Worker;

struct RetiredFrame {
    _frame: Rc<GpuFrame>,
    generation: u64,
}

pub struct App {
    conn: Connection,
    qh: QueueHandle<App>,
    registry_state: RegistryState,
    output_state: OutputState,
    compositor_state: CompositorState,
    layer_shell: LayerShell,
    instance: wgpu::Instance,
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
    renderer: Renderer,
    config: Config,
    control: Arc<Control>,
    mailbox: Arc<Mailbox>,
    decoder: Option<Worker>,
    outputs: HashMap<ObjectId, OutputPair>,
    current: Option<Rc<GpuFrame>>,
    retired: Vec<RetiredFrame>,
    submitted_gen: u64,
    completed_gen: Arc<AtomicU64>,
    overview_open: bool,
    game_active: bool,
    fatal: Option<String>,
}

impl App {
    pub fn new(
        config: Config,
        conn: &Connection,
        globals: &GlobalList,
        qh: &QueueHandle<App>,
        control: Arc<Control>,
        mailbox: Arc<Mailbox>,
    ) -> Result<Self> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::VULKAN,
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: None,
            apply_limit_buckets: false,
        }))
        .map_err(|e| anyhow!("no Vulkan adapter: {e}"))?;
        let info = adapter.get_info();
        if info.backend != wgpu::Backend::Vulkan {
            bail!("expected a Vulkan adapter, got {:?}", info.backend);
        }
        let available = adapter.features();
        let required = wgpu::Features::VULKAN_EXTERNAL_MEMORY_DMA_BUF | wgpu::Features::IMMEDIATES;
        if !available.contains(required) {
            bail!(
                "Vulkan adapter {} lacks required features (DMA-BUF={}, immediates={})",
                info.name,
                available.contains(wgpu::Features::VULKAN_EXTERNAL_MEMORY_DMA_BUF),
                available.contains(wgpu::Features::IMMEDIATES)
            );
        }
        let limits = adapter.limits();
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("thepaper"),
            required_features: required,
            required_limits: limits,
            ..Default::default()
        }))
        .map_err(|e| anyhow!("cannot create Vulkan device: {e}"))?;
        tracing::info!(
            adapter = %info.name,
            backend = ?info.backend,
            device_type = ?info.device_type,
            dma_buf = true,
            "Vulkan renderer initialized"
        );

        let registry_state = RegistryState::new(globals);
        let output_state = OutputState::new(globals, qh);
        let compositor_state = CompositorState::bind(globals, qh)
            .map_err(|e| anyhow!("wl_compositor is not available: {e}"))?;
        let layer_shell = LayerShell::bind(globals, qh)
            .map_err(|e| anyhow!("zwlr_layer_shell_v1 is not available: {e}"))?;
        let renderer = Renderer::new(&device, config.blur_radius())?;

        Ok(Self {
            conn: conn.clone(),
            qh: qh.clone(),
            registry_state,
            output_state,
            compositor_state,
            layer_shell,
            instance,
            adapter,
            device,
            queue,
            renderer,
            config,
            control,
            mailbox,
            decoder: None,
            outputs: HashMap::new(),
            current: None,
            retired: Vec::new(),
            submitted_gen: 0,
            completed_gen: Arc::new(AtomicU64::new(0)),
            overview_open: false,
            game_active: false,
            fatal: None,
        })
    }

    pub fn ensure_all_outputs(&mut self) {
        let outputs: Vec<_> = self.output_state.outputs().collect();
        for output in outputs {
            self.ensure_output(&output);
        }
    }

    fn ensure_output(&mut self, output: &wl_output::WlOutput) {
        let key = output.id();
        if self.outputs.contains_key(&key) {
            return;
        }
        let Some(info) = self.output_state.info(output) else {
            return;
        };
        let name = info
            .name
            .clone()
            .unwrap_or_else(|| format!("output-{}", info.id));
        let scale = u32::try_from(info.scale_factor).unwrap_or(1);
        match self.create_pair(output, &name, scale) {
            Ok(pair) => {
                tracing::info!(
                    output = %name,
                    scale,
                    background = %self.config.background_namespace(&name),
                    overview = %self.config.overview_namespace(&name),
                    "output surfaces created"
                );
                self.outputs.insert(key, pair);
            }
            Err(e) => {
                tracing::error!(output = %name, error = %format!("{e:#}"), "cannot create output surfaces");
                self.fatal = Some(format!("{e:#}"));
            }
        }
    }

    fn create_pair(
        &mut self,
        output: &wl_output::WlOutput,
        name: &str,
        scale: u32,
    ) -> Result<OutputPair> {
        let factory = SurfaceFactory::new(
            &self.instance,
            &self.conn,
            &self.compositor_state,
            &self.layer_shell,
            &self.qh,
        );
        let background = factory.create(output, &self.config.background_namespace(name), 0)?;
        let overview = factory.create(output, &self.config.overview_namespace(name), -1)?;
        Ok(OutputPair::new(
            output.clone(),
            name.to_owned(),
            scale,
            background,
            overview,
        ))
    }

    fn configure_entry(
        &mut self,
        key: &ObjectId,
        mode: DrawMode,
        configure: LayerSurfaceConfigure,
    ) {
        let Some(pair) = self.outputs.get(key) else {
            return;
        };
        let scale = pair.scale();
        let mut logical = Size::new(configure.new_size.0, configure.new_size.1);
        if logical.is_empty() {
            if let Some(info) = self.output_state.info(pair.output()) {
                if let Some(size) = info.logical_size {
                    logical = Size::new(
                        u32::try_from(size.0).unwrap_or(1),
                        u32::try_from(size.1).unwrap_or(1),
                    );
                }
            }
        }
        if logical.is_empty() {
            return;
        }
        let physical = logical.scaled(scale);
        let frame = self.current.clone();
        let should_draw = match mode {
            DrawMode::Live => true,
            DrawMode::Blur => self.overview_open,
        };
        let Some(pair) = self.outputs.get_mut(key) else {
            return;
        };
        let entry = match mode {
            DrawMode::Live => pair.live_mut(),
            DrawMode::Blur => pair.blur_mut(),
        };
        entry.layer().wl_surface().set_buffer_scale(scale as i32);
        if let Err(e) = entry.configure(&self.device, &self.adapter, physical) {
            tracing::warn!(error = %format!("{e:#}"), "surface configure failed");
            return;
        }
        tracing::debug!(
            mode = ?mode,
            configured = entry.configured(),
            size = ?entry.size(),
            physical = ?physical,
            "layer configure"
        );
        let Some(frame) = frame else {
            self.after_submit();
            return;
        };
        if !should_draw {
            self.after_submit();
            return;
        }
        if matches!(mode, DrawMode::Blur) {
            self.renderer.ensure_blur(&self.device, &self.queue, &frame);
        }
        if let Err(e) = entry.present(
            &mut self.renderer,
            &self.device,
            &self.queue,
            &self.adapter,
            mode,
            &frame,
        ) {
            tracing::warn!(error = %format!("{e:#}"), "present failed");
        }
        self.after_submit();
    }

    pub fn on_frame(&mut self, frame: VideoFrame) {
        if self.game_active {
            return;
        }
        let imported = match self.renderer.import(&self.device, frame) {
            Ok(frame) => frame,
            Err(e) => {
                self.fatal = Some(format!("{e:#}"));
                return;
            }
        };
        if self.overview_open {
            self.renderer
                .ensure_blur(&self.device, &self.queue, &imported);
        }
        if imported.seq.is_multiple_of(30) {
            tracing::debug!(seq = imported.seq, "decoded frame imported");
        }
        let old = self.current.replace(Rc::new(imported));
        if let Some(old) = old {
            let generation = self.submitted_gen;
            self.retired.push(RetiredFrame {
                _frame: old,
                generation,
            });
        }
        self.redraw(false);
    }

    fn redraw(&mut self, force: bool) {
        let Some(frame) = self.current.clone() else {
            return;
        };
        if self.overview_open {
            self.renderer.ensure_blur(&self.device, &self.queue, &frame);
        }
        let blur_valid = self.renderer.blur_valid(frame.seq);
        let App {
            outputs,
            renderer,
            device,
            queue,
            adapter,
            overview_open,
            ..
        } = self;
        for pair in outputs.values_mut() {
            let live_needs = force || pair.live().last_seq() != Some(frame.seq);
            if live_needs {
                if let Err(e) = pair.live_mut().present(
                    renderer,
                    device,
                    queue,
                    adapter,
                    DrawMode::Live,
                    &frame,
                ) {
                    tracing::warn!(
                        output = %pair.name(),
                        error = %format!("{e:#}"),
                        "live present failed"
                    );
                }
            }
            if *overview_open && pair.blur().configured() && blur_valid {
                let blur_needs = force || pair.blur().last_seq() != Some(frame.seq);
                if blur_needs {
                    if let Err(e) = pair.blur_mut().present(
                        renderer,
                        device,
                        queue,
                        adapter,
                        DrawMode::Blur,
                        &frame,
                    ) {
                        tracing::warn!(
                            output = %pair.name(),
                            error = %format!("{e:#}"),
                            "blur present failed"
                        );
                    }
                }
            }
        }
        self.after_submit();
    }

    fn after_submit(&mut self) {
        self.submitted_gen = self.submitted_gen.wrapping_add(1);
        let gen = self.submitted_gen;
        let completed = self.completed_gen.clone();
        self.queue.on_submitted_work_done(move || {
            completed.fetch_max(gen, Ordering::SeqCst);
        });
        let _ = self.device.poll(wgpu::PollType::Poll);
        self.drain_retired();
        if self.retired.len() > 2 {
            let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
            self.drain_retired();
            if self.retired.len() > 2 {
                self.retired.clear();
            }
        }
        if self.submitted_gen.is_multiple_of(30) {
            tracing::debug!(
                retired = self.retired.len(),
                submitted_gen = self.submitted_gen,
                completed_gen = self.completed_gen.load(Ordering::SeqCst),
                "frame ring state"
            );
        }
    }

    fn drain_retired(&mut self) {
        let completed = self.completed_gen.load(Ordering::SeqCst);
        self.retired
            .retain(|retired| retired.generation > completed);
    }

    pub fn on_overview(&mut self, open: bool) {
        if self.overview_open == open {
            return;
        }
        self.overview_open = open;
        tracing::info!(
            open,
            blur_passes_total = self.renderer.blur_passes(),
            "overview state changed"
        );
        tracing::debug!(
            blur_configured = ?self.outputs.values().map(|p| p.blur().configured()).collect::<Vec<_>>(),
            "overview open blur state"
        );
        if open {
            let frame = self.current.clone();
            if let Some(frame) = &frame {
                self.renderer.ensure_blur(&self.device, &self.queue, frame);
            }
            for pair in self.outputs.values_mut() {
                pair.blur_mut().set_last_seq(None);
            }
            self.redraw(true);
        }
    }

    pub fn set_decoder(&mut self, worker: Worker) {
        self.decoder = Some(worker);
    }

    pub fn shutdown_decoder(&mut self) {
        if let Some(worker) = self.decoder.take() {
            worker.shutdown();
        }
    }

    pub fn on_game(&mut self, active: bool) {
        if self.game_active == active {
            return;
        }
        self.game_active = active;
        self.control.set_game_active(active);
        match GameScheduling::from_active(active) {
            GameScheduling::Idle => {
                priority::set_game_active(GameScheduling::Idle);
                let _ = self.device.poll(wgpu::PollType::Poll);
                self.drain_retired();
                trim_heap();
                tracing::info!("game active: decoder frozen with SCHED_IDLE worker");
            }
            GameScheduling::Normal => {
                if let Some(worker) = self.decoder.as_mut() {
                    if let Err(e) = worker.resume() {
                        self.fatal = Some(format!("{e:#}"));
                        return;
                    }
                }
                priority::set_game_active(GameScheduling::Normal);
                tracing::info!("game finished: decoder resumed on SCHED_OTHER worker");
            }
        }
    }

    pub fn on_niri_connected(&mut self, connected: bool) {
        if !connected && self.overview_open {
            self.on_overview(false);
        }
        tracing::debug!(connected, "niri IPC connectivity");
    }

    pub fn poll_gpu(&mut self) {
        let _ = self.device.poll(wgpu::PollType::Poll);
        self.drain_retired();
    }

    pub fn take_fatal(&mut self) -> Option<String> {
        self.fatal.take()
    }

    pub fn on_ping(&mut self) {
        match self.mailbox.take() {
            Some(FrameMsg::Frame(frame)) => self.on_frame(frame),
            Some(FrameMsg::Error(e)) => self.fatal = Some(e),
            None => {}
        }
    }

    fn remove_output(&mut self, key: &ObjectId) {
        if let Some(pair) = self.outputs.remove(key) {
            tracing::info!(output = %pair.name(), "output surfaces destroyed");
        }
    }

    fn remove_layer(&mut self, layer: &LayerSurface) {
        let key = self.outputs.iter().find_map(|(key, pair)| {
            if pair.live().layer() == layer || pair.blur().layer() == layer {
                Some(key.clone())
            } else {
                None
            }
        });
        if let Some(key) = key {
            self.remove_output(&key);
        }
    }
}

pub fn trim_heap() {
    unsafe {
        libc::malloc_trim(0);
    }
}

impl Drop for App {
    fn drop(&mut self) {
        for pair in self.outputs.values_mut() {
            pair.live_mut().detach();
            pair.blur_mut().detach();
        }
    }
}

impl CompositorHandler for App {
    fn scale_factor_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wayland_client::protocol::wl_surface::WlSurface,
        _new_factor: i32,
    ) {
    }

    fn transform_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wayland_client::protocol::wl_surface::WlSurface,
        _new_transform: wl_output::Transform,
    ) {
    }

    fn frame(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wayland_client::protocol::wl_surface::WlSurface,
        _time: u32,
    ) {
    }

    fn surface_enter(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wayland_client::protocol::wl_surface::WlSurface,
        _output: &wl_output::WlOutput,
    ) {
    }

    fn surface_leave(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wayland_client::protocol::wl_surface::WlSurface,
        _output: &wl_output::WlOutput,
    ) {
    }
}

impl OutputHandler for App {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }

    fn new_output(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        output: wl_output::WlOutput,
    ) {
        self.ensure_output(&output);
    }

    fn update_output(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        output: wl_output::WlOutput,
    ) {
        let key = output.id();
        match self.output_state.info(&output) {
            Some(info) => {
                let scale = u32::try_from(info.scale_factor).unwrap_or(1);
                if let Some(pair) = self.outputs.get_mut(&key) {
                    pair.set_scale(scale);
                }
                self.ensure_output(&output);
            }
            None => {
                self.remove_output(&key);
            }
        }
    }

    fn output_destroyed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        output: wl_output::WlOutput,
    ) {
        self.remove_output(&output.id());
    }
}

impl LayerShellHandler for App {
    fn closed(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, layer: &LayerSurface) {
        self.remove_layer(layer);
    }

    fn configure(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        layer: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _serial: u32,
    ) {
        let target = self.outputs.iter().find_map(|(key, pair)| {
            if pair.live().layer() == layer {
                Some((key.clone(), DrawMode::Live))
            } else if pair.blur().layer() == layer {
                Some((key.clone(), DrawMode::Blur))
            } else {
                None
            }
        });
        if let Some((key, mode)) = target {
            self.configure_entry(&key, mode, configure);
        }
    }
}

impl ProvidesRegistryState for App {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    registry_handlers![OutputState];
}

delegate_registry!(App);
smithay_client_toolkit::delegate_dispatch2!(App);
