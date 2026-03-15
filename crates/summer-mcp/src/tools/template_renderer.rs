use minijinja::{Environment, UndefinedBehavior};
use rmcp::ErrorData as McpError;
use serde::Serialize;

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

        for template in templates {
            env.add_template(template.name, template.source)
                .map_err(|error| {
                    McpError::internal_error(
                        format!("failed to register template `{}`: {error}", template.name),
                        None,
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
            McpError::internal_error(
                format!("failed to load template `{template_name}`: {error}"),
                None,
            )
        })?;
        let mut rendered = template.render(context).map_err(|error| {
            McpError::internal_error(
                format!("failed to render template `{template_name}`: {error}"),
                None,
            )
        })?;
        if !rendered.ends_with('\n') {
            rendered.push('\n');
        }
        Ok(rendered)
    }
}
