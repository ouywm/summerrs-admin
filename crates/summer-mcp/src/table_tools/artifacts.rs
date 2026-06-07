use std::path::{Path, PathBuf};

use rmcp::ErrorData as McpError;
use summer_domain::{
    dict::{DictBundleSpec, DictBundleSyncResult},
    menu::{MenuConfigSpec, MenuConfigSyncResult},
};

use crate::{
    error_model::invalid_params_error,
    output_contract::{
        ArtifactBundleSummary, ArtifactMode, build_artifact_bundle, generator_artifact_mode,
    },
    table_tools::tool_results::ExportArtifactsResult,
    tools::support::{
        resolve_output_dir, sanitize_file_stem, workspace_root, write_pretty_json_file,
    },
};

pub(crate) fn build_entity_generator_artifacts(
    output_dir: Option<&str>,
    entity_file: &Path,
    mod_file: &Path,
) -> ArtifactBundleSummary {
    let output_root = entity_file.parent().unwrap_or_else(|| Path::new("."));
    build_artifact_bundle(
        generator_artifact_mode(output_dir),
        output_root,
        [("entity_file", entity_file), ("mod_file", mod_file)],
    )
}

pub(crate) fn build_admin_generator_artifacts(
    workspace_root: &Path,
    output_dir: Option<&str>,
    router_file: &Path,
    service_file: &Path,
    dto_file: &Path,
    vo_file: &Path,
    mod_files: &[PathBuf],
) -> ArtifactBundleSummary {
    let output_root = if let Some(output_dir) = output_dir {
        resolve_output_dir(workspace_root, Some(output_dir), "")
    } else {
        workspace_root.to_path_buf()
    };
    let mut files = vec![
        ("router_file", router_file),
        ("service_file", service_file),
        ("dto_file", dto_file),
        ("vo_file", vo_file),
    ];
    for mod_file in mod_files {
        files.push(("mod_file", mod_file.as_path()));
    }
    build_artifact_bundle(generator_artifact_mode(output_dir), &output_root, files)
}

pub(crate) fn build_frontend_api_artifacts(
    output_dir: Option<&str>,
    frontend_root_dir: &Path,
    api_file: &Path,
    api_type_file: &Path,
) -> ArtifactBundleSummary {
    build_artifact_bundle(
        generator_artifact_mode(output_dir),
        frontend_root_dir,
        [("api_file", api_file), ("api_type_file", api_type_file)],
    )
}

pub(crate) fn build_frontend_page_artifacts(
    output_dir: Option<&str>,
    page_dir: &Path,
    types_file: &Path,
    index_file: &Path,
    search_file: &Path,
    form_panel_file: &Path,
) -> ArtifactBundleSummary {
    build_artifact_bundle(
        generator_artifact_mode(output_dir),
        page_dir,
        [
            ("types_file", types_file),
            ("index_file", index_file),
            ("search_file", search_file),
            ("form_panel_file", form_panel_file),
        ],
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn build_frontend_bundle_artifacts(
    output_dir: Option<&str>,
    frontend_root_dir: &Path,
    api_file: &Path,
    api_type_file: &Path,
    types_file: &Path,
    index_file: &Path,
    search_file: &Path,
    form_panel_file: &Path,
) -> ArtifactBundleSummary {
    build_artifact_bundle(
        generator_artifact_mode(output_dir),
        frontend_root_dir,
        [
            ("api_file", api_file),
            ("api_type_file", api_type_file),
            ("types_file", types_file),
            ("index_file", index_file),
            ("search_file", search_file),
            ("form_panel_file", form_panel_file),
        ],
    )
}

pub(crate) async fn export_menu_config_artifacts(
    config: &MenuConfigSpec,
    sync: &MenuConfigSyncResult,
    output_dir: &str,
) -> Result<ExportArtifactsResult, McpError> {
    let output_root = resolve_export_output_dir(output_dir)?;
    let menu_dir = output_root.join("menu");
    let file_stem = menu_export_file_stem(config);
    let spec_file = menu_dir.join(format!("{file_stem}.json"));
    let plan_file = menu_dir.join(format!("{file_stem}.plan.json"));

    write_pretty_json_file(&spec_file, config, "menu export spec").await?;
    write_pretty_json_file(&plan_file, sync, "menu export plan").await?;

    Ok(ExportArtifactsResult {
        output_dir: output_root.display().to_string(),
        spec_file: spec_file.display().to_string(),
        plan_file: plan_file.display().to_string(),
        artifacts: build_export_artifacts(&output_root, &spec_file, &plan_file),
    })
}

pub(crate) async fn export_dict_bundle_artifacts(
    bundle: &DictBundleSpec,
    sync: &DictBundleSyncResult,
    output_dir: &str,
) -> Result<ExportArtifactsResult, McpError> {
    let output_root = resolve_export_output_dir(output_dir)?;
    let dict_dir = output_root.join("dict");
    let file_stem = format!("dict-{}", sanitize_file_stem(&bundle.dict_type));
    let spec_file = dict_dir.join(format!("{file_stem}.json"));
    let plan_file = dict_dir.join(format!("{file_stem}.plan.json"));

    write_pretty_json_file(&spec_file, bundle, "dict export spec").await?;
    write_pretty_json_file(&plan_file, sync, "dict export plan").await?;

    Ok(ExportArtifactsResult {
        output_dir: output_root.display().to_string(),
        spec_file: spec_file.display().to_string(),
        plan_file: plan_file.display().to_string(),
        artifacts: build_export_artifacts(&output_root, &spec_file, &plan_file),
    })
}

fn build_export_artifacts(
    output_root: &Path,
    spec_file: &Path,
    plan_file: &Path,
) -> ArtifactBundleSummary {
    build_artifact_bundle(
        ArtifactMode::Export,
        output_root,
        [("spec_file", spec_file), ("plan_file", plan_file)],
    )
}

fn resolve_export_output_dir(output_dir: &str) -> Result<PathBuf, McpError> {
    if output_dir.trim().is_empty() {
        return Err(invalid_params_error(
            "invalid_output_dir",
            "Invalid output directory",
            Some(
                "Pass a non-empty output_dir. Use an absolute temp path when you want a safe preview.",
            ),
            Some("output_dir cannot be empty".to_string()),
            None,
        ));
    }
    let workspace_root = workspace_root()?;
    Ok(resolve_output_dir(&workspace_root, Some(output_dir), ""))
}

fn menu_export_file_stem(config: &MenuConfigSpec) -> String {
    if let [menu] = config.menus.as_slice() {
        let seed = if menu.path.trim().is_empty() {
            menu.name.as_str()
        } else {
            menu.path.as_str()
        };
        return format!("menu-{}", sanitize_file_stem(seed));
    }
    "menu-config".to_string()
}
