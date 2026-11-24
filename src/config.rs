use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use crate::config_document::ConfigDocument;

const DEFAULT_BLUR_RADIUS: u32 = 12;
const MIN_BLUR_RADIUS: u32 = 1;
const MAX_BLUR_RADIUS: u32 = 64;
const DEFAULT_APP_ID_PREFIX: &str = "thepaper";

#[derive(Debug, Clone)]
pub struct Config {
    video_path: PathBuf,
    blur_radius: u32,
    app_id_prefix: String,
    transcode_codec: Option<String>,
}

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("cannot read config {}", path.display()))?;
        let document = ConfigDocument::new(&text)
            .with_context(|| format!("invalid config {}", path.display()))?;
        Self::from_document(&document).with_context(|| format!("config {}", path.display()))
    }

    fn from_document(document: &ConfigDocument) -> Result<Self> {
        let video_path = PathBuf::from(required_string(document, "video_path")?);
        ensure_video_file(&video_path)?;
        let blur_radius = blur_radius(document)?;
        let app_id_prefix = app_id_prefix(document)?;
        let transcode_codec = optional_string(document, "transcode_codec")?;
        Ok(Self {
            video_path,
            blur_radius,
            app_id_prefix,
            transcode_codec,
        })
    }

    pub fn video_path(&self) -> &Path {
        &self.video_path
    }

    pub fn blur_radius(&self) -> u32 {
        self.blur_radius
    }

    pub fn app_id_prefix(&self) -> &str {
        &self.app_id_prefix
    }

    pub fn transcode_codec(&self) -> Option<&str> {
        self.transcode_codec.as_deref()
    }

    pub fn background_namespace(&self, output: &str) -> String {
        format!("{}-{}", self.app_id_prefix, output)
    }

    pub fn overview_namespace(&self, output: &str) -> String {
        format!("{}-overview-{}", self.app_id_prefix, output)
    }
}

fn required_string(document: &ConfigDocument, name: &str) -> Result<String> {
    let Some(value) = optional_string(document, name)? else {
        bail!("missing option `{name}`");
    };
    if value.is_empty() {
        bail!("`{name}` must not be empty");
    }
    Ok(value)
}

fn optional_string(document: &ConfigDocument, name: &str) -> Result<Option<String>> {
    let Some(value) = document.value(name) else {
        return Ok(None);
    };
    let Some(value) = value.as_string() else {
        bail!("`{name}` must be a string");
    };
    Ok(Some(value.to_owned()))
}

fn blur_radius(document: &ConfigDocument) -> Result<u32> {
    let Some(value) = document.value("blur_radius") else {
        return Ok(DEFAULT_BLUR_RADIUS);
    };
    let Some(value) = value.as_integer() else {
        bail!("`blur_radius` must be an integer");
    };
    let Some(blur_radius) = u32::try_from(value)
        .ok()
        .filter(|radius| is_blur_radius(*radius))
    else {
        bail!("`blur_radius` must be in {MIN_BLUR_RADIUS}..={MAX_BLUR_RADIUS}");
    };
    Ok(blur_radius)
}

fn is_blur_radius(value: u32) -> bool {
    (MIN_BLUR_RADIUS..=MAX_BLUR_RADIUS).contains(&value)
}

fn app_id_prefix(document: &ConfigDocument) -> Result<String> {
    let Some(prefix) = optional_string(document, "app_id_prefix")? else {
        return Ok(DEFAULT_APP_ID_PREFIX.to_owned());
    };
    if prefix.is_empty() {
        bail!("`app_id_prefix` must not be empty");
    }
    Ok(prefix)
}

fn ensure_video_file(path: &Path) -> Result<()> {
    if !path.is_file() {
        bail!(
            "video {} does not exist or is not a regular file",
            path.display()
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn video_file(name: &str) -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("thepaper-config-{}-{name}.mp4", std::process::id()));
        std::fs::write(&path, b"").unwrap();
        path
    }

    fn config_from(text: &str) -> Result<Config> {
        let document = ConfigDocument::new(text)?;
        Config::from_document(&document)
    }

    fn error_from(text: &str) -> String {
        format!("{:#}", config_from(text).unwrap_err())
    }

    #[test]
    fn full_document_yields_values() {
        let video = video_file("full");
        let text = format!(
            "video_path \"{}\"\n\
             blur_radius 24\n\
             app_id_prefix \"custom\"\n\
             transcode_codec \"hevc\"\n",
            video.display()
        );

        let config = config_from(&text).unwrap();

        assert_eq!(config.video_path(), video);
        assert_eq!(config.blur_radius(), 24);
        assert_eq!(config.app_id_prefix(), "custom");
        assert_eq!(config.transcode_codec(), Some("hevc"));
        std::fs::remove_file(video).unwrap();
    }

    #[test]
    fn video_path_alone_yields_defaults() {
        let video = video_file("defaults");
        let text = format!("video_path \"{}\"\n", video.display());

        let config = config_from(&text).unwrap();

        assert_eq!(config.blur_radius(), DEFAULT_BLUR_RADIUS);
        assert_eq!(config.app_id_prefix(), DEFAULT_APP_ID_PREFIX);
        assert_eq!(config.transcode_codec(), None);
        std::fs::remove_file(video).unwrap();
    }

    #[test]
    fn blur_radius_accepts_bounds_and_rejects_outside_range() {
        let video = video_file("blur-bounds");
        for radius in [MIN_BLUR_RADIUS, MAX_BLUR_RADIUS] {
            let text = format!("video_path \"{}\"\nblur_radius {radius}\n", video.display());
            assert_eq!(config_from(&text).unwrap().blur_radius(), radius);
        }
        for radius in [-1, 0, i64::from(MAX_BLUR_RADIUS) + 1] {
            let text = format!("video_path \"{}\"\nblur_radius {radius}\n", video.display());
            let error = error_from(&text);
            assert!(error.contains("`blur_radius` must be in 1..=64"));
        }
        std::fs::remove_file(video).unwrap();
    }

    #[test]
    fn empty_document_is_rejected() {
        let error = error_from("");

        assert!(error.contains("missing option `video_path`"));
    }

    #[test]
    fn missing_video_path_is_rejected() {
        let error = error_from("blur_radius 12\n");

        assert!(error.contains("missing option `video_path`"));
    }

    #[test]
    fn empty_video_path_is_rejected() {
        let error = error_from("video_path \"\"\n");

        assert!(error.contains("`video_path` must not be empty"));
    }

    #[test]
    fn nonexistent_video_is_rejected() {
        let video = std::env::temp_dir().join(format!(
            "thepaper-config-{}-nonexistent.mp4",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&video);
        let text = format!("video_path \"{}\"\n", video.display());

        let error = error_from(&text);

        assert!(error.contains(&video.display().to_string()));
    }

    #[test]
    fn wrong_argument_types_are_rejected() {
        let video = video_file("wrong-types");
        for argument in ["\"12\"", "12.5"] {
            let text = format!(
                "video_path \"{}\"\nblur_radius {argument}\n",
                video.display()
            );
            let error = error_from(&text);
            assert!(error.contains("`blur_radius` must be an integer"));
        }
        let error = error_from("video_path 12\n");
        assert!(error.contains("`video_path` must be a string"));
        std::fs::remove_file(video).unwrap();
    }

    #[test]
    fn empty_app_id_prefix_is_rejected() {
        let video = video_file("empty-prefix");
        let text = format!("video_path \"{}\"\napp_id_prefix \"\"\n", video.display());

        let error = error_from(&text);

        assert!(error.contains("`app_id_prefix` must not be empty"));
        std::fs::remove_file(video).unwrap();
    }

    #[test]
    fn load_reports_config_path() {
        let dir = std::env::temp_dir().join(format!("thepaper-config-{}-load", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.kdl");
        std::fs::write(&path, "blur_radius 12\n").unwrap();

        let error = Config::load(&path).unwrap_err();

        let message = format!("{error:#}");
        assert!(message.contains(&path.display().to_string()));
        assert!(message.contains("missing option `video_path`"));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
