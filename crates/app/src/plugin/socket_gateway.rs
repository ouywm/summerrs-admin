use summer::app::AppBuilder;
use summer::async_trait;
use summer::config::ConfigRegistry;
use summer::plugin::{ComponentRegistry, Plugin};
use summer_web::config::SocketIOConfig;
use summer_web::extractor::Component;
use summer_web::handler::auto_socketio_setup;
use summer_web::socketioxide::extract::{SocketRef, TryData};
use summer_web::socketioxide::handler::ConnectHandler;
use summer_web::socketioxide::SocketIo;

use crate::socketio::room;
use crate::socketio::service::{SocketConnectAuthDto, SocketGatewayService};

pub struct SocketGatewayPlugin;

#[async_trait]
impl Plugin for SocketGatewayPlugin {
    async fn build(&self, app: &mut AppBuilder) {
        let Ok(socketio_config) = app.get_config::<SocketIOConfig>() else {
            tracing::info!("Socket.IO config not found, socket gateway skipped");
            return;
        };

        let Some(io) = app.get_component::<SocketIo>() else {
            tracing::info!("Socket.IO component not found, socket gateway skipped");
            return;
        };

        let namespace = socketio_config.default_namespace.clone();
        if namespace != "/" && io.of(&namespace).is_some() {
            io.delete_ns(&namespace);
        }
        io.ns(
            namespace.clone(),
            socket_connected.with(socket_auth_middleware),
        );

        tracing::info!("Socket gateway registered namespace auth middleware: {namespace}");
    }

    fn name(&self) -> &str {
        "socket-gateway"
    }

    fn dependencies(&self) -> Vec<&str> {
        vec!["summer_web::WebPlugin"]
    }
}

async fn socket_connected(socket: SocketRef) {
    auto_socketio_setup(&socket);
}

async fn socket_auth_middleware(
    socket: SocketRef,
    TryData(auth): TryData<SocketConnectAuthDto>,
    Component(service): Component<SocketGatewayService>,
) -> Result<(), String> {
    let auth = auth.map_err(|_| "Missing accessToken in Socket.IO auth payload".to_string())?;
    let access_token = auth.access_token.trim();
    if access_token.is_empty() {
        return Err("Missing accessToken in Socket.IO auth payload".to_string());
    }

    let (session, identity) = service
        .authenticate_connection(&socket.id.to_string(), socket.ns(), access_token)
        .await
        .map_err(|err| err.to_string())?;

    // 绑定 Room：user:{user_id}, all-{user_type}, role:{role}
    let mut rooms = vec![
        room::user_room(session.user_id),
        room::broadcast_room(&session.user_type),
    ];
    for role in &identity.roles {
        rooms.push(room::role_room(role));
    }
    socket.join(rooms);

    // 存入 per-socket extensions，断连时可直接取用
    socket.extensions.insert(identity);

    Ok(())
}
