# SYS Schema SQL

更新时间：2026-03-21

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
- AI 代码生成：`sys.ai_codegen_session`、`sys.ai_codegen_message`、`sys.ai_codegen_task`、`sys.ai_codegen_version`

增量脚本：

- `ai_codegen_incremental.sql`：给已有库新增 AI 代码生成表、菜单按钮权限、资源清单、ActionResource 映射，并默认授权给 `R_SUPER` / `R_ADMIN`。
