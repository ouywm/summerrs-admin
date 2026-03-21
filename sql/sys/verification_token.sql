-- ============================================================
-- 系统验证令牌表
-- 用于邮箱验证、重置密码、魔法链接、手机号校验等认证场景
-- ============================================================

CREATE SCHEMA IF NOT EXISTS sys;

CREATE TABLE sys.verification_token (
    id               BIGSERIAL       PRIMARY KEY,
    user_id          BIGINT          REFERENCES sys."user"(id) ON DELETE CASCADE,
    token_type       VARCHAR(32)     NOT NULL DEFAULT 'email_verify',
    target_type      VARCHAR(32)     NOT NULL DEFAULT 'email',
    target_value     VARCHAR(255)    NOT NULL DEFAULT '',
    scene_code       VARCHAR(64)     NOT NULL DEFAULT '',
    token_hash       VARCHAR(128)    NOT NULL DEFAULT '',
    code_hash        VARCHAR(128)    NOT NULL DEFAULT '',
    channel          VARCHAR(32)     NOT NULL DEFAULT 'email',
    payload          JSONB           NOT NULL DEFAULT '{}'::jsonb,
    attempt_count    INT             NOT NULL DEFAULT 0,
    send_count       INT             NOT NULL DEFAULT 1,
    status           SMALLINT        NOT NULL DEFAULT 1,
    expire_time      TIMESTAMP       NOT NULL,
    used_time        TIMESTAMP,
    revoke_time      TIMESTAMP,
    request_ip       INET,
    create_time      TIMESTAMP       NOT NULL DEFAULT CURRENT_TIMESTAMP,
    update_time      TIMESTAMP       NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_sys_verification_token_user_type ON sys.verification_token (user_id, token_type);
CREATE INDEX idx_sys_verification_token_target_type ON sys.verification_token (target_value, token_type);
CREATE INDEX idx_sys_verification_token_status_expire ON sys.verification_token (status, expire_time);
CREATE INDEX idx_sys_verification_token_scene_code ON sys.verification_token (scene_code);

COMMENT ON TABLE sys.verification_token IS '系统验证令牌表';
COMMENT ON COLUMN sys.verification_token.id IS '主键ID';
COMMENT ON COLUMN sys.verification_token.user_id IS '关联用户ID，可为空（注册前验证场景）';
COMMENT ON COLUMN sys.verification_token.token_type IS '令牌类型：email_verify/password_reset/magic_link/phone_verify/change_email';
COMMENT ON COLUMN sys.verification_token.target_type IS '目标类型：email/phone/user';
COMMENT ON COLUMN sys.verification_token.target_value IS '目标值，如邮箱、手机号';
COMMENT ON COLUMN sys.verification_token.scene_code IS '业务场景编码';
COMMENT ON COLUMN sys.verification_token.token_hash IS '长令牌哈希值';
COMMENT ON COLUMN sys.verification_token.code_hash IS '短验证码哈希值';
COMMENT ON COLUMN sys.verification_token.channel IS '发送通道：email/sms/system';
COMMENT ON COLUMN sys.verification_token.payload IS '扩展载荷（JSON）';
COMMENT ON COLUMN sys.verification_token.attempt_count IS '校验尝试次数';
COMMENT ON COLUMN sys.verification_token.send_count IS '发送次数';
COMMENT ON COLUMN sys.verification_token.status IS '状态：1=待使用 2=已使用 3=已过期 4=已撤销 5=锁定';
COMMENT ON COLUMN sys.verification_token.expire_time IS '过期时间';
COMMENT ON COLUMN sys.verification_token.used_time IS '使用时间';
COMMENT ON COLUMN sys.verification_token.revoke_time IS '撤销时间';
COMMENT ON COLUMN sys.verification_token.request_ip IS '请求来源IP';
COMMENT ON COLUMN sys.verification_token.create_time IS '创建时间';
COMMENT ON COLUMN sys.verification_token.update_time IS '更新时间';
