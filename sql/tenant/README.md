# Tenant Schema SQL

更新时间：2026-04-15

这里是租户域（tenant）控制面相关 SQL，主要覆盖租户主表、租户数据源/隔离元数据、租户成员关系等。

说明：

- 本目录对应物理 `tenant` schema（与 `sys` 系统域隔离）。
- 租户域的“用户/角色/权限”体系未来会单独建表，不与 `sys."user"` / `sys."role"` 绑定。

当前主要表：

- 租户主表：`tenant.tenant`
- 租户数据源与隔离元数据：`tenant.tenant_datasource`
- 租户成员（租户域用户体系）：`tenant.tenant_membership`

