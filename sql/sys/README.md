# SYS Schema SQL

更新时间：2026-03-21

这里是系统域 SQL，主要覆盖后台账号、权限、菜单、系统配置、系统日志，以及账号认证层。

命名规则：

- 文件名也去掉重复域前缀，例如 `user.sql`、`role.sql`、`user_role.sql`
- 物理表统一落在 `sys` schema，且不再重复 `sys_` 前缀
- 保留字表名按 PostgreSQL 规则加引号：`sys."user"`、`sys."role"`

当前主要表：

- 账号与权限：`sys."user"`、`sys."role"`、`sys.user_role`、`sys.menu`、`sys.role_menu`
- 配置与字典：`sys.config_group`、`sys.config`、`sys.dict_type`、`sys.dict_data`
- 日志与文件：`sys.login_log`、`sys.operation_log`、`sys.notice`、`sys.notice_target`、`sys.notice_user`、`sys.file`
- 认证层：`sys.verification_token`、`sys.two_factor`、`sys.two_factor_backup_code`、`sys.custom_oauth_provider`、`sys.user_oauth_binding`、`sys.passkey_credential`

说明：

- 认证层归 `sys`，不归 `ai`
- 菜单种子数据统一写入 `sys.menu`
- 老库迁移时，需要把原来的 `public.sys_*` 表移动到 `sys` schema 并去掉重复前缀
