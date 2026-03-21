use summer_web::extractor::Component;
use summer_web::on_connection;
use summer_web::on_disconnect;
use summer_web::socketioxide::extract::SocketRef;
use summer_web::socketioxide::socket::DisconnectReason;
use tracing::{info, warn};

use super::service::{SocketGatewayService, SocketIdentity};

#[on_connection]
async fn on_connection(socket: SocketRef) {
    info!(
        socket_id = %socket.id,
        namespace = socket.ns(),
        "Socket.IO connected"
    );
}

#[on_disconnect]
async fn on_disconnect(
    socket: SocketRef,
    reason: DisconnectReason,
    Component(service): Component<SocketGatewayService>,
) {
    let socket_id = socket.id.to_string();
    let namespace = socket.ns().to_string();
    let login_id = socket
        .extensions
        .get::<SocketIdentity>()
        .map(|id| id.login_id.clone());

    match service
        .unregister_connection(&socket_id, &namespace, login_id.as_deref())
        .await
    {
        Ok(()) => {
            info!(
                socket_id = %socket_id,
                namespace = %namespace,
                login_id = login_id.as_deref().unwrap_or("none"),
                reason = ?reason,
                "Socket.IO disconnected, session cleaned"
            );
        }
        Err(err) => {
            warn!(
                socket_id = %socket_id,
                namespace = %namespace,
                login_id = login_id.as_deref().unwrap_or("none"),
                reason = ?reason,
                error = %err,
                "Socket.IO unregister failed"
            );
        }
    }
}
