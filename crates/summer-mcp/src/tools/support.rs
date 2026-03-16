use std::{
    collections::BTreeSet,
    error::Error as StdError,
    path::{Path, PathBuf},
};

use rmcp::ErrorData as McpError;
use serde::Serialize;

use crate::error_model::internal_error;

pub(crate) async fn sync_mod_file(mod_file: &Path, module: &str) -> Result<(), McpError> {
    let existing = match tokio::fs::read_to_string(mod_file).await {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => {
            return Err(io_error(
                format!("read rust module file `{}`", mod_file.display()),
                error,
            ));
        }
    };

    let mut modules = existing
        .lines()
        .filter_map(parse_mod_line)
        .collect::<BTreeSet<_>>();
    modules.insert(module.to_string());

    let contents = modules
        .into_iter()
        .map(|module| format!("pub mod {module};"))
        .collect::<Vec<_>>()
        .join("\n");
    let contents = format!("{contents}\n");

    tokio::fs::write(mod_file, contents).await.map_err(|error| {
        io_error(
            format!("write rust module file `{}`", mod_file.display()),
            error,
        )
    })
}

pub(crate) fn parse_mod_line(line: &str) -> Option<String> {
    let line = line.trim();
    line.strip_prefix("pub mod ")
        .and_then(|rest| rest.strip_suffix(';'))
        .map(ToOwned::to_owned)
}

pub(crate) fn workspace_root() -> Result<PathBuf, McpError> {
    // Prefer explicit env var override for deployed / relocated binaries.
    if let Ok(root) = std::env::var("SUMMER_MCP_WORKSPACE_ROOT") {
        let path = PathBuf::from(root);
        if path.is_absolute() {
            return Ok(path);
        }
    }

    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .map_err(|error| io_error("resolve workspace root", error))
}

pub(crate) fn io_error(action: impl Into<String>, error: std::io::Error) -> McpError {
    let action = action.into();
    internal_error(
        "filesystem_operation_failed",
        "Filesystem operation failed",
        Some("Check output_dir, workspace location, and filesystem permissions."),
        Some(format!("failed to {action}: {error}")),
        Some(serde_json::json!({
            "action": action,
            "kind": error.kind().to_string(),
        })),
    )
}

pub(crate) fn error_chain_message(error: &dyn StdError) -> String {
    let mut parts = Vec::new();
    let mut current = Some(error);
    while let Some(item) = current {
        let message = item.to_string();
        if parts.last() != Some(&message) {
            parts.push(message);
        }
        current = item.source();
    }
    parts.join(": caused by: ")
}

/// Strip common table prefix (`sys_`, `biz_`) to derive a route base name.
pub(crate) fn default_route_base(table: &str) -> String {
    table
        .strip_prefix("sys_")
        .or_else(|| table.strip_prefix("biz_"))
        .unwrap_or(table)
        .to_string()
}

/// Resolve an output directory that may be absolute or relative to the workspace root.
pub(crate) fn resolve_output_dir(
    workspace_root: &Path,
    output_dir: Option<&str>,
    default_relative: &str,
) -> PathBuf {
    match output_dir {
        Some(dir) => {
            let path = PathBuf::from(dir);
            if path.is_absolute() {
                path
            } else {
                workspace_root.join(path)
            }
        }
        None => workspace_root.join(default_relative),
    }
}

pub(crate) fn sanitize_file_stem(value: &str) -> String {
    let sanitized = value
        .chars()
        .map(|ch| match ch {
            'a'..='z' | 'A'..='Z' | '0'..='9' => ch,
            _ => '-',
        })
        .collect::<String>();
    let trimmed = sanitized.trim_matches('-');
    if trimmed.is_empty() {
        "export".to_string()
    } else {
        trimmed.to_string()
    }
}

pub(crate) async fn write_pretty_json_file<T: Serialize>(
    path: &Path,
    value: &T,
    label: &str,
) -> Result<(), McpError> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await.map_err(|error| {
            io_error(
                format!("create export output directory `{}`", parent.display()),
                error,
            )
        })?;
    }

    let contents = serde_json::to_string_pretty(value).map_err(|error| {
        internal_error(
            "serialization_failed",
            "Serialization failed",
            Some("Check that the generated value can be encoded as JSON."),
            Some(format!("failed to serialize {label}: {error}")),
            Some(serde_json::json!({ "label": label })),
        )
    })?;

    tokio::fs::write(path, contents)
        .await
        .map_err(|error| io_error(format!("write {label} file `{}`", path.display()), error))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_mod_line_reads_module_declarations() {
        assert_eq!(
            parse_mod_line("pub mod sys_role;"),
            Some("sys_role".to_string())
        );
        assert_eq!(parse_mod_line("pub use something;"), None);
    }

    #[test]
    fn sanitize_file_stem_normalizes_non_identifier_characters() {
        assert_eq!(sanitize_file_stem("sys/user"), "sys-user".to_string());
        assert_eq!(sanitize_file_stem(" 菜单 "), "export".to_string());
    }

    #[test]
    fn error_chain_message_flattens_nested_causes() {
        let error = anyhow::anyhow!("outer context").context("inner context");
        assert_eq!(
            error_chain_message(error.as_ref()),
            "inner context: caused by: outer context".to_string()
        );
    }
}
