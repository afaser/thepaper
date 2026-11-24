use anyhow::{anyhow, bail, Context, Result};
use smithay_client_toolkit::{
    compositor::CompositorState,
    shell::{
        wlr_layer::{Anchor, KeyboardInteractivity, Layer, LayerShell, LayerSurface},
        WaylandSurface,
    },
};
use wayland_client::{
    protocol::{wl_output, wl_surface},
    Connection, QueueHandle,
};

use crate::app::App;
use crate::render::{GpuFrame, Renderer};
use crate::size::Size;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrawMode {
    Live,
    Blur,
}

pub struct SurfaceEntry {
    surface: Option<wgpu::Surface<'static>>,
    layer: LayerSurface,
    format: wgpu::TextureFormat,
    size: Size,
    configured: bool,
    last_seq: Option<u64>,
}

pub struct SurfaceFactory<'a> {
    pub instance: &'a wgpu::Instance,
    pub conn: &'a Connection,
    pub compositor: &'a CompositorState,
    pub layer_shell: &'a LayerShell,
    pub qh: &'a QueueHandle<App>,
}

impl<'a> SurfaceFactory<'a> {
    pub fn new(
        instance: &'a wgpu::Instance,
        conn: &'a Connection,
        compositor: &'a CompositorState,
        layer_shell: &'a LayerShell,
        qh: &'a QueueHandle<App>,
    ) -> Self {
        Self {
            instance,
            conn,
            compositor,
            layer_shell,
            qh,
        }
    }

    pub fn create(
        &self,
        output: &wl_output::WlOutput,
        namespace: &str,
        exclusive_zone: i32,
    ) -> Result<SurfaceEntry> {
        let wl_surface = self.compositor.create_surface(self.qh);
        let layer = self.layer_shell.create_layer_surface(
            self.qh,
            wl_surface,
            Layer::Background,
            Some(namespace.to_owned()),
            Some(output),
        );
        layer.set_size(0, 0);
        layer.set_anchor(Anchor::TOP | Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT);
        layer.set_exclusive_zone(exclusive_zone);
        layer.set_keyboard_interactivity(KeyboardInteractivity::None);
        let surface = surface_from_wl(self.instance, self.conn, layer.wl_surface())?;
        layer.commit();
        Ok(SurfaceEntry {
            surface: Some(surface),
            layer,
            format: wgpu::TextureFormat::Bgra8Unorm,
            size: Size::new(0, 0),
            configured: false,
            last_seq: None,
        })
    }
}

impl SurfaceEntry {
    pub fn layer(&self) -> &LayerSurface {
        &self.layer
    }

    pub fn configured(&self) -> bool {
        self.configured
    }

    pub fn size(&self) -> Size {
        self.size
    }

    pub fn last_seq(&self) -> Option<u64> {
        self.last_seq
    }

    pub fn set_last_seq(&mut self, seq: Option<u64>) {
        self.last_seq = seq;
    }

    pub fn configure(
        &mut self,
        device: &wgpu::Device,
        adapter: &wgpu::Adapter,
        size: Size,
    ) -> Result<()> {
        if size.is_empty() {
            return Ok(());
        }
        if self.configured && self.size == size {
            return Ok(());
        }
        let Some(surface) = self.surface.as_ref() else {
            return Ok(());
        };
        let caps = surface.get_capabilities(adapter);
        if caps.formats.is_empty() {
            bail!("surface has no supported formats");
        }
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| {
                matches!(
                    f,
                    wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Rgba8Unorm
                )
            })
            .or_else(|| caps.formats.iter().copied().find(|f| !f.is_srgb()))
            .unwrap_or(caps.formats[0]);
        let mut cfg = surface
            .get_default_config(adapter, size.width, size.height)
            .ok_or_else(|| anyhow!("no default surface configuration"))?;
        cfg.format = format;
        cfg.usage = wgpu::TextureUsages::RENDER_ATTACHMENT;
        cfg.present_mode = wgpu::PresentMode::Fifo;
        cfg.desired_maximum_frame_latency = 2;
        cfg.alpha_mode = wgpu::CompositeAlphaMode::Opaque;
        surface.configure(device, &cfg);
        self.format = format;
        self.size = size;
        self.configured = true;
        self.last_seq = None;
        Ok(())
    }

    pub fn detach(&mut self) {
        self.configured = false;
        self.last_seq = None;
        self.layer.wl_surface().attach(None, 0, 0);
        self.layer.commit();
    }

    pub fn present(
        &mut self,
        renderer: &mut Renderer,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        adapter: &wgpu::Adapter,
        mode: DrawMode,
        frame: &GpuFrame,
    ) -> Result<()> {
        if !self.configured {
            return Ok(());
        }
        let mut attempts = 0;
        let acquired = loop {
            let Some(surface) = self.surface.as_ref() else {
                return Ok(());
            };
            match surface.get_current_texture() {
                wgpu::CurrentSurfaceTexture::Success(t)
                | wgpu::CurrentSurfaceTexture::Suboptimal(t) => break t,
                wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                    return Ok(())
                }
                wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                    attempts += 1;
                    if attempts > 2 {
                        bail!("surface stayed outdated/lost");
                    }
                    self.configured = false;
                    self.configure(device, adapter, self.size)?;
                }
                wgpu::CurrentSurfaceTexture::Validation => {
                    bail!("surface texture validation error")
                }
            }
        };
        let view = acquired.texture.create_view(&Default::default());
        match mode {
            DrawMode::Live => {
                renderer.draw_live(device, queue, &view, self.format, self.size, frame)
            }
            DrawMode::Blur => renderer.draw_blur(device, queue, &view, self.format, self.size),
        }
        queue.present(acquired);
        self.last_seq = Some(frame.seq);
        Ok(())
    }
}

pub struct OutputPair {
    output: wl_output::WlOutput,
    name: String,
    scale: u32,
    live: SurfaceEntry,
    blur: SurfaceEntry,
}

impl OutputPair {
    pub fn new(
        output: wl_output::WlOutput,
        name: String,
        scale: u32,
        live: SurfaceEntry,
        blur: SurfaceEntry,
    ) -> Self {
        Self {
            output,
            name,
            scale: clamped_scale(scale),
            live,
            blur,
        }
    }

    pub fn output(&self) -> &wl_output::WlOutput {
        &self.output
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn scale(&self) -> u32 {
        self.scale
    }

    pub fn set_scale(&mut self, scale: u32) {
        self.scale = clamped_scale(scale);
    }

    pub fn live(&self) -> &SurfaceEntry {
        &self.live
    }

    pub fn live_mut(&mut self) -> &mut SurfaceEntry {
        &mut self.live
    }

    pub fn blur(&self) -> &SurfaceEntry {
        &self.blur
    }

    pub fn blur_mut(&mut self) -> &mut SurfaceEntry {
        &mut self.blur
    }
}

fn clamped_scale(scale: u32) -> u32 {
    scale.max(1)
}

fn surface_from_wl(
    instance: &wgpu::Instance,
    display: &Connection,
    layer_surface: &wl_surface::WlSurface,
) -> Result<wgpu::Surface<'static>> {
    use raw_window_handle::{
        RawDisplayHandle, RawWindowHandle, WaylandDisplayHandle, WaylandWindowHandle,
    };
    use std::ptr::NonNull;
    use wayland_client::Proxy as _;

    let raw_display = RawDisplayHandle::Wayland(WaylandDisplayHandle::new(
        NonNull::new(display.backend().display_ptr() as *mut _)
            .context("wl_display pointer is null")?,
    ));
    let raw_window = RawWindowHandle::Wayland(WaylandWindowHandle::new(
        NonNull::new(layer_surface.id().as_ptr() as *mut _)
            .context("wl_surface pointer is null")?,
    ));
    unsafe {
        instance
            .create_surface_unsafe(wgpu::SurfaceTargetUnsafe::RawHandle {
                raw_display_handle: Some(raw_display),
                raw_window_handle: raw_window,
            })
            .map_err(|e| anyhow!("cannot create Vulkan surface for layer surface: {e}"))
    }
}
