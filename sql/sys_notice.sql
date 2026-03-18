-- ============================================================
-- 系统公告表设计说明
-- ============================================================
--
-- 采用“公告定义 + 目标范围 + 用户接收状态”三表方案：
-- 1. sys_notice        ：公告主表，保存公告内容、发布状态、范围类型
-- 2. sys_notice_target ：公告目标表，保存角色/指定用户范围
-- 3. sys_notice_user   ：公告用户表，发布时展开到具体用户，负责未读/已读查询
--
-- 这样设计的好处：
-- - 草稿阶段可以先保存目标范围
-- - 发布后未读数、未读列表查询非常简单
-- - 公告实时推送和数据库最终状态可以解耦
-- - 不依赖数据库外键，保持当前项目风格

-- ============================================================
-- 系统公告主表
-- ============================================================

CREATE TABLE sys_notice (
    id             BIGSERIAL      PRIMARY KEY,
    notice_title   VARCHAR(200)   NOT NULL,
    notice_content TEXT           NOT NULL DEFAULT '',
    notice_level   SMALLINT       NOT NULL DEFAULT 1,
    notice_scope   SMALLINT       NOT NULL DEFAULT 1,
    publish_status SMALLINT       NOT NULL DEFAULT 1,
    pinned         BOOLEAN        NOT NULL DEFAULT FALSE,
    enabled        BOOLEAN        NOT NULL DEFAULT TRUE,
    sort           INT            NOT NULL DEFAULT 0,
    publish_by     VARCHAR(64)    NOT NULL DEFAULT '',
    publish_time   TIMESTAMP,
    expire_time    TIMESTAMP,
    remark         VARCHAR(500)   NOT NULL DEFAULT '',
    create_by      VARCHAR(64)    NOT NULL DEFAULT '',
    create_time    TIMESTAMP      NOT NULL DEFAULT CURRENT_TIMESTAMP,
    update_by      VARCHAR(64)    NOT NULL DEFAULT '',
    update_time    TIMESTAMP      NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX idx_sys_notice_status_time ON sys_notice (publish_status, enabled, publish_time DESC);
CREATE INDEX idx_sys_notice_scope ON sys_notice (notice_scope);
CREATE INDEX idx_sys_notice_pinned_sort ON sys_notice (pinned, sort, id);

COMMENT ON TABLE sys_notice IS '系统公告表';
COMMENT ON COLUMN sys_notice.id IS '公告ID';
COMMENT ON COLUMN sys_notice.notice_title IS '公告标题';
COMMENT ON COLUMN sys_notice.notice_content IS '公告正文内容';
COMMENT ON COLUMN sys_notice.notice_level IS '公告级别：1=普通 2=成功 3=警告 4=危险';
COMMENT ON COLUMN sys_notice.notice_scope IS '公告范围：1=全体后台用户 2=指定角色 3=指定用户';
COMMENT ON COLUMN sys_notice.publish_status IS '发布状态：1=草稿 2=已发布 3=已撤回';
COMMENT ON COLUMN sys_notice.pinned IS '是否置顶';
COMMENT ON COLUMN sys_notice.enabled IS '是否启用';
COMMENT ON COLUMN sys_notice.sort IS '排序，值越小越靠前';
COMMENT ON COLUMN sys_notice.publish_by IS '发布人';
COMMENT ON COLUMN sys_notice.publish_time IS '发布时间';
COMMENT ON COLUMN sys_notice.expire_time IS '过期时间，为空表示不过期';
COMMENT ON COLUMN sys_notice.remark IS '备注';
COMMENT ON COLUMN sys_notice.create_by IS '创建人';
COMMENT ON COLUMN sys_notice.create_time IS '创建时间';
COMMENT ON COLUMN sys_notice.update_by IS '更新人';
COMMENT ON COLUMN sys_notice.update_time IS '更新时间';

-- ============================================================
-- 系统公告目标表
-- ============================================================
--
-- notice_scope = 1（全体后台用户）时，不需要保存目标行
-- notice_scope = 2（指定角色）时，target_type 固定为 1，target_id 对应 sys_role.id
-- notice_scope = 3（指定用户）时，target_type 固定为 2，target_id 对应 sys_user.id

CREATE TABLE sys_notice_target (
    id          BIGSERIAL   PRIMARY KEY,
    notice_id   BIGINT      NOT NULL,
    target_type SMALLINT    NOT NULL,
    target_id   BIGINT      NOT NULL,
    create_time TIMESTAMP   NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE UNIQUE INDEX uk_sys_notice_target_notice_target ON sys_notice_target (notice_id, target_type, target_id);
CREATE INDEX idx_sys_notice_target_target ON sys_notice_target (target_type, target_id);

COMMENT ON TABLE sys_notice_target IS '系统公告目标表';
COMMENT ON COLUMN sys_notice_target.id IS '主键ID';
COMMENT ON COLUMN sys_notice_target.notice_id IS '公告ID，逻辑关联 sys_notice.id';
COMMENT ON COLUMN sys_notice_target.target_type IS '目标类型：1=角色 2=用户';
COMMENT ON COLUMN sys_notice_target.target_id IS '目标ID；target_type=1 时对应 sys_role.id，target_type=2 时对应 sys_user.id';
COMMENT ON COLUMN sys_notice_target.create_time IS '创建时间';

-- ============================================================
-- 系统公告用户接收表
-- ============================================================
--
-- 公告正式发布时，将可见范围展开为具体用户，写入本表。
-- 之后未读数、未读列表、已读状态全部基于本表处理。

CREATE TABLE sys_notice_user (
    id          BIGSERIAL   PRIMARY KEY,
    notice_id   BIGINT      NOT NULL,
    user_id     BIGINT      NOT NULL,
    read_flag   BOOLEAN     NOT NULL DEFAULT FALSE,
    read_time   TIMESTAMP,
    create_time TIMESTAMP   NOT NULL DEFAULT CURRENT_TIMESTAMP,
    update_time TIMESTAMP   NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE UNIQUE INDEX uk_sys_notice_user_notice_user ON sys_notice_user (notice_id, user_id);
CREATE INDEX idx_sys_notice_user_user_read ON sys_notice_user (user_id, read_flag, create_time DESC);
CREATE INDEX idx_sys_notice_user_notice_id ON sys_notice_user (notice_id);

COMMENT ON TABLE sys_notice_user IS '系统公告用户接收表';
COMMENT ON COLUMN sys_notice_user.id IS '主键ID';
COMMENT ON COLUMN sys_notice_user.notice_id IS '公告ID，逻辑关联 sys_notice.id';
COMMENT ON COLUMN sys_notice_user.user_id IS '接收用户ID，逻辑关联 sys_user.id';
COMMENT ON COLUMN sys_notice_user.read_flag IS '是否已读';
COMMENT ON COLUMN sys_notice_user.read_time IS '已读时间';
COMMENT ON COLUMN sys_notice_user.create_time IS '接收时间';
COMMENT ON COLUMN sys_notice_user.update_time IS '更新时间';
