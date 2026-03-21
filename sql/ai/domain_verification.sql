-- ============================================================
-- AI 域名验证表
-- 参考 hadrian domain_verifications / 企业域归属验证
-- ============================================================

CREATE SCHEMA IF NOT EXISTS ai;

CREATE TABLE ai.domain_verification (
    id                  BIGSERIAL       PRIMARY KEY,
    organization_id     BIGINT          NOT NULL REFERENCES ai.organization(id) ON DELETE CASCADE,
    sso_config_id       BIGINT          REFERENCES ai.org_sso_config(id) ON DELETE SET NULL,
    domain_name         VARCHAR(255)    NOT NULL,
    verification_type   VARCHAR(32)     NOT NULL DEFAULT 'dns_txt',
    verification_token  VARCHAR(255)    NOT NULL DEFAULT '',
    dns_record_name     VARCHAR(255)    NOT NULL DEFAULT '',
    dns_record_type     VARCHAR(16)     NOT NULL DEFAULT 'TXT',
    dns_record_value    VARCHAR(512)    NOT NULL DEFAULT '',
    http_file_path      VARCHAR(255)    NOT NULL DEFAULT '',
    http_file_content   VARCHAR(512)    NOT NULL DEFAULT '',
    status              SMALLINT        NOT NULL DEFAULT 1,
    attempt_count       INT             NOT NULL DEFAULT 0,
    last_checked_at     TIMESTAMPTZ,
    verified_at         TIMESTAMPTZ,
    expire_time         TIMESTAMPTZ,
    metadata            JSONB           NOT NULL DEFAULT '{}'::jsonb,
    create_by           VARCHAR(64)     NOT NULL DEFAULT '',
    create_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW(),
    update_by           VARCHAR(64)     NOT NULL DEFAULT '',
    update_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX uk_ai_domain_verification_org_domain ON ai.domain_verification (organization_id, domain_name);
CREATE INDEX idx_ai_domain_verification_status ON ai.domain_verification (status);
CREATE INDEX idx_ai_domain_verification_sso_config_id ON ai.domain_verification (sso_config_id);

COMMENT ON TABLE ai.domain_verification IS 'AI 域名验证表';
COMMENT ON COLUMN ai.domain_verification.id IS '域名验证ID';
COMMENT ON COLUMN ai.domain_verification.organization_id IS '组织ID';
COMMENT ON COLUMN ai.domain_verification.sso_config_id IS '关联 SSO 配置ID';
COMMENT ON COLUMN ai.domain_verification.domain_name IS '待验证域名';
COMMENT ON COLUMN ai.domain_verification.verification_type IS '验证方式：dns_txt/http_file/email';
COMMENT ON COLUMN ai.domain_verification.verification_token IS '验证令牌';
COMMENT ON COLUMN ai.domain_verification.dns_record_name IS 'DNS 记录名';
COMMENT ON COLUMN ai.domain_verification.dns_record_type IS 'DNS 记录类型';
COMMENT ON COLUMN ai.domain_verification.dns_record_value IS 'DNS 记录值';
COMMENT ON COLUMN ai.domain_verification.http_file_path IS 'HTTP 校验文件路径';
COMMENT ON COLUMN ai.domain_verification.http_file_content IS 'HTTP 校验文件内容';
COMMENT ON COLUMN ai.domain_verification.status IS '状态：1=待验证 2=已验证 3=失败 4=过期';
COMMENT ON COLUMN ai.domain_verification.attempt_count IS '尝试次数';
COMMENT ON COLUMN ai.domain_verification.last_checked_at IS '最后校验时间';
COMMENT ON COLUMN ai.domain_verification.verified_at IS '验证成功时间';
COMMENT ON COLUMN ai.domain_verification.expire_time IS '验证过期时间';
COMMENT ON COLUMN ai.domain_verification.metadata IS '扩展元数据（JSON）';
COMMENT ON COLUMN ai.domain_verification.create_by IS '创建人';
COMMENT ON COLUMN ai.domain_verification.create_time IS '创建时间';
COMMENT ON COLUMN ai.domain_verification.update_by IS '更新人';
COMMENT ON COLUMN ai.domain_verification.update_time IS '更新时间';
