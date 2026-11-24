use std::ffi::{CStr, CString};
use std::path::PathBuf;
use std::ptr::{null, null_mut};
use std::time::Duration;

use anyhow::{bail, Context, Result};
use ffmpeg_sys_next as ffi;

use crate::frame::{ColorInfo, DmaPlane, PlaneFormat, VideoFrame};

pub fn render_node() -> String {
    if let Ok(node) = std::env::var("THEPAPER_DRM_RENDER_NODE") {
        return node;
    }
    for n in 128..136 {
        let path = format!("/dev/dri/renderD{n}");
        if std::path::Path::new(&path).exists() {
            return path;
        }
    }
    "/dev/dri/renderD128".to_owned()
}

fn averr(code: i32) -> String {
    let mut buf = [0i8; 256];
    unsafe {
        ffi::av_strerror(code, buf.as_mut_ptr(), buf.len());
        CStr::from_ptr(buf.as_ptr()).to_string_lossy().into_owned()
    }
}

pub struct AvFrame(*mut ffi::AVFrame);

unsafe impl Send for AvFrame {}

impl AvFrame {
    pub fn alloc() -> Result<Self> {
        let p = unsafe { ffi::av_frame_alloc() };
        if p.is_null() {
            bail!("av_frame_alloc failed");
        }
        Ok(Self(p))
    }

    pub fn as_ptr(&self) -> *mut ffi::AVFrame {
        self.0
    }
}

impl Drop for AvFrame {
    fn drop(&mut self) {
        unsafe { ffi::av_frame_free(&mut self.0) };
    }
}

struct FormatCtx(*mut ffi::AVFormatContext);

impl Drop for FormatCtx {
    fn drop(&mut self) {
        unsafe { ffi::avformat_close_input(&mut self.0) };
    }
}

struct CodecCtx(*mut ffi::AVCodecContext);

impl Drop for CodecCtx {
    fn drop(&mut self) {
        unsafe { ffi::avcodec_free_context(&mut self.0) };
    }
}

struct HwDevice(*mut ffi::AVBufferRef);

impl Drop for HwDevice {
    fn drop(&mut self) {
        unsafe { ffi::av_buffer_unref(&mut self.0) };
    }
}

struct Packet(*mut ffi::AVPacket);

impl Drop for Packet {
    fn drop(&mut self) {
        unsafe { ffi::av_packet_free(&mut self.0) };
    }
}

unsafe extern "C" fn get_format(
    _ctx: *mut ffi::AVCodecContext,
    fmts: *const ffi::AVPixelFormat,
) -> ffi::AVPixelFormat {
    let mut p = fmts;
    while *p != ffi::AVPixelFormat::AV_PIX_FMT_NONE {
        if *p == ffi::AVPixelFormat::AV_PIX_FMT_VAAPI {
            return *p;
        }
        p = p.add(1);
    }
    ffi::AVPixelFormat::AV_PIX_FMT_NONE
}

const FOURCC_R8: u32 = u32::from_le_bytes(*b"R8  ");
const FOURCC_GR88: u32 = u32::from_le_bytes(*b"GR88");

unsafe impl Send for Decoder {}

enum ReceiveOutcome {
    Frame(AvFrame),
    NeedPacket,
    Eof,
}

pub struct Decoder {
    _path: PathBuf,
    _node: CString,
    fmt: FormatCtx,
    dec: CodecCtx,
    _hw: HwDevice,
    stream_index: i32,
    interval: Duration,
    packet: Packet,
    seq: u64,
    eof_sent: bool,
    frames_emitted: u64,
}

impl Decoder {
    pub fn open(path: PathBuf) -> Result<Self> {
        let node = CString::new(render_node()).context("render node path contains a NUL byte")?;
        Self::open_inner(path, node)
    }

    fn open_inner(path: PathBuf, node: CString) -> Result<Self> {
        unsafe { ffi::av_log_set_level(ffi::AV_LOG_WARNING) };
        let cpath = CString::new(path.to_string_lossy().as_bytes())
            .with_context(|| format!("invalid video path {}", path.display()))?;
        let mut raw_fmt: *mut ffi::AVFormatContext = null_mut();
        let r =
            unsafe { ffi::avformat_open_input(&mut raw_fmt, cpath.as_ptr(), null(), null_mut()) };
        if r < 0 {
            bail!("cannot open video {}: {}", path.display(), averr(r));
        }
        let fmt = FormatCtx(raw_fmt);
        let r = unsafe { ffi::avformat_find_stream_info(fmt.0, null_mut()) };
        if r < 0 {
            bail!("cannot read stream info: {}", averr(r));
        }

        let mut codec: *const ffi::AVCodec = null();
        let stream_index = unsafe {
            ffi::av_find_best_stream(
                fmt.0,
                ffi::AVMediaType::AVMEDIA_TYPE_VIDEO,
                -1,
                -1,
                &mut codec,
                0,
            )
        };
        if stream_index < 0 {
            bail!("video {} has no video stream", path.display());
        }
        let stream = unsafe { *(*fmt.0).streams.add(stream_index as usize) };
        let par = unsafe { (*stream).codecpar };

        let raw_dec = unsafe { ffi::avcodec_alloc_context3(codec) };
        if raw_dec.is_null() {
            bail!("avcodec_alloc_context3 failed");
        }
        let dec = CodecCtx(raw_dec);
        let r = unsafe { ffi::avcodec_parameters_to_context(dec.0, par) };
        if r < 0 {
            bail!("cannot copy codec parameters: {}", averr(r));
        }

        let mut hw_ref: *mut ffi::AVBufferRef = null_mut();
        let r = unsafe {
            ffi::av_hwdevice_ctx_create(
                &mut hw_ref,
                ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_VAAPI,
                node.as_ptr(),
                null_mut(),
                0,
            )
        };
        if r < 0 {
            bail!(
                "cannot create VA-API device on {}: {} (hardware decode is mandatory)",
                node.to_string_lossy(),
                averr(r)
            );
        }
        let hw = HwDevice(hw_ref);
        unsafe {
            (*dec.0).hw_device_ctx = ffi::av_buffer_ref(hw.0);
            (*dec.0).get_format = Some(get_format);
        }
        let r = unsafe { ffi::avcodec_open2(dec.0, codec, null_mut()) };
        if r < 0 {
            let codec_name = unsafe { CStr::from_ptr((*codec).name) };
            bail!(
                "cannot open VA-API decoder for {}: {}",
                codec_name.to_string_lossy(),
                averr(r)
            );
        }

        let codec_name = unsafe { CStr::from_ptr((*codec).name) }
            .to_string_lossy()
            .into_owned();
        let decoder_name = unsafe {
            (*dec.0)
                .codec
                .as_ref()
                .map(|c| CStr::from_ptr(c.name).to_string_lossy().into_owned())
                .unwrap_or_default()
        };
        let avfps = unsafe { ffi::av_guess_frame_rate(fmt.0, stream, null_mut()) };
        let fps = if avfps.num > 0 && avfps.den > 0 {
            avfps.num as f64 / avfps.den as f64
        } else {
            15.0
        };
        let frame_width = unsafe { (*par).width };
        let frame_height = unsafe { (*par).height };
        tracing::info!(
            codec = %codec_name,
            decoder = %decoder_name,
            width = frame_width,
            height = frame_height,
            fps,
            render_node = %node.to_string_lossy(),
            "VA-API decoder opened"
        );

        Ok(Self {
            _path: path,
            _node: node,
            fmt,
            dec,
            _hw: hw,
            stream_index,
            interval: Duration::from_secs_f64(1.0 / fps.max(1.0)),
            packet: Packet(unsafe { ffi::av_packet_alloc() }),
            seq: 0,
            eof_sent: false,
            frames_emitted: 0,
        })
    }

    pub fn interval(&self) -> Duration {
        self.interval
    }

    fn receive_frame(&mut self) -> Result<ReceiveOutcome> {
        let frame = AvFrame::alloc()?;
        let r = unsafe { ffi::avcodec_receive_frame(self.dec.0, frame.as_ptr()) };
        if r == -libc::EAGAIN {
            return Ok(ReceiveOutcome::NeedPacket);
        }
        if r == ffi::AVERROR_EOF {
            self.eof_sent = false;
            return Ok(ReceiveOutcome::Eof);
        }
        if r < 0 {
            bail!("avcodec_receive_frame: {}", averr(r));
        }
        Ok(ReceiveOutcome::Frame(frame))
    }

    pub fn next_frame(&mut self) -> Result<VideoFrame> {
        loop {
            match self.receive_frame()? {
                ReceiveOutcome::Frame(va) => return self.map_frame(va),
                ReceiveOutcome::Eof => {
                    self.seek_start()?;
                    continue;
                }
                ReceiveOutcome::NeedPacket => {}
            }

            if self.eof_sent {
                std::thread::yield_now();
                continue;
            }

            let r = unsafe { ffi::av_read_frame(self.fmt.0, self.packet.0) };
            if r == ffi::AVERROR_EOF {
                unsafe {
                    let r = ffi::avcodec_send_packet(self.dec.0, null());
                    if r < 0 && r != -libc::EAGAIN && r != ffi::AVERROR_EOF {
                        bail!("draining decoder failed: {}", averr(r));
                    }
                }
                self.eof_sent = true;
                continue;
            }
            if r < 0 {
                bail!("av_read_frame: {}", averr(r));
            }
            if unsafe { (*self.packet.0).stream_index } != self.stream_index {
                unsafe { ffi::av_packet_unref(self.packet.0) };
                continue;
            }
            let r = unsafe { ffi::avcodec_send_packet(self.dec.0, self.packet.0) };
            unsafe { ffi::av_packet_unref(self.packet.0) };
            if r < 0 && r != -libc::EAGAIN {
                bail!("avcodec_send_packet: {}", averr(r));
            }
        }
    }

    fn seek_start(&mut self) -> Result<()> {
        tracing::debug!("end of video, looping");
        let r = unsafe {
            ffi::av_seek_frame(self.fmt.0, self.stream_index, 0, ffi::AVSEEK_FLAG_BACKWARD)
        };
        if r < 0 {
            let r2 = unsafe {
                ffi::avformat_seek_file(self.fmt.0, self.stream_index, i64::MIN, 0, i64::MAX, 0)
            };
            if r2 < 0 {
                bail!("cannot seek to start for looping: {}", averr(r2));
            }
        }
        unsafe {
            ffi::avformat_flush(self.fmt.0);
            ffi::avcodec_flush_buffers(self.dec.0);
        }
        self.eof_sent = false;
        Ok(())
    }

    fn map_frame(&mut self, va: AvFrame) -> Result<VideoFrame> {
        let frame = unsafe { &*va.as_ptr() };
        if frame.format != ffi::AVPixelFormat::AV_PIX_FMT_VAAPI as i32 {
            bail!(
                "decoder returned non-VAAPI frame format {} (software decode is forbidden)",
                frame.format
            );
        }
        let mapped = AvFrame::alloc()?;
        unsafe {
            (*mapped.as_ptr()).format = ffi::AVPixelFormat::AV_PIX_FMT_DRM_PRIME as i32;
            let r = ffi::av_hwframe_map(
                mapped.as_ptr(),
                va.as_ptr(),
                ffi::AV_HWFRAME_MAP_READ as i32,
            );
            if r < 0 {
                bail!("cannot map VA-API frame to DRM_PRIME: {}", averr(r));
            }
        }
        let drm = unsafe { &*mapped.as_ptr() };
        let desc = unsafe { &*(drm.data[0].cast::<ffi::AVDRMFrameDescriptor>()) };
        if desc.nb_layers <= 0 || desc.nb_objects <= 0 {
            bail!("DRM_PRIME frame has no layers/objects");
        }

        let width = frame.width as u32;
        let height = frame.height as u32;
        let layer_count = usize::try_from(desc.nb_layers).unwrap_or(0);
        let plane_descriptors = desc.layers.iter().take(layer_count).flat_map(|layer| {
            let plane_count = usize::try_from(layer.nb_planes).unwrap_or(0);
            layer
                .planes
                .iter()
                .take(plane_count)
                .map(move |plane| (layer.format, plane))
        });
        let planes = plane_descriptors
            .take(2)
            .map(|(layer_format, plane)| {
                let (format, plane_width, plane_height) = match layer_format {
                    FOURCC_R8 => (PlaneFormat::R8, width, height),
                    FOURCC_GR88 => (PlaneFormat::Rg8, width.div_ceil(2), height.div_ceil(2)),
                    other => {
                        let bytes = other.to_le_bytes();
                        bail!(
                            "unsupported DRM_PRIME layer format {:?} (only 8-bit NV12 is supported)",
                            String::from_utf8_lossy(&bytes)
                        );
                    }
                };
                let object = desc
                    .objects
                    .get(plane.object_index as usize)
                    .context("DRM_PRIME plane references a missing object")?;
                let fd = unsafe { libc::dup(object.fd) };
                if fd < 0 {
                    bail!("dup of DMA-BUF fd failed: {}", std::io::Error::last_os_error());
                }
                Ok(DmaPlane {
                    fd,
                    modifier: object.format_modifier,
                    stride: plane.pitch as u64,
                    offset: plane.offset as u64,
                    width: plane_width,
                    height: plane_height,
                    format,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        if planes.len() < 2 {
            bail!("expected 2 DMA-BUF planes (NV12), got {}", planes.len());
        }

        let seq = self.seq;
        self.seq = self.seq.wrapping_add(1);
        self.frames_emitted += 1;
        Ok(VideoFrame::new(
            seq,
            width,
            height,
            planes,
            ColorInfo::from_frame(frame),
            va,
            mapped,
        ))
    }

    pub fn frames_emitted(&self) -> u64 {
        self.frames_emitted
    }
}
