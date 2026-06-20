<div align="center">

<img src="docs/static/logo.png" alt="Summerrs Admin Logo" width="200"/>

# Summerrs Admin

[中文](README.zh.md) | **English**

> Rust admin system

[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)


</div>

---

## What It Is

`summerrs-admin` is a production-grade admin system **written entirely in Rust**. It combines Web, SeaORM, Redis, cron jobs, object storage, MCP, and other capabilities from the Summer ecosystem to provide common backend admin features such as permissions, menus, users, files, logs, notifications, and operational tooling.

---

## Core Capabilities

- **System Management and RBAC**: built-in backend admin modules for users, roles, menus, buttons, resources, dictionaries, configuration, announcements, logs, online users, monitoring, and more.

---


## Project Structure

```
summerrs-admin/
├── crates/
│   ├── app/                          # binary entry, assembles all plugins
│   ├── summer-admin-macros/          # macros
│   ├── summer-auth/                  # authorization
│   ├── summer-ai/                    # AI capability abstraction and model integration
│   ├── summer-common/                # shared types and utilities
│   ├── summer-domain/                # domain models (entities / VOs)
│   ├── summer-mcp/                   # MCP server
│   ├── summer-migration/             # SeaORM migrations and seed data
│   ├── summer-plugins/               # summer-rs plugins
│   └── summer-system/                # system business
│       └── model/
├── config/                           # multi-environment configs (dev / prod / test)
├── sql/                              # database source of truth
│   └── sys/                          # system domain (DDL / menus / permissions / seed data)
├── doc/                              # deployment / migration / technical guides
├── docs/                             # research, surveys, reference materials
├── locales/                          # i18n resources
├── build-tools/                      # fmt / clippy / pre-commit scripts
├── docker-compose.yml                # one-shot stack: postgres + redis + rustfs + app
└── Dockerfile                        # multi-stage build
```

---

## Frontend & Docs

### Frontend Project
Frontend implementation based on Ant Design Pro:
- **Repository**: [art-design-pro](https://github.com/ouywm/art-design-pro)


### Documentation Site
Docs:
[https://ouywm.github.io/summer-admin-site/](https://ouywm.github.io/summer-admin-site/)

---

<div align="center">

If this project helps you, a Star is appreciated.

[Report an issue](https://github.com/ouywm/summerrs-admin/issues) · [Start a discussion](https://github.com/ouywm/summerrs-admin/discussions)

</div>
