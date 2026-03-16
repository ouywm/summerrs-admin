use std::path::{Path, PathBuf};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio::process::Command;

use crate::tools::{frontend_target::FrontendTargetPreset, support::parse_mod_line};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ValidationStatus {
    Passed,
    Failed,
    Skipped,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct ValidationCheckResult {
    pub name: String,
    pub status: ValidationStatus,
    pub command: Option<String>,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct GenerationValidationSummary {
    pub status: ValidationStatus,
    pub checks: Vec<ValidationCheckResult>,
}

impl GenerationValidationSummary {
    pub fn from_checks(checks: Vec<ValidationCheckResult>) -> Self {
        let status = if checks
            .iter()
            .any(|check| check.status == ValidationStatus::Failed)
        {
            ValidationStatus::Failed
        } else if checks
            .iter()
            .any(|check| check.status == ValidationStatus::Passed)
        {
            ValidationStatus::Passed
        } else {
            ValidationStatus::Skipped
        };
        Self { status, checks }
    }
}

pub async fn validate_rust_sources(name: &str, files: &[PathBuf]) -> ValidationCheckResult {
    let mut errors = Vec::new();
    for path in files {
        match tokio::fs::read_to_string(path).await {
            Ok(source) => {
                if let Err(error) = syn::parse_file(&source) {
                    errors.push(format!("{}: {error}", path.display()));
                }
            }
            Err(error) => errors.push(format!("{}: {error}", path.display())),
        }
    }

    if errors.is_empty() {
        passed(name)
    } else {
        failed(name, None, errors.join("\n"))
    }
}

pub async fn validate_mod_registration(
    name: &str,
    mod_file: &Path,
    expected_module: &str,
) -> ValidationCheckResult {
    let source = match tokio::fs::read_to_string(mod_file).await {
        Ok(source) => source,
        Err(error) => {
            return failed(name, None, format!("{}: {error}", mod_file.display()));
        }
    };

    let has_module = source
        .lines()
        .filter_map(parse_mod_line)
        .any(|module| module == expected_module);

    if has_module {
        passed(name)
    } else {
        failed(
            name,
            None,
            format!(
                "expected module `{expected_module}` to be registered in `{}`",
                mod_file.display()
            ),
        )
    }
}

pub async fn validate_workspace_cargo_check(
    workspace_root: &Path,
    packages: &[&str],
) -> ValidationCheckResult {
    if !workspace_root.join("Cargo.toml").is_file() {
        return skipped(
            "rust_workspace_check",
            "workspace Cargo.toml not found; skipped cargo check",
        );
    }

    let mut command = Command::new("cargo");
    command.arg("check").arg("--quiet");
    for package in packages {
        command.arg("-p").arg(package);
    }
    command.current_dir(workspace_root);

    run_command(
        "rust_workspace_check",
        command,
        format!(
            "cargo check --quiet {}",
            packages
                .iter()
                .map(|package| format!("-p {package}"))
                .collect::<Vec<_>>()
                .join(" ")
        ),
        Some("cargo is unavailable; skipped workspace compile check"),
    )
    .await
}

pub async fn validate_workspace_cargo_check_for_generated_output(
    workspace_root: &Path,
    explicit_output_dir: Option<&str>,
    packages: &[&str],
) -> ValidationCheckResult {
    if explicit_output_dir.is_none() {
        validate_workspace_cargo_check(workspace_root, packages).await
    } else {
        skipped_check(
            "rust_workspace_check",
            "skipped cargo check because generation used explicit output_dir; run a project-local check in that target if needed",
        )
    }
}

pub async fn validate_art_design_pro_typecheck(
    frontend_root: &Path,
    generated_files: &[PathBuf],
) -> ValidationCheckResult {
    if !frontend_root.join("package.json").is_file()
        || !frontend_root.join("tsconfig.json").is_file()
    {
        return skipped(
            "frontend_typecheck",
            "package.json or tsconfig.json not found; skipped frontend typecheck",
        );
    }

    let include = build_art_design_pro_validation_include(frontend_root, generated_files);

    if include.is_empty() {
        return skipped(
            "frontend_typecheck",
            "generated files are outside the frontend project root; skipped frontend typecheck",
        );
    }

    let tsconfig_path = frontend_root.join("tsconfig.summer-mcp.generated.json");
    let tsconfig_source = serde_json::json!({
        "extends": "./tsconfig.json",
        "include": include,
        "exclude": ["dist", "node_modules"],
    });

    if let Err(error) = tokio::fs::write(
        &tsconfig_path,
        serde_json::to_string_pretty(&tsconfig_source)
            .expect("generated frontend validation tsconfig should serialize"),
    )
    .await
    {
        return failed(
            "frontend_typecheck",
            None,
            format!("{}: {error}", tsconfig_path.display()),
        );
    }

    let mut command = Command::new("pnpm");
    command
        .arg("exec")
        .arg("vue-tsc")
        .arg("--noEmit")
        .arg("-p")
        .arg(tsconfig_path.file_name().unwrap())
        .current_dir(frontend_root);

    let result = run_command(
        "frontend_typecheck",
        command,
        format!(
            "pnpm exec vue-tsc --noEmit -p {}",
            tsconfig_path.file_name().unwrap().to_string_lossy()
        ),
        Some("pnpm or vue-tsc is unavailable; skipped frontend typecheck"),
    )
    .await;

    let _ = tokio::fs::remove_file(&tsconfig_path).await;
    result
}

pub async fn validate_frontend_target_output(
    target_preset: FrontendTargetPreset,
    frontend_root_dir: &Path,
    generated_files: &[PathBuf],
) -> GenerationValidationSummary {
    let checks = vec![if target_preset == FrontendTargetPreset::ArtDesignPro {
        validate_art_design_pro_typecheck(frontend_root_dir, generated_files).await
    } else {
        skipped(
            "frontend_typecheck",
            "skipped frontend typecheck because target_preset is not art_design_pro",
        )
    }];

    GenerationValidationSummary::from_checks(checks)
}

pub fn skipped_check(name: &str, detail: impl Into<String>) -> ValidationCheckResult {
    skipped(name, detail)
}

async fn run_command(
    name: &str,
    mut command: Command,
    display: String,
    not_found_skip_detail: Option<&str>,
) -> ValidationCheckResult {
    match command.output().await {
        Ok(output) if output.status.success() => passed_with_command(name, display),
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            failed(
                name,
                Some(display),
                format!(
                    "status: {}\nstdout:\n{}\nstderr:\n{}",
                    output.status.code().map_or_else(
                        || "terminated by signal".to_string(),
                        |code| code.to_string()
                    ),
                    stdout.trim(),
                    stderr.trim()
                ),
            )
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => skipped(
            name,
            not_found_skip_detail.unwrap_or("validation command is unavailable"),
        ),
        Err(error) => failed(name, Some(display), error.to_string()),
    }
}

fn relative_path_string(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn build_art_design_pro_validation_include(
    frontend_root: &Path,
    generated_files: &[PathBuf],
) -> Vec<String> {
    let mut include = generated_files
        .iter()
        .filter_map(|path| {
            path.strip_prefix(frontend_root)
                .ok()
                .map(relative_path_string)
        })
        .collect::<Vec<_>>();

    // Generated pages depend on ambient API namespaces, auto-import declarations,
    // and project-level module declarations that are not imported explicitly.
    include.extend(["*.d.ts".to_string(), "src/**/*.d.ts".to_string()]);
    include.sort();
    include.dedup();
    include
}

fn passed(name: &str) -> ValidationCheckResult {
    passed_with_command(name, String::new())
}

fn passed_with_command(name: &str, command: String) -> ValidationCheckResult {
    ValidationCheckResult {
        name: name.to_string(),
        status: ValidationStatus::Passed,
        command: (!command.is_empty()).then_some(command),
        detail: None,
    }
}

fn skipped(name: &str, detail: impl Into<String>) -> ValidationCheckResult {
    ValidationCheckResult {
        name: name.to_string(),
        status: ValidationStatus::Skipped,
        command: None,
        detail: Some(detail.into()),
    }
}

fn failed(name: &str, command: Option<String>, detail: impl Into<String>) -> ValidationCheckResult {
    ValidationCheckResult {
        name: name.to_string(),
        status: ValidationStatus::Failed,
        command,
        detail: Some(detail.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn validation_summary_marks_failed_if_any_check_failed() {
        let summary = GenerationValidationSummary::from_checks(vec![
            ValidationCheckResult {
                name: "a".to_string(),
                status: ValidationStatus::Passed,
                command: None,
                detail: None,
            },
            ValidationCheckResult {
                name: "b".to_string(),
                status: ValidationStatus::Failed,
                command: None,
                detail: Some("boom".to_string()),
            },
        ]);

        assert_eq!(summary.status, ValidationStatus::Failed);
    }

    #[tokio::test]
    async fn frontend_target_validation_skips_non_art_design_pro_targets() {
        let summary = validate_frontend_target_output(
            FrontendTargetPreset::SummerMcp,
            Path::new("/tmp/ignored"),
            &[],
        )
        .await;

        assert_eq!(summary.status, ValidationStatus::Skipped);
        assert_eq!(summary.checks.len(), 1);
        assert_eq!(summary.checks[0].name, "frontend_typecheck");
        assert_eq!(summary.checks[0].status, ValidationStatus::Skipped);
    }

    #[test]
    fn art_design_pro_validation_include_keeps_generated_files_and_global_declarations() {
        let frontend_root = Path::new("/tmp/art-design-pro");
        let include = build_art_design_pro_validation_include(
            frontend_root,
            &[
                frontend_root.join("src/views/system/showcase-profile/index.vue"),
                frontend_root.join("src/types/api/showcase-profile.d.ts"),
            ],
        );

        assert_eq!(
            include,
            vec![
                "*.d.ts".to_string(),
                "src/**/*.d.ts".to_string(),
                "src/types/api/showcase-profile.d.ts".to_string(),
                "src/views/system/showcase-profile/index.vue".to_string(),
            ]
        );
    }
}
