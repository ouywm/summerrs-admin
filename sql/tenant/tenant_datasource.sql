-- ============================================================
-- 租户数据源与隔离元数据表
-- 直接服务 summer-sharding 的租户元数据加载
-- ============================================================

CREATE SCHEMA IF NOT EXISTS tenant;

CREATE TABLE tenant.tenant_datasource (
    id              BIGSERIAL       PRIMARY KEY,
    tenant_id       VARCHAR(64)     NOT NULL,
    isolation_level SMALLINT        NOT NULL DEFAULT 1,
    status          VARCHAR(32)     NOT NULL DEFAULT 'active',
    schema_name     VARCHAR(128),
    datasource_name VARCHAR(128),
    db_uri          VARCHAR(1024),
    db_enable_logging BOOLEAN,
    db_min_conns    INT,
    db_max_conns    INT,
    db_connect_timeout_ms BIGINT,
    db_idle_timeout_ms BIGINT,
    db_acquire_timeout_ms BIGINT,
    db_test_before_acquire BOOLEAN,
    readonly_config JSONB           NOT NULL DEFAULT '{}'::jsonb,
    extra_config    JSONB           NOT NULL DEFAULT '{}'::jsonb,
    last_sync_time  TIMESTAMP,
    remark          VARCHAR(500)    NOT NULL DEFAULT '',
    create_by       VARCHAR(64)     NOT NULL DEFAULT '',
    create_time     TIMESTAMP       NOT NULL DEFAULT CURRENT_TIMESTAMP,
    update_by       VARCHAR(64)     NOT NULL DEFAULT '',
    update_time     TIMESTAMP       NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE UNIQUE INDEX uk_tenant_tenant_datasource_tenant_id ON tenant.tenant_datasource (tenant_id);
CREATE UNIQUE INDEX uk_tenant_tenant_datasource_datasource_name
    ON tenant.tenant_datasource (datasource_name)
    WHERE datasource_name IS NOT NULL AND datasource_name <> '';
CREATE UNIQUE INDEX uk_tenant_tenant_datasource_schema_name
    ON tenant.tenant_datasource (schema_name)
    WHERE schema_name IS NOT NULL AND schema_name <> '';
CREATE INDEX idx_tenant_tenant_datasource_status ON tenant.tenant_datasource (status);
CREATE INDEX idx_tenant_tenant_datasource_isolation_level ON tenant.tenant_datasource (isolation_level);
CREATE INDEX idx_tenant_tenant_datasource_last_sync_time ON tenant.tenant_datasource (last_sync_time);

COMMENT ON TABLE tenant.tenant_datasource IS '租户数据源与隔离元数据表';
COMMENT ON COLUMN tenant.tenant_datasource.id IS '主键ID';
COMMENT ON COLUMN tenant.tenant_datasource.tenant_id IS '租户业务唯一标识，对应 tenant.tenant.tenant_id';
COMMENT ON COLUMN tenant.tenant_datasource.isolation_level IS '隔离级别：1-共享行 2-独立表 3-独立Schema 4-独立库';
COMMENT ON COLUMN tenant.tenant_datasource.status IS '运行状态：active-启用 inactive-停用 provisioning-开通中 error-异常';
COMMENT ON COLUMN tenant.tenant_datasource.schema_name IS '独立 schema 名称（separate_schema 时使用）';
COMMENT ON COLUMN tenant.tenant_datasource.datasource_name IS '动态数据源名称（separate_database 时使用）';
COMMENT ON COLUMN tenant.tenant_datasource.db_uri IS '租户专属数据库连接串';
COMMENT ON COLUMN tenant.tenant_datasource.db_enable_logging IS '租户专属数据源 SQL 日志开关';
COMMENT ON COLUMN tenant.tenant_datasource.db_min_conns IS '租户专属数据源最小连接数';
COMMENT ON COLUMN tenant.tenant_datasource.db_max_conns IS '租户专属数据源最大连接数';
COMMENT ON COLUMN tenant.tenant_datasource.db_connect_timeout_ms IS '租户专属数据源连接超时（毫秒）';
COMMENT ON COLUMN tenant.tenant_datasource.db_idle_timeout_ms IS '租户专属数据源空闲超时（毫秒）';
COMMENT ON COLUMN tenant.tenant_datasource.db_acquire_timeout_ms IS '租户专属数据源获取连接超时（毫秒）';
COMMENT ON COLUMN tenant.tenant_datasource.db_test_before_acquire IS '租户专属数据源借出前连通性检测开关';
COMMENT ON COLUMN tenant.tenant_datasource.readonly_config IS '读写分离或只读副本配置（JSON）';
COMMENT ON COLUMN tenant.tenant_datasource.extra_config IS '运行时扩展配置（JSON）';
COMMENT ON COLUMN tenant.tenant_datasource.last_sync_time IS '最近一次元数据同步时间';
COMMENT ON COLUMN tenant.tenant_datasource.remark IS '备注';
COMMENT ON COLUMN tenant.tenant_datasource.create_by IS '创建人';
COMMENT ON COLUMN tenant.tenant_datasource.create_time IS '创建时间';
COMMENT ON COLUMN tenant.tenant_datasource.update_by IS '更新人';
COMMENT ON COLUMN tenant.tenant_datasource.update_time IS '更新时间';
