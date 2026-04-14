pub mod bitmap;
pub mod config;
pub mod error;
pub mod extractor;
pub mod middleware;
pub mod online;
pub mod path_auth;
pub mod plugin;
pub mod qrcode;
pub mod session;
pub mod storage;
pub mod token;
pub mod user_type;

pub use bitmap::PermissionMap;
pub use config::{AuthConfig, JwtAlgorithm};
pub use error::{AuthError, AuthResult};
pub use extractor::{LoginUser, OptionalLoginUser};
pub use middleware::AuthLayer;
pub use online::{OnlineUserItem, OnlineUserPage, OnlineUserQuery};
pub use path_auth::{AuthConfigurator, PathAuthBuilder, SummerAuthConfigurator};
pub use qrcode::QrCodeState;
pub use session::{
    DeviceInfo, DeviceSession, LoginParams, SessionManager, UserProfile, UserSession,
    ValidatedAccess, permission_matches,
};
pub use token::{AccessClaims, RefreshClaims, TokenPair};
pub use user_type::{DeviceType, LoginId};
