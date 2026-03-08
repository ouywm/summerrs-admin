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

pub use config::{AuthConfig, JwtAlgorithm, TokenStyle};
pub use error::{AuthError, AuthResult};
pub use online::{OnlineUser, OnlineUserPage, OnlineUserQuery};
pub use qrcode::QrCodeState;
pub use session::{
    AdminProfile, BusinessProfile, CustomerProfile, LoginParams, SessionManager, UserProfile,
};
pub use token::TokenPair;
pub use token::{JwtClaims, JwtHandler};
pub use user_type::{DeviceType, LoginId, UserType};

#[cfg(feature = "web")]
pub use extractor::{AdminUser, BusinessUser, CustomerUser, LoginUser};
#[cfg(feature = "web")]
pub use middleware::AuthLayer;
#[cfg(feature = "web")]
pub use path_auth::{AuthConfigurator, PathAuthBuilder, SummerAuthConfigurator};
#[cfg(feature = "web")]
pub use plugin::SummerAuthPlugin;
