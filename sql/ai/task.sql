-- ============================================================
-- AI 异步任务表
-- 对标 new-api task，并补上 request/account/subscription 关联字段
-- ============================================================

CREATE SCHEMA IF NOT EXISTS ai;

CREATE TABLE ai.task (
    id              BIGSERIAL       PRIMARY KEY,
    user_id         BIGINT          NOT NULL DEFAULT 0,
    token_id        BIGINT          NOT NULL DEFAULT 0,
    project_id      BIGINT          NOT NULL DEFAULT 0,
    trace_id        BIGINT          NOT NULL DEFAULT 0,
    channel_id      BIGINT          NOT NULL DEFAULT 0,
    account_id      BIGINT          NOT NULL DEFAULT 0,
    subscription_id BIGINT          NOT NULL DEFAULT 0,
    request_id      VARCHAR(64)     NOT NULL DEFAULT '',
    task_type       SMALLINT        NOT NULL DEFAULT 1,
    platform        VARCHAR(32)     NOT NULL DEFAULT '',
    action          VARCHAR(64)     NOT NULL DEFAULT '',
    model_name      VARCHAR(128)    NOT NULL DEFAULT '',
    request_body    JSONB,
    response_body   JSONB,
    upstream_task_id VARCHAR(128)   NOT NULL DEFAULT '',
    progress        SMALLINT        NOT NULL DEFAULT 0,
    status          SMALLINT        NOT NULL DEFAULT 1,
    fail_reason     TEXT            NOT NULL DEFAULT '',
    quota           BIGINT          NOT NULL DEFAULT 0,
    billing_source  VARCHAR(32)     NOT NULL DEFAULT '',
    submit_time     TIMESTAMPTZ     NOT NULL DEFAULT NOW(),
    start_time      TIMESTAMPTZ,
    finish_time     TIMESTAMPTZ,
    remark          VARCHAR(500)    NOT NULL DEFAULT '',
    create_by       VARCHAR(64)     NOT NULL DEFAULT '',
    create_time     TIMESTAMPTZ     NOT NULL DEFAULT NOW(),
    update_by       VARCHAR(64)     NOT NULL DEFAULT '',
    update_time     TIMESTAMPTZ     NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_ai_task_user_id ON ai.task (user_id);
CREATE INDEX idx_ai_task_token_id ON ai.task (token_id);
CREATE INDEX idx_ai_task_project_id ON ai.task (project_id);
CREATE INDEX idx_ai_task_trace_id ON ai.task (trace_id);
CREATE INDEX idx_ai_task_channel_id ON ai.task (channel_id);
CREATE INDEX idx_ai_task_account_id ON ai.task (account_id);
CREATE INDEX idx_ai_task_subscription_id ON ai.task (subscription_id);
CREATE INDEX idx_ai_task_request_id ON ai.task (request_id);
CREATE INDEX idx_ai_task_status ON ai.task (status);
CREATE INDEX idx_ai_task_upstream_task_id ON ai.task (upstream_task_id);
CREATE INDEX idx_ai_task_create_time ON ai.task (create_time);

COMMENT ON TABLE ai.task IS 'AI 异步任务表（图像/音频/视频/批处理等长任务）';
COMMENT ON COLUMN ai.task.id IS '任务ID';
COMMENT ON COLUMN ai.task.user_id IS '用户ID';
COMMENT ON COLUMN ai.task.token_id IS '令牌ID';
COMMENT ON COLUMN ai.task.project_id IS '所属项目ID（0 表示个人任务）';
COMMENT ON COLUMN ai.task.trace_id IS '所属追踪ID';
COMMENT ON COLUMN ai.task.channel_id IS '渠道ID';
COMMENT ON COLUMN ai.task.account_id IS '账号ID';
COMMENT ON COLUMN ai.task.subscription_id IS '关联订阅ID';
COMMENT ON COLUMN ai.task.request_id IS '来源请求ID';
COMMENT ON COLUMN ai.task.task_type IS '任务类型：1=图像生成 2=图像编辑 3=批量推理 4=音频 5=视频';
COMMENT ON COLUMN ai.task.platform IS '平台标识（midjourney/dall-e/suno/sora 等）';
COMMENT ON COLUMN ai.task.action IS '操作类型';
COMMENT ON COLUMN ai.task.model_name IS '使用的模型名';
COMMENT ON COLUMN ai.task.request_body IS '请求参数';
COMMENT ON COLUMN ai.task.response_body IS '任务结果/轮询结果';
COMMENT ON COLUMN ai.task.upstream_task_id IS '上游任务ID';
COMMENT ON COLUMN ai.task.progress IS '任务进度（0-100）';
COMMENT ON COLUMN ai.task.status IS '状态：1=排队中 2=处理中 3=已完成 4=失败 5=已取消';
COMMENT ON COLUMN ai.task.fail_reason IS '失败原因';
COMMENT ON COLUMN ai.task.quota IS '消耗额度';
COMMENT ON COLUMN ai.task.billing_source IS '计费来源：wallet/subscription/free/admin';
COMMENT ON COLUMN ai.task.submit_time IS '提交时间';
COMMENT ON COLUMN ai.task.start_time IS '开始时间';
COMMENT ON COLUMN ai.task.finish_time IS '完成时间';
COMMENT ON COLUMN ai.task.remark IS '备注';
COMMENT ON COLUMN ai.task.create_by IS '创建人';
COMMENT ON COLUMN ai.task.create_time IS '创建时间';
COMMENT ON COLUMN ai.task.update_by IS '更新人';
COMMENT ON COLUMN ai.task.update_time IS '更新时间';
