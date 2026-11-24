use std::collections::HashSet;
use std::process::Command;

use anyhow::{anyhow, bail, Context, Result};

#[derive(Default)]
pub struct VaInfo {
    entries: HashSet<(String, String)>,
}

impl VaInfo {
    pub fn query(node: &str) -> Result<Self> {
        let out = Command::new("vainfo")
            .args(["--display", "drm", "--device", node])
            .output()
            .context("cannot run vainfo (libva-utils is required for transcode planning)")?;
        if !out.status.success() {
            bail!(
                "vainfo failed on {node}: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
        let mut entries = HashSet::new();
        for line in String::from_utf8_lossy(&out.stdout).lines() {
            let Some((profile, entry)) = line.split_once(':') else {
                continue;
            };
            let profile = profile
                .trim()
                .strip_prefix("VAProfile")
                .unwrap_or(profile.trim())
                .to_owned();
            entries.insert((profile, entry.trim().to_owned()));
        }
        Ok(Self { entries })
    }

    fn has(&self, prefix: &str, entrypoint: &str) -> bool {
        self.entries
            .iter()
            .any(|(profile, entry)| profile.starts_with(prefix) && entry == entrypoint)
    }

    fn decode_profiles(&self) -> Vec<String> {
        let mut profiles: Vec<_> = self
            .entries
            .iter()
            .filter(|(_, entry)| entry == "VAEntrypointVLD")
            .map(|(profile, _)| profile.clone())
            .collect();
        profiles.sort();
        profiles
    }

    fn encode_profiles(&self) -> Vec<String> {
        let mut profiles: Vec<_> = self
            .entries
            .iter()
            .filter(|(_, entry)| entry.starts_with("VAEntrypointEnc"))
            .map(|(profile, _)| profile.clone())
            .collect();
        profiles.sort();
        profiles
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Codec {
    Av1,
    Hevc,
    Vp9,
    H264,
}

impl Codec {
    pub fn name(self) -> &'static str {
        match self {
            Codec::Av1 => "av1",
            Codec::Hevc => "hevc",
            Codec::Vp9 => "vp9",
            Codec::H264 => "h264",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "av1" => Some(Codec::Av1),
            "hevc" | "h265" => Some(Codec::Hevc),
            "vp9" => Some(Codec::Vp9),
            "h264" | "avc" => Some(Codec::H264),
            _ => None,
        }
    }

    fn decode_profile_prefix(self) -> &'static str {
        match self {
            Codec::Av1 => "AV1Profile",
            Codec::Hevc => "HEVCMain",
            Codec::Vp9 => "VP9Profile",
            Codec::H264 => "H264",
        }
    }

    fn vaapi_encoder(self) -> &'static str {
        match self {
            Codec::Av1 => "av1_vaapi",
            Codec::Hevc => "hevc_vaapi",
            Codec::Vp9 => "vp9_vaapi",
            Codec::H264 => "h264_vaapi",
        }
    }

    fn cpu_encoder(self) -> &'static str {
        match self {
            Codec::Av1 => "libsvtav1",
            Codec::Hevc => "libx265",
            Codec::Vp9 => "libvpx-vp9",
            Codec::H264 => "libx264",
        }
    }
}

fn run_ffmpeg(args: &[&str]) -> Result<std::process::Output> {
    let out = Command::new("ffmpeg")
        .args(args)
        .output()
        .context("cannot run ffmpeg")?;
    Ok(out)
}

pub fn available_encoders() -> HashSet<String> {
    let mut set = HashSet::new();
    if let Ok(out) = run_ffmpeg(&["-hide_banner", "-encoders"]) {
        for line in String::from_utf8_lossy(&out.stdout).lines() {
            let mut parts = line.split_whitespace();
            let _flags = parts.next();
            if let Some(name) = parts.next() {
                set.insert(name.to_owned());
            }
        }
    }
    set
}

pub fn choose_target(
    va: &VaInfo,
    encoders: &HashSet<String>,
    forced: Option<&str>,
) -> Result<(Codec, String)> {
    let candidates: Vec<Codec> = if let Some(forced) = forced {
        let codec =
            Codec::parse(forced).ok_or_else(|| anyhow!("unknown transcode_codec `{forced}`"))?;
        vec![codec]
    } else {
        vec![Codec::Av1, Codec::Hevc, Codec::Vp9, Codec::H264]
    };

    for &codec in &candidates {
        if !va.has(codec.decode_profile_prefix(), "VAEntrypointVLD") {
            continue;
        }
        let vaapi = codec.vaapi_encoder();
        if (va.has(codec.decode_profile_prefix(), "VAEntrypointEncSlice")
            || va.has(codec.decode_profile_prefix(), "VAEntrypointEncSliceLP"))
            && encoders.contains(vaapi)
        {
            return Ok((codec, vaapi.to_owned()));
        }
        let cpu = codec.cpu_encoder();
        if encoders.contains(cpu) {
            return Ok((codec, cpu.to_owned()));
        }
    }
    bail!(
        "no codec is both hardware-decodable and encodable on this VA-API device \
         (decode profiles: {:?}, encode profiles: {:?})",
        va.decode_profiles(),
        va.encode_profiles()
    )
}
