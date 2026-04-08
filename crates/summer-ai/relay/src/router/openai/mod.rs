pub mod chat;
pub mod embeddings;
pub mod responses;

use summer_web::Router;

pub fn routes() -> Router {
    Router::new()
        .merge(chat::routes())
        .merge(embeddings::routes())
        .merge(responses::routes())
}
