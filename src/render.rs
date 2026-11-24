use std::collections::HashMap;
use std::os::fd::{FromRawFd, OwnedFd};
use std::sync::Arc;

use anyhow::{anyhow, bail, Context, Result};

use crate::frame::{ColorInfo, PlaneFormat, VideoFrame};
use crate::size::Size;

const IMMEDIATE_SIZE: u32 = 96;

#[repr(C)]
#[derive(Clone, Copy)]
struct Params {
    src_size: [f32; 2],
    dst_size: [f32; 2],
    y_off: f32,
    y_scale: f32,
    c_scale: f32,
    radius: f32,
    mat0: [f32; 4],
    mat1: [f32; 4],
    mat2: [f32; 4],
    dir: [f32; 2],
    dst_srgb: f32,
    _pad: f32,
}

impl Params {
    fn new(src: Size, dst: Size, color: ColorInfo, radius: f32) -> Self {
        Self {
            src_size: src.as_f32(),
            dst_size: dst.as_f32(),
            y_off: color.y_off,
            y_scale: color.y_scale,
            c_scale: color.c_scale,
            radius,
            mat0: color.matrix[0],
            mat1: color.matrix[1],
            mat2: color.matrix[2],
            dir: [0.0; 2],
            dst_srgb: 0.0,
            _pad: 0.0,
        }
    }

    fn bytes(&self) -> &[u8] {
        let ptr = (self as *const Self).cast::<u8>();
        unsafe { std::slice::from_raw_parts(ptr, std::mem::size_of::<Self>()) }
    }
}

struct SurfacePipelines {
    live: wgpu::RenderPipeline,
    blur_draw: wgpu::RenderPipeline,
}

struct CachedFrame {
    width: u32,
    height: u32,
    _y: wgpu::Texture,
    _uv: wgpu::Texture,
    y_view: wgpu::TextureView,
    uv_view: wgpu::TextureView,
}

pub struct GpuFrame {
    pub seq: u64,
    pub width: u32,
    pub height: u32,
    pub color: ColorInfo,
    bind_group: wgpu::BindGroup,
    _cached: Arc<CachedFrame>,
    _frame: VideoFrame,
}

struct BlurTargets {
    width: u32,
    height: u32,
    _ping: wgpu::Texture,
    _blur: wgpu::Texture,
    ping_view: wgpu::TextureView,
    blur_view: wgpu::TextureView,
    ping_bind: wgpu::BindGroup,
    blur_bind: wgpu::BindGroup,
}

pub struct Renderer {
    shader: wgpu::ShaderModule,
    bgl: wgpu::BindGroupLayout,
    layout: wgpu::PipelineLayout,
    sampler: wgpu::Sampler,
    dummy: wgpu::TextureView,
    blur_h: wgpu::RenderPipeline,
    blur_v: wgpu::RenderPipeline,
    surfaces: HashMap<wgpu::TextureFormat, SurfacePipelines>,
    blur_targets: BlurTargets,
    blur_valid_seq: Option<u64>,
    blur_passes: u64,
    frame_cache: HashMap<u64, Arc<CachedFrame>>,
    blur_radius: u32,
}

impl Renderer {
    pub fn new(device: &wgpu::Device, blur_radius: u32) -> Result<Self> {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("thepaper shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });

        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("yuv/rgba"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("thepaper layout"),
            bind_group_layouts: &[Some(&bgl)],
            immediate_size: IMMEDIATE_SIZE,
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("linear clamp"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });

        let dummy_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("dummy"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let dummy = dummy_tex.create_view(&Default::default());

        let blur_h = create_pipeline(
            device,
            &layout,
            &shader,
            "fs_blur_yuv",
            wgpu::TextureFormat::Rgba8Unorm,
        );
        let blur_v = create_pipeline(
            device,
            &layout,
            &shader,
            "fs_blur_rgba",
            wgpu::TextureFormat::Rgba8Unorm,
        );
        let blur_targets = make_blur_targets(device, &bgl, &sampler, &dummy, Size::new(1, 1));

        Ok(Self {
            shader,
            bgl,
            layout,
            sampler,
            dummy,
            blur_h,
            blur_v,
            surfaces: HashMap::new(),
            blur_targets,
            blur_valid_seq: None,
            blur_passes: 0,
            frame_cache: HashMap::new(),
            blur_radius,
        })
    }

    fn pipelines(
        &mut self,
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
    ) -> &SurfacePipelines {
        self.surfaces
            .entry(format)
            .or_insert_with(|| SurfacePipelines {
                live: create_pipeline(device, &self.layout, &self.shader, "fs_live", format),
                blur_draw: create_pipeline(
                    device,
                    &self.layout,
                    &self.shader,
                    "fs_blur_draw",
                    format,
                ),
            })
    }

    fn bind_group_for(&self, device: &wgpu::Device, cached: &CachedFrame) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("frame"),
            layout: &self.bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&cached.y_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&cached.uv_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        })
    }

    pub fn import(&mut self, device: &wgpu::Device, frame: VideoFrame) -> Result<GpuFrame> {
        let key = frame_key(&frame)?;
        if let Some(cached) = self.frame_cache.get(&key).cloned() {
            let bind_group = self.bind_group_for(device, &cached);
            return Ok(GpuFrame {
                seq: frame.seq,
                width: cached.width,
                height: cached.height,
                color: frame.color,
                bind_group,
                _cached: cached,
                _frame: frame,
            });
        }

        let mut textures = Vec::with_capacity(frame.planes.len());
        for plane in &frame.planes {
            let fd = unsafe { libc::dup(plane.fd) };
            if fd < 0 {
                bail!("dup DMA-BUF fd: {}", std::io::Error::last_os_error());
            }
            let raw_fd = fd;
            let owned = unsafe { OwnedFd::from_raw_fd(fd) };
            let format = match plane.format {
                PlaneFormat::R8 => wgpu::TextureFormat::R8Unorm,
                PlaneFormat::Rg8 => wgpu::TextureFormat::Rg8Unorm,
            };
            let hal_desc = wgpu::hal::TextureDescriptor {
                label: Some("dmabuf plane"),
                size: wgpu::Extent3d {
                    width: plane.width,
                    height: plane.height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUses::RESOURCE,
                memory_flags: wgpu::hal::MemoryFlags::empty(),
                view_formats: vec![],
            };
            let hal_device = unsafe { device.as_hal::<wgpu::hal::vulkan::Api>() }
                .ok_or_else(|| anyhow!("wgpu device is not a Vulkan device"))?;
            let hal_texture = match unsafe {
                hal_device.texture_from_dmabuf_fd(
                    owned,
                    &hal_desc,
                    plane.modifier,
                    plane.stride,
                    plane.offset,
                )
            } {
                Ok(texture) => {
                    unsafe { libc::close(raw_fd) };
                    texture
                }
                Err(e) => return Err(anyhow!("importing DMA-BUF plane failed: {e:?}")),
            };
            let desc = wgpu::TextureDescriptor {
                label: Some("dmabuf plane"),
                size: hal_desc.size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            };
            let texture = unsafe {
                device.create_texture_from_hal::<wgpu::hal::vulkan::Api>(
                    hal_texture,
                    &desc,
                    wgpu::TextureUses::UNINITIALIZED,
                )
            };
            textures.push(texture);
        }
        if textures.len() < 2 {
            bail!("expected 2 DMA-BUF planes, got {}", textures.len());
        }
        let y = textures.remove(0);
        let uv = textures.remove(0);
        let y_view = y.create_view(&Default::default());
        let uv_view = uv.create_view(&Default::default());
        let cached = Arc::new(CachedFrame {
            width: frame.width,
            height: frame.height,
            _y: y,
            _uv: uv,
            y_view,
            uv_view,
        });
        let bind_group = self.bind_group_for(device, &cached);
        if self.frame_cache.len() >= 32 {
            self.frame_cache.clear();
        }
        self.frame_cache.insert(key, cached.clone());
        Ok(GpuFrame {
            seq: frame.seq,
            width: cached.width,
            height: cached.height,
            color: frame.color,
            bind_group,
            _cached: cached,
            _frame: frame,
        })
    }

    fn ensure_blur_targets(&mut self, device: &wgpu::Device, size: Size) -> &BlurTargets {
        if self.blur_targets.width == size.width && self.blur_targets.height == size.height {
            return &self.blur_targets;
        }
        self.blur_targets = make_blur_targets(device, &self.bgl, &self.sampler, &self.dummy, size);
        self.blur_valid_seq = None;
        &self.blur_targets
    }

    pub fn draw_live(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        view: &wgpu::TextureView,
        format: wgpu::TextureFormat,
        out_size: Size,
        frame: &GpuFrame,
    ) {
        let pipeline = self.pipelines(device, format).live.clone();
        let mut params = Params::new(
            Size::new(frame.width, frame.height),
            out_size,
            frame.color,
            self.blur_radius as f32,
        );
        params.dst_srgb = if format.is_srgb() { 1.0 } else { 0.0 };
        let mut encoder = device.create_command_encoder(&Default::default());
        draw_triangle(
            &mut encoder,
            "live",
            &pipeline,
            &frame.bind_group,
            view,
            &params,
        );
        queue.submit([encoder.finish()]);
    }

    pub fn ensure_blur(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, frame: &GpuFrame) {
        if self.blur_valid(frame.seq) {
            return;
        }
        let size = Size::new(frame.width, frame.height);
        let spacing = (self.blur_radius as f32 / 6.0).max(0.5);
        let mut h = Params::new(size, size, frame.color, self.blur_radius as f32);
        h.dir = [spacing / frame.width as f32, 0.0];
        let mut v = Params::new(size, size, frame.color, self.blur_radius as f32);
        v.dir = [0.0, spacing / frame.height as f32];

        let blur_h = self.blur_h.clone();
        let blur_v = self.blur_v.clone();
        let targets = self.ensure_blur_targets(device, size);
        let mut encoder = device.create_command_encoder(&Default::default());
        draw_triangle(
            &mut encoder,
            "blur h",
            &blur_h,
            &frame.bind_group,
            &targets.ping_view,
            &h,
        );
        draw_triangle(
            &mut encoder,
            "blur v",
            &blur_v,
            &targets.ping_bind,
            &targets.blur_view,
            &v,
        );
        queue.submit([encoder.finish()]);
        self.blur_valid_seq = Some(frame.seq);
        self.blur_passes += 1;
    }

    pub fn draw_blur(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        view: &wgpu::TextureView,
        format: wgpu::TextureFormat,
        out_size: Size,
    ) {
        let pipeline = self.pipelines(device, format).blur_draw.clone();
        let targets = &self.blur_targets;
        let mut params = Params::new(
            Size::new(targets.width, targets.height),
            out_size,
            ColorInfo {
                y_off: 0.0,
                y_scale: 1.0,
                c_scale: 1.0,
                matrix: [
                    [1.0, 0.0, 0.0, 0.0],
                    [0.0, 1.0, 0.0, 0.0],
                    [0.0, 0.0, 1.0, 0.0],
                ],
            },
            self.blur_radius as f32,
        );
        params.dst_srgb = if format.is_srgb() { 1.0 } else { 0.0 };
        let mut encoder = device.create_command_encoder(&Default::default());
        draw_triangle(
            &mut encoder,
            "blur draw",
            &pipeline,
            &targets.blur_bind,
            view,
            &params,
        );
        queue.submit([encoder.finish()]);
    }

    pub fn blur_valid(&self, seq: u64) -> bool {
        self.blur_valid_seq == Some(seq)
    }

    pub fn blur_passes(&self) -> u64 {
        self.blur_passes
    }
}

fn make_blur_targets(
    device: &wgpu::Device,
    bgl: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    dummy: &wgpu::TextureView,
    size: Size,
) -> BlurTargets {
    let make = |label: &str| {
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width: size.width,
                height: size.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
    };
    let ping = make("blur ping");
    let blur = make("blur cache");
    let ping_view = ping.create_view(&Default::default());
    let blur_view = blur.create_view(&Default::default());
    let bind = |label: &str, view: &wgpu::TextureView| {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(label),
            layout: bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(dummy),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        })
    };
    let ping_bind = bind("ping", &ping_view);
    let blur_bind = bind("blur", &blur_view);
    BlurTargets {
        width: size.width,
        height: size.height,
        _ping: ping,
        _blur: blur,
        ping_view,
        blur_view,
        ping_bind,
        blur_bind,
    }
}

fn draw_triangle(
    encoder: &mut wgpu::CommandEncoder,
    label: &str,
    pipeline: &wgpu::RenderPipeline,
    bind_group: &wgpu::BindGroup,
    view: &wgpu::TextureView,
    params: &Params,
) {
    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some(label),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });
    pass.set_pipeline(pipeline);
    pass.set_bind_group(0, bind_group, &[]);
    pass.set_immediates(0, params.bytes());
    pass.draw(0..3, 0..1);
}

fn create_pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    fragment_entry: &str,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(fragment_entry),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        primitive: Default::default(),
        depth_stencil: None,
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some(fragment_entry),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::REPLACE),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

fn frame_key(frame: &VideoFrame) -> Result<u64> {
    fn inode(fd: std::os::fd::RawFd) -> Result<u64> {
        let mut st: libc::stat = unsafe { std::mem::zeroed() };
        let r = unsafe { libc::fstat(fd, &mut st) };
        if r != 0 {
            bail!("fstat DMA-BUF fd: {}", std::io::Error::last_os_error());
        }
        Ok(st.st_ino)
    }
    let plane = frame
        .planes
        .first()
        .context("frame has no DMA-BUF planes")?;
    inode(plane.fd)
}
