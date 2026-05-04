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
    script          TEXT,
    script_engine   VARCHAR(16),
    enabled         BOOLEAN       NOT NULL DEFAULT TRUE,
    blocking        VARCHAR(16)   NOT NULL DEFAULT 'SERIAL',
    misfire         VARCHAR(16)   NOT NULL DEFAULT 'FIRE_NOW',
    timeout_ms      BIGINT        NOT NULL DEFAULT 0,
    retry_max       INT           NOT NULL DEFAULT 0,
    retry_backoff   VARCHAR(16)   NOT NULL DEFAULT 'EXPONENTIAL',
    unique_key      VARCHAR(128),
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
COMMENT ON COLUMN sys.job.handler IS 'handler 名称（registry key 或 rhai/lua 脚本引擎）';
COMMENT ON COLUMN sys.job.schedule_type IS '调度类型：CRON / FIXED_RATE / FIXED_DELAY / ONESHOT';
COMMENT ON COLUMN sys.job.cron_expr IS 'cron 表达式（schedule_type=CRON）';
COMMENT ON COLUMN sys.job.interval_ms IS '固定间隔毫秒（schedule_type=FIXED_*）';
COMMENT ON COLUMN sys.job.fire_time IS '一次性触发时间（schedule_type=ONESHOT）';
COMMENT ON COLUMN sys.job.params_json IS 'handler 参数（任意 JSON）';
COMMENT ON COLUMN sys.job.script IS '脚本任务源码（handler=rhai/lua 时使用）';
COMMENT ON COLUMN sys.job.script_engine IS '脚本引擎：rhai / lua';
COMMENT ON COLUMN sys.job.enabled IS '是否启用';
COMMENT ON COLUMN sys.job.blocking IS '阻塞策略：SERIAL / DISCARD / OVERRIDE';
COMMENT ON COLUMN sys.job.misfire IS '错过触发策略：FIRE_NOW / IGNORE / RESCHEDULE';
COMMENT ON COLUMN sys.job.timeout_ms IS '执行超时毫秒（0=不限）';
COMMENT ON COLUMN sys.job.retry_max IS '最大重试次数';
COMMENT ON COLUMN sys.job.retry_backoff IS '退避策略：EXPONENTIAL / LINEAR / FIXED';
COMMENT ON COLUMN sys.job.unique_key IS '幂等键（按参数 hash 去重）';
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
    instance      VARCHAR(64),
    scheduled_at  TIMESTAMP     NOT NULL,
    started_at    TIMESTAMP,
    finished_at   TIMESTAMP,
    retry_count   INT           NOT NULL DEFAULT 0,
    result_json   JSONB,
    error_message TEXT,
    log_excerpt   TEXT,
    unique_key    VARCHAR(128),
    create_time   TIMESTAMP     NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_sys_job_run_job_scheduled ON sys.job_run (job_id, scheduled_at DESC);
CREATE INDEX idx_sys_job_run_state_pending ON sys.job_run (state) WHERE state IN ('ENQUEUED', 'RUNNING');
CREATE INDEX idx_sys_job_run_trace ON sys.job_run (trace_id);
-- Unique 去重的并发兜底：同一 job 同一 unique_key 在 ENQUEUED/RUNNING 时唯一
CREATE UNIQUE INDEX idx_sys_job_run_unique_active
    ON sys.job_run (job_id, unique_key)
    WHERE unique_key IS NOT NULL AND state IN ('ENQUEUED', 'RUNNING');

COMMENT ON TABLE sys.job_run IS '任务执行记录';
COMMENT ON COLUMN sys.job_run.job_id IS '所属任务ID';
COMMENT ON COLUMN sys.job_run.trace_id IS '链路追踪ID';
COMMENT ON COLUMN sys.job_run.trigger_type IS '触发来源：CRON / MANUAL / RETRY / WORKFLOW / API / MISFIRE';
COMMENT ON COLUMN sys.job_run.trigger_by IS '手动触发的用户ID';
COMMENT ON COLUMN sys.job_run.state IS '状态：ENQUEUED / RUNNING / SUCCEEDED / FAILED / TIMEOUT / CANCELED / DISCARDED';
COMMENT ON COLUMN sys.job_run.instance IS '执行实例标识 hostname:pid';
COMMENT ON COLUMN sys.job_run.scheduled_at IS '计划触发时间';
COMMENT ON COLUMN sys.job_run.started_at IS '实际开始执行时间';
COMMENT ON COLUMN sys.job_run.finished_at IS '执行结束时间';
COMMENT ON COLUMN sys.job_run.retry_count IS '当前重试次数';
COMMENT ON COLUMN sys.job_run.result_json IS '返回值（handler 成功返回的 JSON）';
COMMENT ON COLUMN sys.job_run.error_message IS '错误信息';
COMMENT ON COLUMN sys.job_run.log_excerpt IS '日志摘录';
COMMENT ON COLUMN sys.job_run.unique_key IS 'Unique 去重值（worker 按 sys.job.unique_key 维度计算 sha256 后写入）';


-- ------------------------------------------------------------
-- 任务依赖（A 跑完成功 → 自动触发 B）
-- ------------------------------------------------------------
CREATE TABLE sys.job_dependency (
    id              BIGSERIAL     PRIMARY KEY,
    upstream_id     BIGINT        NOT NULL,
    downstream_id   BIGINT        NOT NULL,
    on_state        VARCHAR(16)   NOT NULL DEFAULT 'SUCCEEDED',
    enabled         BOOLEAN       NOT NULL DEFAULT TRUE,
    create_time     TIMESTAMP     NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE UNIQUE INDEX uk_sys_job_dep_pair ON sys.job_dependency (upstream_id, downstream_id);
CREATE INDEX idx_sys_job_dep_upstream ON sys.job_dependency (upstream_id) WHERE enabled = TRUE;
CREATE INDEX idx_sys_job_dep_downstream ON sys.job_dependency (downstream_id);

COMMENT ON TABLE sys.job_dependency IS '任务依赖关系（upstream 跑完后按 on_state 触发 downstream）';
COMMENT ON COLUMN sys.job_dependency.upstream_id IS '上游任务ID';
COMMENT ON COLUMN sys.job_dependency.downstream_id IS '下游任务ID';
COMMENT ON COLUMN sys.job_dependency.on_state IS '触发条件：SUCCEEDED / FAILED / ALWAYS';
COMMENT ON COLUMN sys.job_dependency.enabled IS '是否启用（软禁用，保留配置）';

