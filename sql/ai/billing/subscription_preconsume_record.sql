-- ============================================================
-- AI 订阅预扣记录表
-- 参考 new-api SubscriptionPreConsumeRecord / 订阅额度预留与结算
-- ============================================================

CREATE SCHEMA IF NOT EXISTS ai;

CREATE TABLE ai.subscription_preconsume_record (
    id                  BIGSERIAL       PRIMARY KEY,
    user_subscription_id BIGINT         NOT NULL DEFAULT 0,
    request_id          VARCHAR(64)     NOT NULL DEFAULT '',
    task_id             BIGINT          NOT NULL DEFAULT 0,
    status              SMALLINT        NOT NULL DEFAULT 1,
    reserved_quota      BIGINT          NOT NULL DEFAULT 0,
    settled_quota       BIGINT          NOT NULL DEFAULT 0,
    reserved_amount     DECIMAL(20,8)   NOT NULL DEFAULT 0,
    settled_amount      DECIMAL(20,8)   NOT NULL DEFAULT 0,
    expire_time         TIMESTAMPTZ,
    settle_time         TIMESTAMPTZ,
    metadata            JSONB           NOT NULL DEFAULT '{}'::jsonb,
    create_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW(),
    update_time         TIMESTAMPTZ     NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_ai_subscription_preconsume_subscription_id ON ai.subscription_preconsume_record (user_subscription_id);
CREATE INDEX idx_ai_subscription_preconsume_request_id ON ai.subscription_preconsume_record (request_id);
CREATE INDEX idx_ai_subscription_preconsume_status ON ai.subscription_preconsume_record (status);
CREATE INDEX idx_ai_subscription_preconsume_expire_time ON ai.subscription_preconsume_record (expire_time);

COMMENT ON TABLE ai.subscription_preconsume_record IS 'AI 订阅预扣记录表';
COMMENT ON COLUMN ai.subscription_preconsume_record.id IS '预扣记录ID';
COMMENT ON COLUMN ai.subscription_preconsume_record.user_subscription_id IS '用户订阅ID';
COMMENT ON COLUMN ai.subscription_preconsume_record.request_id IS '关联请求ID';
COMMENT ON COLUMN ai.subscription_preconsume_record.task_id IS '关联任务ID';
COMMENT ON COLUMN ai.subscription_preconsume_record.status IS '状态：1=预扣中 2=已结算 3=已释放 4=过期';
COMMENT ON COLUMN ai.subscription_preconsume_record.reserved_quota IS '预留额度';
COMMENT ON COLUMN ai.subscription_preconsume_record.settled_quota IS '最终结算额度';
COMMENT ON COLUMN ai.subscription_preconsume_record.reserved_amount IS '预留金额';
COMMENT ON COLUMN ai.subscription_preconsume_record.settled_amount IS '结算金额';
COMMENT ON COLUMN ai.subscription_preconsume_record.expire_time IS '预扣失效时间';
COMMENT ON COLUMN ai.subscription_preconsume_record.settle_time IS '结算时间';
COMMENT ON COLUMN ai.subscription_preconsume_record.metadata IS '扩展信息（JSON）';
COMMENT ON COLUMN ai.subscription_preconsume_record.create_time IS '创建时间';
COMMENT ON COLUMN ai.subscription_preconsume_record.update_time IS '更新时间';
