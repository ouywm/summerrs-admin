# SYS Schema SQL

更新时间：2026-06-13

这里是系统域 SQL，主要覆盖后台账号、权限、菜单、系统配置、系统日志，以及账号认证层。

命名规则：

- 文件名也去掉重复域前缀，例如 `user.sql`、`role.sql`、`user_role.sql`
- 物理表统一落在 `sys` schema，且不再重复 `sys_` 前缀
- 保留字表名按 PostgreSQL 规则加引号：`sys."user"`、`sys."role"`

当前主要表：

- 账号与权限：`sys."user"`、`sys."role"`、`sys.user_role`、`sys.menu`、`sys.role_menu`、`sys.resource`、`sys.action_resource`
- 配置与字典：`sys.config_group`、`sys.config`、`sys.dict_type`、`sys.dict_data`
- 日志与文件：`sys.login_log`、`sys.operation_log`、`sys.notice`、`sys.notice_target`、`sys.notice_user`、`sys.file`
- 认证层：`sys.verification_token`、`sys.two_factor`、`sys.two_factor_backup_code`、`sys.custom_oauth_provider`、`sys.user_oauth_binding`、`sys.passkey_credential`

数据约定：

- `*.sql` 表结构文件只保留 DDL、索引和注释，不写入业务或初始化数据
- 基础角色、用户、字典、菜单、资源和权限绑定数据集中放在 `seed/system_data.sql`
- `menu_data/` 与 `menu_data_all.sql` 是菜单种子数据资产，不作为表结构 DDL 使用
