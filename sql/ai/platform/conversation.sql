-- ============================================================
-- AI 对话历史表
-- 面向内置聊天 UI 的可选能力
-- ============================================================

CREATE SCHEMA IF NOT EXISTS ai;

CREATE TABLE ai.conversation (
    id              BIGSERIAL       PRIMARY KEY,
    user_id         BIGINT          NOT NULL,
    project_id      BIGINT          NOT NULL DEFAULT 0,
    session_id      BIGINT          NOT NULL DEFAULT 0,
    thread_id       BIGINT          NOT NULL DEFAULT 0,
    title           VARCHAR(256)    NOT NULL DEFAULT '',
    model_name      VARCHAR(128)    NOT NULL DEFAULT '',
    system_prompt   TEXT            NOT NULL DEFAULT '',
    messages        JSONB           NOT NULL DEFAULT '[]'::jsonb,
    message_count   INT             NOT NULL DEFAULT 0,
    total_tokens    BIGINT          NOT NULL DEFAULT 0,
    pinned          BOOLEAN         NOT NULL DEFAULT FALSE,
    pin_sort        INT             NOT NULL DEFAULT 0,
    status          SMALLINT        NOT NULL DEFAULT 1,
    metadata        JSONB           NOT NULL DEFAULT '{}'::jsonb,
    last_message_at TIMESTAMPTZ,
    create_time     TIMESTAMPTZ     NOT NULL DEFAULT NOW(),
    update_time     TIMESTAMPTZ     NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_ai_conversation_user_id ON ai.conversation (user_id);
CREATE INDEX idx_ai_conversation_project_id ON ai.conversation (project_id);
CREATE INDEX idx_ai_conversation_session_id ON ai.conversation (session_id);
CREATE INDEX idx_ai_conversation_thread_id ON ai.conversation (thread_id);
CREATE INDEX idx_ai_conversation_user_pinned ON ai.conversation (user_id, pinned, pin_sort);
CREATE INDEX idx_ai_conversation_update_time ON ai.conversation (update_time);

COMMENT ON TABLE ai.conversation IS 'AI 对话历史表（用户与 AI 的聊天记录）';
COMMENT ON COLUMN ai.conversation.id IS '对话ID';
COMMENT ON COLUMN ai.conversation.user_id IS '用户ID';
COMMENT ON COLUMN ai.conversation.project_id IS '项目ID';
COMMENT ON COLUMN ai.conversation.session_id IS '会话ID';
COMMENT ON COLUMN ai.conversation.thread_id IS '线程ID';
COMMENT ON COLUMN ai.conversation.title IS '对话标题';
COMMENT ON COLUMN ai.conversation.model_name IS '使用的模型名称';
COMMENT ON COLUMN ai.conversation.system_prompt IS '系统提示词';
COMMENT ON COLUMN ai.conversation.messages IS '消息列表快照缓存（JSON 数组，规范化明细见 ai.message）';
COMMENT ON COLUMN ai.conversation.message_count IS '消息条数';
COMMENT ON COLUMN ai.conversation.total_tokens IS '累计消耗 Token 数';
COMMENT ON COLUMN ai.conversation.pinned IS '是否置顶';
COMMENT ON COLUMN ai.conversation.pin_sort IS '置顶排序（越小越靠前）';
COMMENT ON COLUMN ai.conversation.status IS '状态：1=正常 2=归档 3=删除';
COMMENT ON COLUMN ai.conversation.metadata IS '扩展元数据（JSON）';
COMMENT ON COLUMN ai.conversation.last_message_at IS '最后一条消息时间';
COMMENT ON COLUMN ai.conversation.create_time IS '创建时间';
COMMENT ON COLUMN ai.conversation.update_time IS '最后更新时间';
