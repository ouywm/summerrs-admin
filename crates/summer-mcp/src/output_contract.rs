use std::path::Path;

use rmcp::schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactMode {
    InPlace,
    ExplicitOutput,
    Export,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct ArtifactFileSummary {
    pub kind: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct ArtifactBundleSummary {
    pub mode: ArtifactMode,
    pub output_root: String,
    pub files: Vec<ArtifactFileSummary>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolExecutionMode {
    Read,
    Plan,
    Export,
    Apply,
}

pub fn build_artifact_bundle<'a, I>(
    mode: ArtifactMode,
    output_root: &Path,
    files: I,
) -> ArtifactBundleSummary
where
    I: IntoIterator<Item = (&'a str, &'a Path)>,
{
    ArtifactBundleSummary {
        mode,
        output_root: output_root.display().to_string(),
        files: files
            .into_iter()
            .map(|(kind, path)| ArtifactFileSummary {
                kind: kind.to_string(),
                path: path.display().to_string(),
            })
            .collect(),
    }
}

pub fn generator_artifact_mode(explicit_output_dir: Option<&str>) -> ArtifactMode {
    if explicit_output_dir.is_some() {
        ArtifactMode::ExplicitOutput
    } else {
        ArtifactMode::InPlace
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn generator_artifact_mode_marks_explicit_output_dirs() {
        assert_eq!(generator_artifact_mode(None), ArtifactMode::InPlace);
        assert_eq!(
            generator_artifact_mode(Some("/tmp/summer-mcp-preview")),
            ArtifactMode::ExplicitOutput
        );
    }

    #[test]
    fn build_artifact_bundle_keeps_kinds_and_paths() {
        let root = Path::new("/tmp/summer-mcp-output");
        let entity = root.join("entity/sys_role.rs");
        let mod_file = root.join("entity/mod.rs");

        let bundle = build_artifact_bundle(
            ArtifactMode::Export,
            root,
            [
                ("entity_file", entity.as_path()),
                ("mod_file", mod_file.as_path()),
            ],
        );

        assert_eq!(bundle.mode, ArtifactMode::Export);
        assert_eq!(bundle.output_root, "/tmp/summer-mcp-output");
        assert_eq!(bundle.files.len(), 2);
        assert_eq!(bundle.files[0].kind, "entity_file");
        assert_eq!(bundle.files[1].path, "/tmp/summer-mcp-output/entity/mod.rs");
    }
}
