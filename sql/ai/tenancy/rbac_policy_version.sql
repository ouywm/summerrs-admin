-- ============================================================
-- AI RBAC 策略版本表
-- 参考 hadrian org_rbac_policy_versions / 策略版本快照
-- ============================================================

CREATE SCHEMA IF NOT EXISTS ai;

CREATE TABLE ai.rbac_policy_version (
    id                  BIGSERIAL       PRIMARY KEY,
    policy_id           BIGINT          NOT NULL,
    version_no          INT             NOT NULL DEFAULT 1,
    change_summary      VARCHAR(255)    NOT NULL DEFAULT '',
    policy_document     JSONB           NOT NULL DEFAULT '{}'::jsonb,
    status              SMALLINT        NOT NULL DEFAULT 1,
    published_by        VARCHAR(64)     NOT NULL DEFAULT '',
    published_at        TIMESTAMPTZ,
    create_by           VARCHAR(64)     NOT NULL DEFAULT '',
    create_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX uk_ai_rbac_policy_version_policy_version_no ON ai.rbac_policy_version (policy_id, version_no);
CREATE INDEX idx_ai_rbac_policy_version_status ON ai.rbac_policy_version (policy_id, status);

COMMENT ON TABLE ai.rbac_policy_version IS 'AI RBAC 策略版本表';
COMMENT ON COLUMN ai.rbac_policy_version.id IS '策略版本ID';
COMMENT ON COLUMN ai.rbac_policy_version.policy_id IS '所属策略ID';
COMMENT ON COLUMN ai.rbac_policy_version.version_no IS '版本号';
COMMENT ON COLUMN ai.rbac_policy_version.change_summary IS '变更摘要';
COMMENT ON COLUMN ai.rbac_policy_version.policy_document IS '策略文档（JSON）';
COMMENT ON COLUMN ai.rbac_policy_version.status IS '状态：1=草稿 2=生效 3=归档';
COMMENT ON COLUMN ai.rbac_policy_version.published_by IS '发布人';
COMMENT ON COLUMN ai.rbac_policy_version.published_at IS '发布时间';
COMMENT ON COLUMN ai.rbac_policy_version.create_by IS '创建人';
COMMENT ON COLUMN ai.rbac_policy_version.create_time IS '创建时间';
