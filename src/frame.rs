use std::os::fd::RawFd;

use ffmpeg_sys_next as ffi;

use crate::decoder::AvFrame;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaneFormat {
    R8,
    Rg8,
}

#[derive(Debug)]
pub struct DmaPlane {
    pub fd: RawFd,
    pub modifier: u64,
    pub stride: u64,
    pub offset: u64,
    pub width: u32,
    pub height: u32,
    pub format: PlaneFormat,
}

impl Drop for DmaPlane {
    fn drop(&mut self) {
        unsafe { libc::close(self.fd) };
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ColorInfo {
    pub y_off: f32,
    pub y_scale: f32,
    pub c_scale: f32,
    pub matrix: [[f32; 4]; 3],
}

impl ColorInfo {
    pub fn from_frame(frame: &ffi::AVFrame) -> Self {
        let limited = !matches!(frame.color_range, ffi::AVColorRange::AVCOL_RANGE_JPEG);
        let (y_off, y_scale, c_scale) = if limited {
            (16.0 / 255.0, 255.0 / 219.0, 255.0 / 224.0)
        } else {
            (0.0, 1.0, 1.0)
        };
        let matrix = match frame.colorspace {
            ffi::AVColorSpace::AVCOL_SPC_BT2020_NCL | ffi::AVColorSpace::AVCOL_SPC_BT2020_CL => [
                [1.0, 0.0, 1.4746, 0.0],
                [1.0, -0.16455, -0.57135, 0.0],
                [1.0, 1.8814, 0.0, 0.0],
            ],
            ffi::AVColorSpace::AVCOL_SPC_SMPTE170M
            | ffi::AVColorSpace::AVCOL_SPC_BT470BG
            | ffi::AVColorSpace::AVCOL_SPC_FCC => [
                [1.0, 0.0, 1.402, 0.0],
                [1.0, -0.344136, -0.714136, 0.0],
                [1.0, 1.772, 0.0, 0.0],
            ],
            _ => [
                [1.0, 0.0, 1.5748, 0.0],
                [1.0, -0.1873, -0.4681, 0.0],
                [1.0, 1.8556, 0.0, 0.0],
            ],
        };
        Self {
            y_off,
            y_scale,
            c_scale,
            matrix,
        }
    }
}

pub struct VideoFrame {
    pub seq: u64,
    pub width: u32,
    pub height: u32,
    pub planes: Vec<DmaPlane>,
    pub color: ColorInfo,
    _va: AvFrame,
    _drm: AvFrame,
}

impl VideoFrame {
    pub fn new(
        seq: u64,
        width: u32,
        height: u32,
        planes: Vec<DmaPlane>,
        color: ColorInfo,
        va: AvFrame,
        drm: AvFrame,
    ) -> Self {
        Self {
            seq,
            width,
            height,
            planes,
            color,
            _va: va,
            _drm: drm,
        }
    }
}

unsafe impl Send for VideoFrame {}
