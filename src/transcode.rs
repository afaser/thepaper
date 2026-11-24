use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};

use crate::codec::{available_encoders, choose_target, VaInfo};
use crate::paths;

#[derive(Debug, Clone)]
pub struct MediaPlan {
    pub path: PathBuf,
    pub transcoded: bool,
}

fn source_codec(path: &Path) -> Result<String> {
    let out = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=codec_name",
            "-of",
            "default=nw=1:nk=1",
        ])
        .arg(path)
        .output()
        .context("cannot run ffprobe")?;
    if !out.status.success() {
        bail!(
            "ffprobe failed for {}: {}",
            path.display(),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    let codec = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    if codec.is_empty() {
        bail!("ffprobe found no video stream in {}", path.display());
    }
    Ok(codec)
}

fn source_decodes_hardware(path: &Path, node: &str) -> bool {
    let out = Command::new("ffmpeg")
        .args([
            "-nostdin",
            "-v",
            "error",
            "-vaapi_device",
            node,
            "-hwaccel",
            "vaapi",
            "-hwaccel_output_format",
            "vaapi",
            "-i",
        ])
        .arg(path)
        .args(["-frames:v", "1", "-f", "null", "-"])
        .output();
    matches!(out, Ok(o) if o.status.success())
}

fn cache_key(path: &Path, codec: &str, encoder: &str) -> Result<PathBuf> {
    let meta =
        std::fs::metadata(path).with_context(|| format!("cannot stat {}", path.display()))?;
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let mut hasher = Sha256::new();
    hasher.update(path.to_string_lossy().as_bytes());
    hasher.update(b"\0");
    hasher.update(meta.len().to_le_bytes());
    hasher.update(mtime.to_le_bytes());
    hasher.update(b"\0");
    hasher.update(codec.as_bytes());
    hasher.update(b"\0");
    hasher.update(encoder.as_bytes());
    hasher.update(b"\0v1");
    let digest = hasher.finalize();
    let mut name = String::with_capacity(32);
    for b in &digest[..16] {
        use std::fmt::Write as _;
        let _ = write!(name, "{b:02x}");
    }
    Ok(paths::cache_dir().join(format!("{name}-{codec}.mkv")))
}

fn transcode(src: &Path, dst: &Path, node: &str, encoder: &str) -> Result<()> {
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("cannot create cache dir {}", parent.display()))?;
    }
    let _ = std::fs::remove_file(dst);
    let mut args: Vec<String> = vec![
        "-nostdin".into(),
        "-y".into(),
        "-hide_banner".into(),
        "-loglevel".into(),
        "warning".into(),
    ];
    if encoder.ends_with("_vaapi") {
        args.extend(["-vaapi_device".into(), node.to_owned()]);
    }
    args.push("-i".into());
    args.push(src.to_string_lossy().into_owned());
    args.extend([
        "-map".into(),
        "0:v:0".into(),
        "-an".into(),
        "-map_metadata".into(),
        "-1".into(),
    ]);
    if encoder.ends_with("_vaapi") {
        args.extend([
            "-vf".into(),
            "format=nv12,hwupload".into(),
            "-c:v".into(),
            encoder.to_owned(),
            "-qp".into(),
            "23".into(),
        ]);
    } else {
        let codec_args: &[&str] = match encoder {
            "libx265" => &["-preset", "medium", "-crf", "23"],
            "libx264" => &["-preset", "medium", "-crf", "20"],
            "libvpx-vp9" => &["-crf", "30", "-b:v", "0"],
            "libsvtav1" => &["-preset", "6", "-crf", "30"],
            _ => &[],
        };
        args.push("-c:v".into());
        args.push(encoder.to_owned());
        for arg in codec_args {
            args.push((*arg).to_owned());
        }
        args.push("-pix_fmt".into());
        args.push("yuv420p".into());
    }
    args.extend(["-f".into(), "matroska".into()]);
    args.push(dst.to_string_lossy().into_owned());

    tracing::info!(
        encoder,
        source = %src.display(),
        cache = %dst.display(),
        "transcoding source once into a hardware-decodable file"
    );
    let out = Command::new("ffmpeg")
        .args(&args)
        .output()
        .context("cannot run ffmpeg transcode")?;
    if !out.status.success() {
        let _ = std::fs::remove_file(dst);
        let stderr = String::from_utf8_lossy(&out.stderr);
        let tail: String = stderr
            .lines()
            .rev()
            .take(12)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join("\n");
        bail!("transcode with {encoder} failed:\n{tail}");
    }
    Ok(())
}

fn verify_hardware(path: &Path) -> Result<()> {
    let mut dec = crate::decoder::Decoder::open(path.to_path_buf())?;
    dec.next_frame()
        .context("transcoded file is not hardware-decodable")?;
    Ok(())
}

pub fn prepare(path: &Path, forced: Option<&str>) -> Result<MediaPlan> {
    let node = crate::decoder::render_node();
    let codec = source_codec(path)?;
    if source_decodes_hardware(path, &node) {
        tracing::info!(
            source_codec = %codec,
            path = %path.display(),
            "source codec has hardware VA-API decode; playing as-is"
        );
        return Ok(MediaPlan {
            path: path.to_path_buf(),
            transcoded: false,
        });
    }

    tracing::warn!(
        source_codec = %codec,
        "source codec has no hardware decode on this VA-API device; preparing a transcoded copy"
    );
    let va = VaInfo::query(&node)?;
    let encoders = available_encoders();
    let (target, encoder) = choose_target(&va, &encoders, forced)?;
    let cache = cache_key(path, target.name(), &encoder)?;

    if cache.is_file() && verify_hardware(&cache).is_ok() {
        tracing::info!(
            target_codec = target.name(),
            encoder = %encoder,
            cache = %cache.display(),
            "reusing cached transcoded file"
        );
        return Ok(MediaPlan {
            path: cache,
            transcoded: true,
        });
    }

    transcode(path, &cache, &node, &encoder)?;
    verify_hardware(&cache)?;
    tracing::info!(
        target_codec = target.name(),
        encoder = %encoder,
        cache = %cache.display(),
        "transcoded copy is hardware-decodable"
    );
    Ok(MediaPlan {
        path: cache,
        transcoded: true,
    })
}
