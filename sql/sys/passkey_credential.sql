-- ============================================================
-- 系统 Passkey 凭据表
-- WebAuthn / FIDO2 认证器注册信息
-- ============================================================

CREATE SCHEMA IF NOT EXISTS sys;

CREATE TABLE sys.passkey_credential (
    id                 BIGSERIAL       PRIMARY KEY,
    user_id            BIGINT          NOT NULL REFERENCES sys."user"(id) ON DELETE CASCADE,
    credential_id      VARCHAR(255)    NOT NULL,
    credential_name    VARCHAR(128)    NOT NULL DEFAULT '',
    public_key         TEXT            NOT NULL DEFAULT '',
    aaguid             VARCHAR(64)     NOT NULL DEFAULT '',
    sign_count         BIGINT          NOT NULL DEFAULT 0,
    transports         JSONB           NOT NULL DEFAULT '[]'::jsonb,
    attachment         VARCHAR(32)     NOT NULL DEFAULT '',
    backed_up          BOOLEAN         NOT NULL DEFAULT FALSE,
    uv_initialized     BOOLEAN         NOT NULL DEFAULT FALSE,
    status             SMALLINT        NOT NULL DEFAULT 1,
    last_used_time     TIMESTAMP,
    create_time        TIMESTAMP       NOT NULL DEFAULT CURRENT_TIMESTAMP,
    update_time        TIMESTAMP       NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE UNIQUE INDEX uk_sys_passkey_credential_credential_id ON sys.passkey_credential (credential_id);
CREATE INDEX idx_sys_passkey_credential_user_status ON sys.passkey_credential (user_id, status);
CREATE INDEX idx_sys_passkey_credential_last_used_time ON sys.passkey_credential (last_used_time);

COMMENT ON TABLE sys.passkey_credential IS '系统 Passkey 凭据表';
COMMENT ON COLUMN sys.passkey_credential.id IS '主键ID';
COMMENT ON COLUMN sys.passkey_credential.user_id IS '用户ID';
COMMENT ON COLUMN sys.passkey_credential.credential_id IS 'WebAuthn credential ID';
COMMENT ON COLUMN sys.passkey_credential.credential_name IS '用户自定义设备名称';
COMMENT ON COLUMN sys.passkey_credential.public_key IS '公钥内容';
COMMENT ON COLUMN sys.passkey_credential.aaguid IS '认证器 AAGUID';
COMMENT ON COLUMN sys.passkey_credential.sign_count IS '签名计数器';
COMMENT ON COLUMN sys.passkey_credential.transports IS '传输方式（JSON 数组）';
COMMENT ON COLUMN sys.passkey_credential.attachment IS '认证器附着类型：platform/cross-platform';
COMMENT ON COLUMN sys.passkey_credential.backed_up IS '是否支持备份';
COMMENT ON COLUMN sys.passkey_credential.uv_initialized IS '是否已启用用户验证';
COMMENT ON COLUMN sys.passkey_credential.status IS '状态：1=正常 2=停用 3=吊销';
COMMENT ON COLUMN sys.passkey_credential.last_used_time IS '最近使用时间';
COMMENT ON COLUMN sys.passkey_credential.create_time IS '创建时间';
COMMENT ON COLUMN sys.passkey_credential.update_time IS '更新时间';
