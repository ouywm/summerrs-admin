-- ============================================================
-- AI 提示词模板表
-- ============================================================

CREATE SCHEMA IF NOT EXISTS ai;

CREATE TABLE ai.prompt_template (
    id              BIGSERIAL       PRIMARY KEY,
    user_id         BIGINT          NOT NULL DEFAULT 0,
    name            VARCHAR(128)    NOT NULL DEFAULT '',
    description     VARCHAR(500)    NOT NULL DEFAULT '',
    content         TEXT            NOT NULL DEFAULT '',
    model_name      VARCHAR(128)    NOT NULL DEFAULT '',
    category        VARCHAR(64)     NOT NULL DEFAULT '',
    tags            JSONB           NOT NULL DEFAULT '[]'::jsonb,
    is_public       BOOLEAN         NOT NULL DEFAULT FALSE,
    use_count       BIGINT          NOT NULL DEFAULT 0,
    template_sort   INT             NOT NULL DEFAULT 0,
    status          SMALLINT        NOT NULL DEFAULT 1,
    remark          VARCHAR(500)    NOT NULL DEFAULT '',
    create_by       VARCHAR(64)     NOT NULL DEFAULT '',
    create_time     TIMESTAMPTZ     NOT NULL DEFAULT NOW(),
    update_by       VARCHAR(64)     NOT NULL DEFAULT '',
    update_time     TIMESTAMPTZ     NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_ai_prompt_template_user_id ON ai.prompt_template (user_id);
CREATE INDEX idx_ai_prompt_template_category ON ai.prompt_template (category);
CREATE INDEX idx_ai_prompt_template_is_public ON ai.prompt_template (is_public);
CREATE INDEX idx_ai_prompt_template_status ON ai.prompt_template (status);

COMMENT ON TABLE ai.prompt_template IS 'AI 提示词模板表（可复用的系统提示词/角色预设）';
COMMENT ON COLUMN ai.prompt_template.id IS '模板ID';
COMMENT ON COLUMN ai.prompt_template.user_id IS '创建者用户ID（0=系统模板）';
COMMENT ON COLUMN ai.prompt_template.name IS '模板名称';
COMMENT ON COLUMN ai.prompt_template.description IS '模板简介';
COMMENT ON COLUMN ai.prompt_template.content IS '提示词内容';
COMMENT ON COLUMN ai.prompt_template.model_name IS '推荐模型';
COMMENT ON COLUMN ai.prompt_template.category IS '分类标签';
COMMENT ON COLUMN ai.prompt_template.tags IS '标签数组（JSON）';
COMMENT ON COLUMN ai.prompt_template.is_public IS '是否公开';
COMMENT ON COLUMN ai.prompt_template.use_count IS '使用次数';
COMMENT ON COLUMN ai.prompt_template.template_sort IS '排序';
COMMENT ON COLUMN ai.prompt_template.status IS '状态：1=启用 2=禁用';
COMMENT ON COLUMN ai.prompt_template.remark IS '备注';
COMMENT ON COLUMN ai.prompt_template.create_by IS '创建人';
COMMENT ON COLUMN ai.prompt_template.create_time IS '创建时间';
COMMENT ON COLUMN ai.prompt_template.update_by IS '更新人';
COMMENT ON COLUMN ai.prompt_template.update_time IS '更新时间';
