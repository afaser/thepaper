use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub video_path: PathBuf,
    #[serde(default = "default_blur_radius")]
    pub blur_radius: u32,
    #[serde(default = "default_app_id_prefix")]
    pub app_id_prefix: String,
    #[serde(default)]
    pub transcode_codec: Option<String>,
}

fn default_blur_radius() -> u32 {
    12
}

fn default_app_id_prefix() -> String {
    "thepaper".to_owned()
}

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("cannot read config {}", path.display()))?;
        let config: Self =
            toml::from_str(&text).with_context(|| format!("invalid config {}", path.display()))?;
        if config.video_path.as_os_str().is_empty() {
            anyhow::bail!("config {}: `video_path` must not be empty", path.display());
        }
        if config.blur_radius == 0 || config.blur_radius > 64 {
            anyhow::bail!(
                "config {}: `blur_radius` must be in 1..=64, got {}",
                path.display(),
                config.blur_radius
            );
        }
        if config.app_id_prefix.is_empty() {
            anyhow::bail!(
                "config {}: `app_id_prefix` must not be empty",
                path.display()
            );
        }
        if !config.video_path.is_file() {
            anyhow::bail!(
                "config {}: video {} does not exist or is not a regular file",
                path.display(),
                config.video_path.display()
            );
        }
        Ok(config)
    }

    pub fn live_namespace(&self, output: &str) -> String {
        format!("{}-live-{}", self.app_id_prefix, output)
    }

    pub fn blur_namespace(&self, output: &str) -> String {
        format!("{}-blur-{}", self.app_id_prefix, output)
    }
}
