use std::path::{Path, PathBuf};

use rmcp::ErrorData as McpError;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::support::resolve_output_dir;

pub(crate) const DEFAULT_SUMMER_FRONTEND_ROOT_DIR: &str = "crates/app/frontend-routes";
const DEFAULT_SUMMER_FRONTEND_VIEW_DIR: &str = "crates/app/frontend-routes/views/system";

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum FrontendTargetPreset {
    #[default]
    SummerMcp,
    ArtDesignPro,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FrontendResolvedLayout {
    pub(crate) frontend_root_dir: PathBuf,
    pub(crate) api_dir: PathBuf,
    pub(crate) api_type_dir: PathBuf,
    pub(crate) view_system_dir: PathBuf,
}

impl FrontendTargetPreset {
    pub(crate) fn resolve_bundle_layout(
        self,
        workspace_root: &Path,
        output_dir: Option<&str>,
    ) -> Result<FrontendResolvedLayout, McpError> {
        Ok(match self {
            Self::SummerMcp => {
                let frontend_root_dir = resolve_output_dir(
                    workspace_root,
                    output_dir,
                    DEFAULT_SUMMER_FRONTEND_ROOT_DIR,
                );
                FrontendResolvedLayout {
                    api_dir: frontend_root_dir.join("api"),
                    api_type_dir: frontend_root_dir.join("api_type"),
                    view_system_dir: frontend_root_dir.join("views").join("system"),
                    frontend_root_dir,
                }
            }
            Self::ArtDesignPro => {
                let frontend_root_dir =
                    resolve_art_design_pro_root(workspace_root, output_dir, self)?;
                FrontendResolvedLayout {
                    api_dir: frontend_root_dir.join("src").join("api"),
                    api_type_dir: frontend_root_dir.join("src").join("types").join("api"),
                    view_system_dir: frontend_root_dir.join("src").join("views").join("system"),
                    frontend_root_dir,
                }
            }
        })
    }

    pub(crate) fn resolve_page_output_dir(
        self,
        workspace_root: &Path,
        output_dir: Option<&str>,
    ) -> Result<PathBuf, McpError> {
        Ok(match self {
            Self::SummerMcp => {
                resolve_output_dir(workspace_root, output_dir, DEFAULT_SUMMER_FRONTEND_VIEW_DIR)
            }
            Self::ArtDesignPro => resolve_art_design_pro_root(workspace_root, output_dir, self)?
                .join("src")
                .join("views")
                .join("system"),
        })
    }
}

fn resolve_art_design_pro_root(
    workspace_root: &Path,
    output_dir: Option<&str>,
    preset: FrontendTargetPreset,
) -> Result<PathBuf, McpError> {
    let Some(output_dir) = output_dir else {
        return Err(McpError::invalid_params(
            format!(
                "target_preset `{}` requires output_dir pointing at the frontend project root",
                preset.as_str()
            ),
            None,
        ));
    };
    Ok(resolve_output_dir(workspace_root, Some(output_dir), ""))
}

impl FrontendTargetPreset {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::SummerMcp => "summer_mcp",
            Self::ArtDesignPro => "art_design_pro",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summer_layout_defaults_to_workspace_frontend_routes() {
        let workspace_root = Path::new("/workspace");
        let layout = FrontendTargetPreset::SummerMcp
            .resolve_bundle_layout(workspace_root, None)
            .unwrap();

        assert_eq!(
            layout.frontend_root_dir,
            PathBuf::from("/workspace/crates/app/frontend-routes")
        );
        assert_eq!(
            layout.api_dir,
            PathBuf::from("/workspace/crates/app/frontend-routes/api")
        );
        assert_eq!(
            layout.api_type_dir,
            PathBuf::from("/workspace/crates/app/frontend-routes/api_type")
        );
        assert_eq!(
            layout.view_system_dir,
            PathBuf::from("/workspace/crates/app/frontend-routes/views/system")
        );
    }

    #[test]
    fn art_design_pro_layout_targets_src_tree() {
        let workspace_root = Path::new("/workspace");
        let layout = FrontendTargetPreset::ArtDesignPro
            .resolve_bundle_layout(workspace_root, Some("/tmp/art-design-pro"))
            .unwrap();

        assert_eq!(
            layout.frontend_root_dir,
            PathBuf::from("/tmp/art-design-pro")
        );
        assert_eq!(layout.api_dir, PathBuf::from("/tmp/art-design-pro/src/api"));
        assert_eq!(
            layout.api_type_dir,
            PathBuf::from("/tmp/art-design-pro/src/types/api")
        );
        assert_eq!(
            layout.view_system_dir,
            PathBuf::from("/tmp/art-design-pro/src/views/system")
        );
    }

    #[test]
    fn art_design_pro_requires_project_root() {
        let error = FrontendTargetPreset::ArtDesignPro
            .resolve_bundle_layout(Path::new("/workspace"), None)
            .unwrap_err();

        assert!(
            error
                .message
                .contains("target_preset `art_design_pro` requires output_dir")
        );
    }
}
