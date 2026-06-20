<div align="center">

<img src="docs/static/logo.png" alt="Summerrs Admin Logo" width="200"/>

# Summerrs Admin

**中文** | [English](README.md)

> Rust 后台管理系统

[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)


</div>

---

## 项目定位

`summerrs-admin` 是一套**完全用 Rust 写**的生产级后台管理系统，基于 Summer 生态组合 Web、SeaORM、Redis、定时任务、对象存储、MCP 等能力，提供后台系统常见的权限、菜单、用户、文件、日志、通知与运维基础能力。

---

## 核心能力

- **系统管理与 RBAC**：内置用户、角色、菜单、按钮、资源、字典、配置、公告、日志、在线用户、监控等后台管理模块。

---


## 项目结构

```
summerrs-admin/
├── crates/
│   ├── app/                          # 二进制入口，组装所有插件
│   ├── summer-admin-macros/          # 宏
│   ├── summer-auth/                  # 授权
│   ├── summer-ai/                    # AI 能力抽象与模型接入
│   ├── summer-common/                # 通用类型与工具
│   ├── summer-domain/                # 领域模型（实体 / VO）
│   ├── summer-mcp/                   # MCP 服务器
│   ├── summer-migration/             # SeaORM 迁移与种子数据
│   ├── summer-plugins/               # summer-rs 插件
│   └── summer-system/                # 系统业务
│       └── model/
├── config/                           # 多环境配置（dev / prod / test）
├── sql/                              # 数据库 source of truth
│   └── sys/                          # 系统域（DDL / 菜单 / 权限 / 种子数据）
├── doc/                              # 部署 / 迁移 / 技术指南
├── docs/                             # 调研、研究、参考资料
├── locales/                          # i18n 资源
├── build-tools/                      # fmt / clippy / pre-commit 脚本
├── docker-compose.yml                # 一键启动 postgres + redis + rustfs + app
└── Dockerfile                        # 多阶段构建
```

---

## 前端与文档

### 前端项目
基于 Ant Design Pro 的前端实现：
- **仓库地址**：[art-design-pro](https://github.com/ouywm/art-design-pro)


### 文档站
文档：
[https://ouywm.github.io/summer-admin-site/](https://ouywm.github.io/summer-admin-site/)

---

<div align="center">

如果这个项目对你有帮助，欢迎 Star 支持。

[报告问题](https://github.com/ouywm/summerrs-admin/issues) · [发起讨论](https://github.com/ouywm/summerrs-admin/discussions)

</div>
