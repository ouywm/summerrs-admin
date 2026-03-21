-- ============================================================
-- 系统二次认证恢复码表
-- ============================================================

CREATE SCHEMA IF NOT EXISTS sys;

CREATE TABLE sys.two_factor_backup_code (
    id               BIGSERIAL       PRIMARY KEY,
    factor_id        BIGINT          NOT NULL REFERENCES sys.two_factor(id) ON DELETE CASCADE,
    user_id          BIGINT          NOT NULL REFERENCES sys."user"(id) ON DELETE CASCADE,
    code_hash        VARCHAR(128)    NOT NULL,
    status           SMALLINT        NOT NULL DEFAULT 1,
    used_time        TIMESTAMP,
    expire_time      TIMESTAMP,
    create_time      TIMESTAMP       NOT NULL DEFAULT CURRENT_TIMESTAMP,
    update_time      TIMESTAMP       NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE UNIQUE INDEX uk_sys_two_factor_backup_code_hash ON sys.two_factor_backup_code (code_hash);
CREATE INDEX idx_sys_two_factor_backup_code_factor_id ON sys.two_factor_backup_code (factor_id);
CREATE INDEX idx_sys_two_factor_backup_code_user_status ON sys.two_factor_backup_code (user_id, status);

COMMENT ON TABLE sys.two_factor_backup_code IS '系统二次认证恢复码表';
COMMENT ON COLUMN sys.two_factor_backup_code.id IS '主键ID';
COMMENT ON COLUMN sys.two_factor_backup_code.factor_id IS '关联二次认证因子ID';
COMMENT ON COLUMN sys.two_factor_backup_code.user_id IS '用户ID';
COMMENT ON COLUMN sys.two_factor_backup_code.code_hash IS '恢复码哈希';
COMMENT ON COLUMN sys.two_factor_backup_code.status IS '状态：1=可用 2=已使用 3=已作废 4=已过期';
COMMENT ON COLUMN sys.two_factor_backup_code.used_time IS '使用时间';
COMMENT ON COLUMN sys.two_factor_backup_code.expire_time IS '过期时间';
COMMENT ON COLUMN sys.two_factor_backup_code.create_time IS '创建时间';
COMMENT ON COLUMN sys.two_factor_backup_code.update_time IS '更新时间';
