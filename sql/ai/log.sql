-- ============================================================
-- AI 消费日志表
-- 这是账务/审计摘要表，不等同于完整请求表
-- 详细链路见 ai.request / ai.request_execution
-- ============================================================

CREATE SCHEMA IF NOT EXISTS ai;

CREATE TABLE ai.log (
    id                  BIGSERIAL       PRIMARY KEY,
    user_id             BIGINT          NOT NULL DEFAULT 0,
    token_id            BIGINT          NOT NULL DEFAULT 0,
    token_name          VARCHAR(128)    NOT NULL DEFAULT '',
    project_id          BIGINT          NOT NULL DEFAULT 0,
    conversation_id     BIGINT          NOT NULL DEFAULT 0,
    message_id          BIGINT          NOT NULL DEFAULT 0,
    session_id          BIGINT          NOT NULL DEFAULT 0,
    thread_id           BIGINT          NOT NULL DEFAULT 0,
    trace_id            BIGINT          NOT NULL DEFAULT 0,
    channel_id          BIGINT          NOT NULL DEFAULT 0,
    channel_name        VARCHAR(128)    NOT NULL DEFAULT '',
    account_id          BIGINT          NOT NULL DEFAULT 0,
    account_name        VARCHAR(128)    NOT NULL DEFAULT '',
    execution_id        BIGINT          NOT NULL DEFAULT 0,
    endpoint            VARCHAR(64)     NOT NULL DEFAULT '/v1/chat/completions',
    request_format      VARCHAR(64)     NOT NULL DEFAULT 'openai/chat_completions',
    requested_model     VARCHAR(128)    NOT NULL DEFAULT '',
    upstream_model      VARCHAR(128)    NOT NULL DEFAULT '',
    model_name          VARCHAR(128)    NOT NULL DEFAULT '',
    prompt_tokens       INT             NOT NULL DEFAULT 0,
    completion_tokens   INT             NOT NULL DEFAULT 0,
    total_tokens        INT             NOT NULL DEFAULT 0,
    cached_tokens       INT             NOT NULL DEFAULT 0,
    reasoning_tokens    INT             NOT NULL DEFAULT 0,
    quota               BIGINT          NOT NULL DEFAULT 0,
    cost_total          DECIMAL(20,10)  NOT NULL DEFAULT 0,
    price_reference     VARCHAR(128)    NOT NULL DEFAULT '',
    elapsed_time        INT             NOT NULL DEFAULT 0,
    first_token_time    INT             NOT NULL DEFAULT 0,
    is_stream           BOOLEAN         NOT NULL DEFAULT FALSE,
    request_id          VARCHAR(64)     NOT NULL DEFAULT '',
    dedupe_key          VARCHAR(128)    NOT NULL DEFAULT '',
    upstream_request_id VARCHAR(128)    NOT NULL DEFAULT '',
    status_code         INT             NOT NULL DEFAULT 0,
    client_ip           VARCHAR(64)     NOT NULL DEFAULT '',
    user_agent          VARCHAR(512)    NOT NULL DEFAULT '',
    content             TEXT            NOT NULL DEFAULT '',
    log_type            SMALLINT        NOT NULL DEFAULT 2,
    status              SMALLINT        NOT NULL DEFAULT 1,
    create_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_ai_log_user_id ON ai.log (user_id);
CREATE INDEX idx_ai_log_token_id ON ai.log (token_id);
CREATE INDEX idx_ai_log_project_id ON ai.log (project_id);
CREATE INDEX idx_ai_log_conversation_id ON ai.log (conversation_id);
CREATE INDEX idx_ai_log_message_id ON ai.log (message_id);
CREATE INDEX idx_ai_log_session_id ON ai.log (session_id);
CREATE INDEX idx_ai_log_thread_id ON ai.log (thread_id);
CREATE INDEX idx_ai_log_trace_id ON ai.log (trace_id);
CREATE INDEX idx_ai_log_channel_id ON ai.log (channel_id);
CREATE INDEX idx_ai_log_account_id ON ai.log (account_id);
CREATE INDEX idx_ai_log_requested_model ON ai.log (requested_model);
CREATE INDEX idx_ai_log_model_name ON ai.log (model_name);
CREATE INDEX idx_ai_log_create_time_type ON ai.log (create_time, log_type);
CREATE INDEX idx_ai_log_request_id ON ai.log (request_id);
CREATE UNIQUE INDEX uk_ai_log_dedupe_key
    ON ai.log (dedupe_key)
    WHERE dedupe_key <> '';

COMMENT ON TABLE ai.log IS 'AI 消费日志表（单次调用的账务/审计摘要）';
COMMENT ON COLUMN ai.log.id IS '日志ID';
COMMENT ON COLUMN ai.log.user_id IS '调用用户ID';
COMMENT ON COLUMN ai.log.token_id IS '使用的令牌ID';
COMMENT ON COLUMN ai.log.token_name IS '令牌名称冗余';
COMMENT ON COLUMN ai.log.project_id IS '所属项目ID（0 表示个人请求）';
COMMENT ON COLUMN ai.log.conversation_id IS '所属对话ID';
COMMENT ON COLUMN ai.log.message_id IS '所属消息ID';
COMMENT ON COLUMN ai.log.session_id IS '所属会话ID';
COMMENT ON COLUMN ai.log.thread_id IS '所属线程ID';
COMMENT ON COLUMN ai.log.trace_id IS '所属追踪ID';
COMMENT ON COLUMN ai.log.channel_id IS '实际命中的渠道ID';
COMMENT ON COLUMN ai.log.channel_name IS '渠道名称冗余';
COMMENT ON COLUMN ai.log.account_id IS '实际命中的账号ID（ai.channel_account.id）';
COMMENT ON COLUMN ai.log.account_name IS '账号名称冗余';
COMMENT ON COLUMN ai.log.execution_id IS '执行尝试ID（对应 ai.request_execution.id，未落库时为0）';
COMMENT ON COLUMN ai.log.endpoint IS '请求 endpoint';
COMMENT ON COLUMN ai.log.request_format IS '协议格式（如 openai/chat_completions）';
COMMENT ON COLUMN ai.log.requested_model IS '客户端请求模型名';
COMMENT ON COLUMN ai.log.upstream_model IS '实际转发给上游的模型名';
COMMENT ON COLUMN ai.log.model_name IS '标准化计费模型名';
COMMENT ON COLUMN ai.log.prompt_tokens IS '输入 Token 数';
COMMENT ON COLUMN ai.log.completion_tokens IS '输出 Token 数';
COMMENT ON COLUMN ai.log.total_tokens IS '总 Token 数';
COMMENT ON COLUMN ai.log.cached_tokens IS '缓存命中 Token 数';
COMMENT ON COLUMN ai.log.reasoning_tokens IS '推理 Token 数';
COMMENT ON COLUMN ai.log.quota IS '本次消耗配额';
COMMENT ON COLUMN ai.log.cost_total IS '按渠道采购价或成本口径计算的金额';
COMMENT ON COLUMN ai.log.price_reference IS '命中的 ai.channel_model_price_version.reference_id';
COMMENT ON COLUMN ai.log.elapsed_time IS '总耗时（毫秒）';
COMMENT ON COLUMN ai.log.first_token_time IS '首 token 延迟（毫秒）';
COMMENT ON COLUMN ai.log.is_stream IS '是否流式';
COMMENT ON COLUMN ai.log.request_id IS '请求唯一标识';
COMMENT ON COLUMN ai.log.dedupe_key IS '日志幂等键（用于避免重复写最终摘要）';
COMMENT ON COLUMN ai.log.upstream_request_id IS '上游返回的请求ID';
COMMENT ON COLUMN ai.log.status_code IS '最终状态码';
COMMENT ON COLUMN ai.log.client_ip IS '客户端 IP';
COMMENT ON COLUMN ai.log.user_agent IS '客户端 UA';
COMMENT ON COLUMN ai.log.content IS '备注/错误摘要';
COMMENT ON COLUMN ai.log.log_type IS '日志类型：1=充值 2=消费 3=管理操作 4=系统';
COMMENT ON COLUMN ai.log.status IS '调用状态：1=成功 2=失败 3=取消';
COMMENT ON COLUMN ai.log.create_time IS '记录时间';
