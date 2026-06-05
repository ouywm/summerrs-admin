#[path = "../router.rs"]
mod router;

use std::error::Error;
use std::fs;
use std::path::PathBuf;

use summer_web::OpenApi;
use summer_web::aide::openapi::Info;

fn main() -> Result<(), Box<dyn Error>> {
    let output_path = output_path();
    let document = build_openapi_document()?;

    if let Some(parent) = output_path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)?;
    }

    fs::write(&output_path, document)?;
    eprintln!("OpenAPI document written to {}", output_path.display());

    Ok(())
}

fn output_path() -> PathBuf {
    match std::env::args().nth(1) {
        Some(path) => PathBuf::from(path),
        None => PathBuf::from("target/openapi/openapi.json"),
    }
}

fn build_openapi_document() -> Result<String, serde_json::Error> {
    summer_web::enable_openapi();

    let mut api = OpenApi {
        info: Info {
            title: "summerrs-admin".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            ..Default::default()
        },
        ..Default::default()
    };

    let _router = router::router().finish_api(&mut api);

    serde_json::to_string_pretty(&api)
}
