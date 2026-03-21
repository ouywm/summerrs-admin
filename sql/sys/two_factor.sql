-- ============================================================
-- 系统二次认证因子表
-- 当前以 TOTP 为主，也为 email/sms 等因子预留扩展位
-- ============================================================

CREATE SCHEMA IF NOT EXISTS sys;

CREATE TABLE sys.two_factor (
    id                 BIGSERIAL       PRIMARY KEY,
    user_id            BIGINT          NOT NULL REFERENCES sys."user"(id) ON DELETE CASCADE,
    factor_type        VARCHAR(32)     NOT NULL DEFAULT 'totp',
    factor_name        VARCHAR(64)     NOT NULL DEFAULT '',
    secret_ciphertext  TEXT            NOT NULL DEFAULT '',
    secret_ref         VARCHAR(255)    NOT NULL DEFAULT '',
    issuer             VARCHAR(128)    NOT NULL DEFAULT '',
    account_name       VARCHAR(128)    NOT NULL DEFAULT '',
    algorithm          VARCHAR(16)     NOT NULL DEFAULT 'SHA1',
    digits             SMALLINT        NOT NULL DEFAULT 6,
    period_seconds     SMALLINT        NOT NULL DEFAULT 30,
    is_primary         BOOLEAN         NOT NULL DEFAULT TRUE,
    verified           BOOLEAN         NOT NULL DEFAULT FALSE,
    status             SMALLINT        NOT NULL DEFAULT 1,
    verified_time      TIMESTAMP,
    last_used_time     TIMESTAMP,
    metadata           JSONB           NOT NULL DEFAULT '{}'::jsonb,
    create_time        TIMESTAMP       NOT NULL DEFAULT CURRENT_TIMESTAMP,
    update_time        TIMESTAMP       NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE UNIQUE INDEX uk_sys_two_factor_user_name ON sys.two_factor (user_id, factor_name);
CREATE INDEX idx_sys_two_factor_user_status ON sys.two_factor (user_id, status);
CREATE INDEX idx_sys_two_factor_user_type ON sys.two_factor (user_id, factor_type);

COMMENT ON TABLE sys.two_factor IS '系统二次认证因子表';
COMMENT ON COLUMN sys.two_factor.id IS '主键ID';
COMMENT ON COLUMN sys.two_factor.user_id IS '用户ID';
COMMENT ON COLUMN sys.two_factor.factor_type IS '因子类型：totp/email/sms/app';
COMMENT ON COLUMN sys.two_factor.factor_name IS '因子名称（用户维度唯一）';
COMMENT ON COLUMN sys.two_factor.secret_ciphertext IS '加密后的因子密钥';
COMMENT ON COLUMN sys.two_factor.secret_ref IS '外部密钥引用';
COMMENT ON COLUMN sys.two_factor.issuer IS 'TOTP issuer';
COMMENT ON COLUMN sys.two_factor.account_name IS 'TOTP account name';
COMMENT ON COLUMN sys.two_factor.algorithm IS '签名算法';
COMMENT ON COLUMN sys.two_factor.digits IS '验证码位数';
COMMENT ON COLUMN sys.two_factor.period_seconds IS '验证码周期秒数';
COMMENT ON COLUMN sys.two_factor.is_primary IS '是否主因子';
COMMENT ON COLUMN sys.two_factor.verified IS '是否已完成初始化校验';
COMMENT ON COLUMN sys.two_factor.status IS '状态：1=启用 2=禁用 3=待初始化 4=已撤销';
COMMENT ON COLUMN sys.two_factor.verified_time IS '初始化验证时间';
COMMENT ON COLUMN sys.two_factor.last_used_time IS '最近使用时间';
COMMENT ON COLUMN sys.two_factor.metadata IS '扩展配置（JSON）';
COMMENT ON COLUMN sys.two_factor.create_time IS '创建时间';
COMMENT ON COLUMN sys.two_factor.update_time IS '更新时间';
