//! I18n 示例接口（用于验证 rust-i18n + Locale 提取器）

use rust_i18n::t;
use schemars::JsonSchema;
use serde::Serialize;
use summer_admin_macros::no_auth;
use summer_common::error::ApiResult;
use summer_common::extractor::Locale;
use summer_common::response::Json;
use summer_web::handler::TypeRouter;
use summer_web::{Router, get_api};

#[derive(Debug, Serialize, JsonSchema)]
pub struct I18nDemoVo {
    pub locale: String,
    pub ok: String,
    pub unauthorized: String,
}

/// I18n 示例：根据 `X-Lang` / `Accept-Language` 返回不同语言文本
#[no_auth]
#[get_api("/system/i18n/demo")]
pub async fn i18n_demo(Locale(locale): Locale) -> ApiResult<Json<I18nDemoVo>> {
    let loc = locale.as_str();

    Ok(Json(I18nDemoVo {
        locale: loc.to_string(),
        ok: t!("common.ok", locale = loc).to_string(),
        unauthorized: t!("errors.unauthorized", locale = loc).to_string(),
    }))
}

pub fn routes(router: Router) -> Router {
    router.typed_route(i18n_demo)
}
