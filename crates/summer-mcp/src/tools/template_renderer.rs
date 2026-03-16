use minijinja::{Environment, UndefinedBehavior};
use rmcp::ErrorData as McpError;
use serde::Serialize;

use crate::error_model::internal_error;

#[derive(Debug, Clone, Copy)]
pub(crate) struct EmbeddedTemplate {
    pub name: &'static str,
    pub source: &'static str,
}

pub(crate) struct TemplateRenderer {
    env: Environment<'static>,
}

impl TemplateRenderer {
    pub(crate) fn new(templates: &[EmbeddedTemplate]) -> Result<Self, McpError> {
        let mut env = Environment::new();
        env.set_undefined_behavior(UndefinedBehavior::Strict);
        env.set_trim_blocks(true);
        env.set_lstrip_blocks(true);

        for template in templates {
            env.add_template(template.name, template.source)
                .map_err(|error| {
                    internal_error(
                        "template_render_failed",
                        "Template rendering failed",
                        Some("Check the embedded template name and syntax before regenerating."),
                        Some(format!(
                            "failed to register template `{}`: {error}",
                            template.name
                        )),
                        Some(serde_json::json!({ "template": template.name })),
                    )
                })?;
        }

        Ok(Self { env })
    }

    pub(crate) fn render<T: Serialize>(
        &self,
        template_name: &str,
        context: &T,
    ) -> Result<String, McpError> {
        let template = self.env.get_template(template_name).map_err(|error| {
            internal_error(
                "template_render_failed",
                "Template rendering failed",
                Some("Check that the requested template name exists in the embedded template set."),
                Some(format!(
                    "failed to load template `{template_name}`: {error}"
                )),
                Some(serde_json::json!({ "template": template_name })),
            )
        })?;
        let mut rendered = template.render(context).map_err(|error| {
            internal_error(
                "template_render_failed",
                "Template rendering failed",
                Some("Check the template context fields and the referenced template variables."),
                Some(format!(
                    "failed to render template `{template_name}`: {error}"
                )),
                Some(serde_json::json!({ "template": template_name })),
            )
        })?;
        if !rendered.ends_with('\n') {
            rendered.push('\n');
        }
        Ok(rendered)
    }
}
