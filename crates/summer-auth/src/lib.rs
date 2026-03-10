pub mod bitmap;
pub mod config;
pub mod error;
pub mod online;
pub mod qrcode;
pub mod session;
pub mod storage;
pub mod token;
pub mod user_type;

#[cfg(feature = "web")]
pub mod extractor;
#[cfg(feature = "web")]
pub mod middleware;
#[cfg(feature = "web")]
pub mod path_auth;
#[cfg(feature = "web")]
pub mod plugin;

pub use bitmap::PermissionMap;
pub use config::{AuthConfig, AuthConfigOverride, JwtAlgorithm, MultiAuthConfig, ResolvedTypeConfig, TokenStyle};
pub use error::{AuthError, AuthResult};
pub use online::{OnlineUserItem, OnlineUserPage, OnlineUserQuery};
pub use qrcode::QrCodeState;
pub use session::{
    AdminProfile, BusinessProfile, CustomerProfile, DeviceInfo, DeviceSession, LoginParams,
    SessionManager, UuidSessionData, UserProfile, UserSession, ValidatedAccess, permission_matches,
};
pub use token::{AccessClaims, RefreshClaims, TokenPair};
pub use user_type::{DeviceType, LoginId, UserType};

#[cfg(feature = "web")]
pub use extractor::{AdminUser, BusinessUser, CustomerUser, LoginUser};
#[cfg(feature = "web")]
pub use middleware::AuthLayer;
#[cfg(feature = "web")]
pub use path_auth::{AuthConfigurator, PathAuthBuilder, SummerAuthConfigurator};
#[cfg(feature = "web")]
pub use plugin::SummerAuthPlugin;
