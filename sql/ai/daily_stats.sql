-- ============================================================
-- AI 日度统计表
-- 由定时任务从 ai.log 聚合生成，避免后台实时扫明细
-- ============================================================

CREATE SCHEMA IF NOT EXISTS ai;

CREATE TABLE ai.daily_stats (
    id                  BIGSERIAL       PRIMARY KEY,
    stats_date          DATE            NOT NULL,
    user_id             BIGINT          NOT NULL DEFAULT 0,
    project_id          BIGINT          NOT NULL DEFAULT 0,
    channel_id          BIGINT          NOT NULL DEFAULT 0,
    account_id          BIGINT          NOT NULL DEFAULT 0,
    model_name          VARCHAR(128)    NOT NULL DEFAULT '',
    request_count       BIGINT          NOT NULL DEFAULT 0,
    success_count       BIGINT          NOT NULL DEFAULT 0,
    fail_count          BIGINT          NOT NULL DEFAULT 0,
    prompt_tokens       BIGINT          NOT NULL DEFAULT 0,
    completion_tokens   BIGINT          NOT NULL DEFAULT 0,
    total_tokens        BIGINT          NOT NULL DEFAULT 0,
    cached_tokens       BIGINT          NOT NULL DEFAULT 0,
    reasoning_tokens    BIGINT          NOT NULL DEFAULT 0,
    quota               BIGINT          NOT NULL DEFAULT 0,
    cost_total          DECIMAL(20,10)  NOT NULL DEFAULT 0,
    avg_elapsed_time    INT             NOT NULL DEFAULT 0,
    avg_first_token_time INT            NOT NULL DEFAULT 0,
    create_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX uk_ai_daily_stats_date_user_channel_account_model
    ON ai.daily_stats (stats_date, user_id, project_id, channel_id, account_id, model_name);
CREATE INDEX idx_ai_daily_stats_date ON ai.daily_stats (stats_date);
CREATE INDEX idx_ai_daily_stats_user_id ON ai.daily_stats (user_id);
CREATE INDEX idx_ai_daily_stats_project_id ON ai.daily_stats (project_id);
CREATE INDEX idx_ai_daily_stats_channel_id ON ai.daily_stats (channel_id);
CREATE INDEX idx_ai_daily_stats_account_id ON ai.daily_stats (account_id);
CREATE INDEX idx_ai_daily_stats_model_name ON ai.daily_stats (model_name);

COMMENT ON TABLE ai.daily_stats IS 'AI 日度统计表（按天聚合的消费与性能数据）';
COMMENT ON COLUMN ai.daily_stats.id IS '统计ID';
COMMENT ON COLUMN ai.daily_stats.stats_date IS '统计日期';
COMMENT ON COLUMN ai.daily_stats.user_id IS '用户ID（0=全局汇总）';
COMMENT ON COLUMN ai.daily_stats.project_id IS '项目ID（0=全局汇总）';
COMMENT ON COLUMN ai.daily_stats.channel_id IS '渠道ID（0=全局汇总）';
COMMENT ON COLUMN ai.daily_stats.account_id IS '账号ID（0=全局汇总）';
COMMENT ON COLUMN ai.daily_stats.model_name IS '标准化模型名（空字符串=全局汇总）';
COMMENT ON COLUMN ai.daily_stats.request_count IS '请求总数';
COMMENT ON COLUMN ai.daily_stats.success_count IS '成功次数';
COMMENT ON COLUMN ai.daily_stats.fail_count IS '失败次数';
COMMENT ON COLUMN ai.daily_stats.prompt_tokens IS '输入 Token 总数';
COMMENT ON COLUMN ai.daily_stats.completion_tokens IS '输出 Token 总数';
COMMENT ON COLUMN ai.daily_stats.total_tokens IS '总 Token 数';
COMMENT ON COLUMN ai.daily_stats.cached_tokens IS '缓存命中 Token 总数';
COMMENT ON COLUMN ai.daily_stats.reasoning_tokens IS '推理 Token 总数';
COMMENT ON COLUMN ai.daily_stats.quota IS '消耗配额总计';
COMMENT ON COLUMN ai.daily_stats.cost_total IS '成本金额总计';
COMMENT ON COLUMN ai.daily_stats.avg_elapsed_time IS '平均总耗时（毫秒）';
COMMENT ON COLUMN ai.daily_stats.avg_first_token_time IS '平均首 token 时间（毫秒）';
COMMENT ON COLUMN ai.daily_stats.create_time IS '记录创建时间';
