use crate::error::{AuthError, AuthResult};
use crate::session::manager::LoginParams;
use crate::session::SessionManager;
use crate::token::generator::TokenGenerator;
use crate::token::pair::TokenPair;
use crate::user_type::LoginId;

fn qr_key(code: &str) -> String {
    format!("auth:qr:{code}")
}

/// QR 码登录状态
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum QrCodeState {
    /// 等待扫码
    Pending,
    /// 已扫码，等待确认
    Scanned { login_id: LoginId },
    /// 已确认，可以获取 token
    Confirmed { token_pair: TokenPair },
    /// 已取消
    Cancelled,
}

impl SessionManager {
    /// 生成 QR 码（返回 code）
    pub async fn create_qr_code(&self) -> AuthResult<String> {
        let code = TokenGenerator::uuid();
        let state = QrCodeState::Pending;
        let json =
            serde_json::to_string(&state).map_err(|e| AuthError::Internal(e.to_string()))?;

        self.storage
            .set_string(&qr_key(&code), &json, self.config.qr_code_timeout)
            .await
            .map_err(|e| AuthError::StorageError(e.to_string()))?;

        Ok(code)
    }

    /// 扫码（移动端调用 — 标记已扫码）
    pub async fn scan_qr_code(&self, code: &str, login_id: &LoginId) -> AuthResult<()> {
        let state = self.get_qr_state_internal(code).await?;
        match state {
            QrCodeState::Pending => {}
            _ => return Err(AuthError::QrCodeInvalidState),
        }

        let new_state = QrCodeState::Scanned {
            login_id: login_id.clone(),
        };
        let json =
            serde_json::to_string(&new_state).map_err(|e| AuthError::Internal(e.to_string()))?;
        self.storage
            .set_string(&qr_key(code), &json, self.config.qr_code_timeout)
            .await
            .map_err(|e| AuthError::StorageError(e.to_string()))?;

        Ok(())
    }

    /// 确认登录（移动端调用 — 扫码后确认）
    pub async fn confirm_qr_code(&self, code: &str, params: LoginParams) -> AuthResult<()> {
        let state = self.get_qr_state_internal(code).await?;
        match state {
            QrCodeState::Scanned { login_id } if login_id == params.login_id => {}
            _ => return Err(AuthError::QrCodeInvalidState),
        }

        // 执行登录
        let token_pair = self.login(params).await?;

        let new_state = QrCodeState::Confirmed { token_pair };
        let json =
            serde_json::to_string(&new_state).map_err(|e| AuthError::Internal(e.to_string()))?;
        self.storage
            .set_string(&qr_key(code), &json, self.config.qr_code_timeout)
            .await
            .map_err(|e| AuthError::StorageError(e.to_string()))?;

        Ok(())
    }

    /// 取消扫码登录
    pub async fn cancel_qr_code(&self, code: &str) -> AuthResult<()> {
        let state = self.get_qr_state_internal(code).await?;
        match state {
            QrCodeState::Pending | QrCodeState::Scanned { .. } => {}
            _ => return Err(AuthError::QrCodeInvalidState),
        }

        let new_state = QrCodeState::Cancelled;
        let json =
            serde_json::to_string(&new_state).map_err(|e| AuthError::Internal(e.to_string()))?;
        self.storage
            .set_string(&qr_key(code), &json, self.config.qr_code_timeout)
            .await
            .map_err(|e| AuthError::StorageError(e.to_string()))?;

        Ok(())
    }

    /// 查询 QR 码状态（Web 端轮询）
    pub async fn get_qr_code_state(&self, code: &str) -> AuthResult<QrCodeState> {
        self.get_qr_state_internal(code).await
    }

    /// 内部方法：获取 QR 码状态
    async fn get_qr_state_internal(&self, code: &str) -> AuthResult<QrCodeState> {
        let json = self
            .storage
            .get_string(&qr_key(code))
            .await
            .map_err(|e| AuthError::StorageError(e.to_string()))?
            .ok_or(AuthError::QrCodeNotFound)?;

        serde_json::from_str(&json).map_err(|e| AuthError::Internal(e.to_string()))
    }
}
