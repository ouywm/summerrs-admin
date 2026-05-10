CREATE SCHEMA IF NOT EXISTS sys;

-- ------------------------------------------------------------
-- 任务定义
-- ------------------------------------------------------------
CREATE TABLE sys.job (
    id              BIGSERIAL     PRIMARY KEY,
    tenant_id       BIGINT,
    name            VARCHAR(128)  NOT NULL,
    group_name      VARCHAR(64)   NOT NULL DEFAULT 'default',
    description     TEXT          NOT NULL DEFAULT '',
    handler         VARCHAR(128)  NOT NULL,
    schedule_type   VARCHAR(16)   NOT NULL,
    cron_expr       VARCHAR(64),
    interval_ms     BIGINT,
    fire_time       TIMESTAMP,
    params_json     JSONB         NOT NULL DEFAULT '{}',
    enabled         BOOLEAN       NOT NULL DEFAULT TRUE,
    timeout_ms      BIGINT        NOT NULL DEFAULT 0,
    retry_max       INT           NOT NULL DEFAULT 0,
    version         BIGINT        NOT NULL DEFAULT 0,
    created_by      BIGINT,
    create_time     TIMESTAMP     NOT NULL DEFAULT CURRENT_TIMESTAMP,
    update_time     TIMESTAMP     NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- 同租户内任务名唯一；tenant_id 为 NULL 时按 0 落地以保留全局唯一性
CREATE UNIQUE INDEX uk_sys_job_tenant_name ON sys.job (COALESCE(tenant_id, 0), name);
CREATE INDEX idx_sys_job_enabled ON sys.job (enabled) WHERE enabled = TRUE;
CREATE INDEX idx_sys_job_handler ON sys.job (handler);

COMMENT ON TABLE sys.job IS '动态任务定义';
COMMENT ON COLUMN sys.job.id IS '任务ID';
COMMENT ON COLUMN sys.job.tenant_id IS '租户ID（NULL=全局任务）';
COMMENT ON COLUMN sys.job.name IS '任务名称';
COMMENT ON COLUMN sys.job.group_name IS '任务分组';
COMMENT ON COLUMN sys.job.handler IS 'handler 名称（registry key）';
COMMENT ON COLUMN sys.job.schedule_type IS '调度类型：CRON / FIXED_RATE / ONESHOT';
COMMENT ON COLUMN sys.job.cron_expr IS 'cron 表达式（schedule_type=CRON）';
COMMENT ON COLUMN sys.job.interval_ms IS '固定间隔毫秒（schedule_type=FIXED_RATE）';
COMMENT ON COLUMN sys.job.fire_time IS '一次性触发时间（schedule_type=ONESHOT）';
COMMENT ON COLUMN sys.job.params_json IS 'handler 参数（任意 JSON）';
COMMENT ON COLUMN sys.job.enabled IS '是否启用';
COMMENT ON COLUMN sys.job.timeout_ms IS '执行超时毫秒（0=不限）';
COMMENT ON COLUMN sys.job.retry_max IS '最大重试次数（失败后按指数退避）';
COMMENT ON COLUMN sys.job.version IS '乐观锁版本号';

-- ------------------------------------------------------------
-- 执行记录（任务每次触发的状态机）
-- ------------------------------------------------------------
CREATE TABLE sys.job_run (
    id            BIGSERIAL     PRIMARY KEY,
    job_id        BIGINT        NOT NULL,
    trace_id      VARCHAR(64)   NOT NULL,
    trigger_type  VARCHAR(16)   NOT NULL,
    trigger_by    BIGINT,
    state         VARCHAR(16)   NOT NULL,
    scheduled_at  TIMESTAMP     NOT NULL,
    started_at    TIMESTAMP,
    finished_at   TIMESTAMP,
    retry_count   INT           NOT NULL DEFAULT 0,
    result_json   JSONB,
    error_message TEXT,
    create_time   TIMESTAMP     NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_sys_job_run_job_scheduled ON sys.job_run (job_id, scheduled_at DESC);
CREATE INDEX idx_sys_job_run_state_running ON sys.job_run (state) WHERE state = 'RUNNING';
CREATE INDEX idx_sys_job_run_trace ON sys.job_run (trace_id);

COMMENT ON TABLE sys.job_run IS '任务执行记录';
COMMENT ON COLUMN sys.job_run.job_id IS '所属任务ID';
COMMENT ON COLUMN sys.job_run.trace_id IS '链路追踪ID';
COMMENT ON COLUMN sys.job_run.trigger_type IS '触发来源：CRON / MANUAL / RETRY';
COMMENT ON COLUMN sys.job_run.trigger_by IS '手动触发的用户ID';
COMMENT ON COLUMN sys.job_run.state IS '状态：RUNNING / SUCCEEDED / FAILED / TIMEOUT / DISCARDED';
COMMENT ON COLUMN sys.job_run.scheduled_at IS '计划触发时间';
COMMENT ON COLUMN sys.job_run.started_at IS '实际开始执行时间';
COMMENT ON COLUMN sys.job_run.finished_at IS '执行结束时间';
COMMENT ON COLUMN sys.job_run.retry_count IS '当前重试次数';
COMMENT ON COLUMN sys.job_run.result_json IS '返回值（handler 成功返回的 JSON）';
COMMENT ON COLUMN sys.job_run.error_message IS '错误信息';