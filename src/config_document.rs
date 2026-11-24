use anyhow::{anyhow, bail, Result};
use kdl::{KdlDocument, KdlError, KdlNode, KdlValue};

const OPTIONS: [&str; 4] = [
    "video_path",
    "blur_radius",
    "app_id_prefix",
    "transcode_codec",
];

#[derive(Debug, Clone)]
pub(crate) struct ConfigDocument {
    document: KdlDocument,
}

impl ConfigDocument {
    pub(crate) fn new(text: &str) -> Result<Self> {
        let document = parse_document(text)?;
        validate_nodes(&document)?;
        Ok(Self { document })
    }

    pub(crate) fn value(&self, name: &str) -> Option<&KdlValue> {
        self.document.get_arg(name)
    }
}

fn parse_document(text: &str) -> Result<KdlDocument> {
    text.parse::<KdlDocument>().map_err(parse_error)
}

fn parse_error(error: KdlError) -> anyhow::Error {
    let detail = error
        .diagnostics
        .first()
        .map(ToString::to_string)
        .unwrap_or_else(|| error.to_string());
    anyhow!("invalid KDL: {detail}")
}

fn validate_nodes(document: &KdlDocument) -> Result<()> {
    for node in document.nodes() {
        validate_node(document, node)?;
    }
    Ok(())
}

fn validate_node(document: &KdlDocument, node: &KdlNode) -> Result<()> {
    let name = node.name().value();
    if !is_known_option(name) {
        bail!("unknown option `{name}`");
    }
    if is_duplicated(document, name) {
        bail!("duplicate option `{name}`");
    }
    if has_properties(node) {
        bail!("option `{name}` must not have properties");
    }
    if argument_count(node) != 1 {
        bail!("option `{name}` must have exactly one argument");
    }
    if node.children().is_some() {
        bail!("option `{name}` must not have a child block");
    }
    Ok(())
}

fn is_known_option(name: &str) -> bool {
    OPTIONS.contains(&name)
}

fn is_duplicated(document: &KdlDocument, name: &str) -> bool {
    document
        .nodes()
        .iter()
        .filter(|node| node.name().value() == name)
        .count()
        > 1
}

fn has_properties(node: &KdlNode) -> bool {
    node.entries().iter().any(|entry| entry.name().is_some())
}

fn argument_count(node: &KdlNode) -> usize {
    node.entries()
        .iter()
        .filter(|entry| entry.name().is_none())
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> ConfigDocument {
        ConfigDocument::new(text).unwrap()
    }

    fn rejection(text: &str) -> String {
        ConfigDocument::new(text).unwrap_err().to_string()
    }

    #[test]
    fn full_document_yields_values() {
        let document = parse(
            "video_path \"/videos/wallpaper.mp4\"\n\
             blur_radius 24\n\
             app_id_prefix custom\n\
             transcode_codec \"hevc\"\n",
        );

        assert_eq!(
            document.value("video_path").and_then(KdlValue::as_string),
            Some("/videos/wallpaper.mp4")
        );
        assert_eq!(
            document.value("blur_radius").and_then(KdlValue::as_integer),
            Some(24)
        );
        assert_eq!(
            document
                .value("app_id_prefix")
                .and_then(KdlValue::as_string),
            Some("custom")
        );
        assert_eq!(
            document
                .value("transcode_codec")
                .and_then(KdlValue::as_string),
            Some("hevc")
        );
    }

    #[test]
    fn comments_are_accepted() {
        let document = parse(
            "// video_path is required\n\
             video_path \"/videos/wallpaper.mp4\"\n\
             /* defaults apply */\n",
        );

        assert_eq!(
            document.value("video_path").and_then(KdlValue::as_string),
            Some("/videos/wallpaper.mp4")
        );
    }

    #[test]
    fn unknown_option_is_rejected() {
        let error = rejection("video_path \"/videos/wallpaper.mp4\"\nblur_radius2 12\n");

        assert!(error.contains("unknown option `blur_radius2`"));
    }

    #[test]
    fn duplicate_option_is_rejected() {
        let error = rejection("blur_radius 12\nblur_radius 24\n");

        assert!(error.contains("duplicate option `blur_radius`"));
    }

    #[test]
    fn missing_argument_is_rejected() {
        let error = rejection("blur_radius\n");

        assert!(error.contains("`blur_radius` must have exactly one argument"));
    }

    #[test]
    fn two_arguments_are_rejected() {
        let error = rejection("blur_radius 12 24\n");

        assert!(error.contains("`blur_radius` must have exactly one argument"));
    }

    #[test]
    fn property_is_rejected() {
        let error = rejection("blur_radius value=12\n");

        assert!(error.contains("`blur_radius` must not have properties"));
    }

    #[test]
    fn child_block_is_rejected() {
        let error = rejection("blur_radius 12 {\n  nested 1\n}\n");

        assert!(error.contains("`blur_radius` must not have a child block"));
    }
}
