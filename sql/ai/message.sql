-- ============================================================
-- AI 消息表
-- 参考 llmgateway message / 对话消息明细落库
-- ============================================================

CREATE SCHEMA IF NOT EXISTS ai;

CREATE TABLE ai.message (
    id                  BIGSERIAL       PRIMARY KEY,
    conversation_id     BIGINT          NOT NULL REFERENCES ai.conversation(id) ON DELETE CASCADE,
    organization_id     BIGINT          NOT NULL DEFAULT 0,
    project_id          BIGINT          NOT NULL DEFAULT 0,
    user_id             BIGINT          NOT NULL DEFAULT 0,
    session_id          BIGINT          NOT NULL DEFAULT 0,
    thread_id           BIGINT          NOT NULL DEFAULT 0,
    trace_id            BIGINT          NOT NULL DEFAULT 0,
    request_id          VARCHAR(64)     NOT NULL DEFAULT '',
    parent_message_id   BIGINT          NOT NULL DEFAULT 0,
    actor_type          VARCHAR(32)     NOT NULL DEFAULT 'user',
    actor_id            BIGINT          NOT NULL DEFAULT 0,
    role                VARCHAR(32)     NOT NULL DEFAULT 'user',
    message_type        VARCHAR(32)     NOT NULL DEFAULT 'chat',
    status              SMALLINT        NOT NULL DEFAULT 1,
    model_name          VARCHAR(128)    NOT NULL DEFAULT '',
    content_text        TEXT            NOT NULL DEFAULT '',
    content_blocks      JSONB           NOT NULL DEFAULT '[]'::jsonb,
    tool_calls          JSONB           NOT NULL DEFAULT '[]'::jsonb,
    tool_results        JSONB           NOT NULL DEFAULT '[]'::jsonb,
    file_refs           JSONB           NOT NULL DEFAULT '[]'::jsonb,
    token_usage         JSONB           NOT NULL DEFAULT '{}'::jsonb,
    finish_reason       VARCHAR(64)     NOT NULL DEFAULT '',
    latency_ms          INT             NOT NULL DEFAULT 0,
    metadata            JSONB           NOT NULL DEFAULT '{}'::jsonb,
    create_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW(),
    update_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_ai_message_conversation_id_create_time ON ai.message (conversation_id, create_time);
CREATE INDEX idx_ai_message_thread_id_create_time ON ai.message (thread_id, create_time);
CREATE INDEX idx_ai_message_trace_id ON ai.message (trace_id);
CREATE INDEX idx_ai_message_request_id ON ai.message (request_id);
CREATE INDEX idx_ai_message_parent_message_id ON ai.message (parent_message_id);

COMMENT ON TABLE ai.message IS 'AI 消息表（对话消息规范化明细）';
COMMENT ON COLUMN ai.message.id IS '消息ID';
COMMENT ON COLUMN ai.message.conversation_id IS '对话ID';
COMMENT ON COLUMN ai.message.organization_id IS '组织ID';
COMMENT ON COLUMN ai.message.project_id IS '项目ID';
COMMENT ON COLUMN ai.message.user_id IS '用户ID';
COMMENT ON COLUMN ai.message.session_id IS '会话ID';
COMMENT ON COLUMN ai.message.thread_id IS '线程ID';
COMMENT ON COLUMN ai.message.trace_id IS '追踪ID';
COMMENT ON COLUMN ai.message.request_id IS '关联请求ID';
COMMENT ON COLUMN ai.message.parent_message_id IS '父消息ID';
COMMENT ON COLUMN ai.message.actor_type IS '消息生产者类型：user/assistant/tool/system/service_account';
COMMENT ON COLUMN ai.message.actor_id IS '消息生产者ID';
COMMENT ON COLUMN ai.message.role IS '消息角色：system/user/assistant/tool';
COMMENT ON COLUMN ai.message.message_type IS '消息类型：chat/tool_call/tool_result/event';
COMMENT ON COLUMN ai.message.status IS '状态：1=正常 2=编辑中 3=删除';
COMMENT ON COLUMN ai.message.model_name IS '生成该消息的模型名';
COMMENT ON COLUMN ai.message.content_text IS '纯文本内容';
COMMENT ON COLUMN ai.message.content_blocks IS '结构化内容块（JSON）';
COMMENT ON COLUMN ai.message.tool_calls IS '工具调用列表（JSON）';
COMMENT ON COLUMN ai.message.tool_results IS '工具调用结果（JSON）';
COMMENT ON COLUMN ai.message.file_refs IS '关联文件引用（JSON）';
COMMENT ON COLUMN ai.message.token_usage IS '消息级 Token 用量（JSON）';
COMMENT ON COLUMN ai.message.finish_reason IS '结束原因';
COMMENT ON COLUMN ai.message.latency_ms IS '消息生成耗时';
COMMENT ON COLUMN ai.message.metadata IS '扩展元数据（JSON）';
COMMENT ON COLUMN ai.message.create_time IS '创建时间';
COMMENT ON COLUMN ai.message.update_time IS '更新时间';
