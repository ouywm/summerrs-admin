<div align="center">

<img src="docs/static/logo.png" alt="Summerrs Admin Logo" width="200"/>

# Summerrs Admin

[中文](README.zh.md) | **English**

> Full-stack Rust admin system · Database sharding, multi-tenant isolation, MCP service, declarative macros — all in one binary

[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.93%2B-orange.svg?logo=rust&logoColor=white)](https://www.rust-lang.org)
[![Edition](https://img.shields.io/badge/edition-2024-orange.svg)](https://doc.rust-lang.org/edition-guide/rust-2024/index.html)
[![GitHub stars](https://img.shields.io/github/stars/ouywm/summerrs-admin?style=flat&color=yellow&logo=github)](https://github.com/ouywm/summerrs-admin/stargazers)
[![zread](https://img.shields.io/badge/Ask_Zread-_.svg?style=flat&color=00b0aa&labelColor=000000&logo=data%3Aimage%2Fsvg%2Bxml%3Bbase64%2CPHN2ZyB3aWR0aD0iMTYiIGhlaWdodD0iMTYiIHZpZXdCb3g9IjAgMCAxNiAxNiIgZmlsbD0ibm9uZSIgeG1sbnM9Imh0dHA6Ly93d3cudzMub3JnLzIwMDAvc3ZnIj4KPHBhdGggZD0iTTQuOTYxNTYgMS42MDAxSDIuMjQxNTZDMS44ODgxIDEuNjAwMSAxLjYwMTU2IDEuODg2NjQgMS42MDE1NiAyLjI0MDFWNC45NjAxQzEuNjAxNTYgNS4zMTM1NiAxLjg4ODEgNS42MDAxIDIuMjQxNTYgNS42MDAxSDQuOTYxNTZDNS4zMTUwMiA1LjYwMDEgNS42MDE1NiA1LjMxMzU2IDUuNjAxNTYgNC45NjAxVjIuMjQwMUM1LjYwMTU2IDEuODg2NjQgNS4zMTUwMiAxLjYwMDEgNC45NjE1NiAxLjYwMDFaIiBmaWxsPSIjZmZmIi8%2BCjxwYXRoIGQ9Ik00Ljk2MTU2IDEwLjM5OTlIMi4yNDE1NkMxLjg4ODEgMTAuMzk5OSAxLjYwMTU2IDEwLjY4NjQgMS42MDE1NiAxMS4wMzk5VjEzLjc1OTlDMS42MDE1NiAxNC4xMTM0IDEuODg4MSAxNC4zOTk5IDIuMjQxNTYgMTQuMzk5OUg0Ljk2MTU2QzUuMzE1MDIgMTQuMzk5OSA1LjYwMTU2IDE0LjExMzQgNS42MDE1NiAxMy43NTk5VjExLjAzOTlDNS42MDE1NiAxMC42ODY0IDUuMzE1MDIgMTAuMzk5OSA0Ljk2MTU2IDEwLjM5OTlaIiBmaWxsPSIjZmZmIi8%2BCjxwYXRoIGQ9Ik0xMy43NTg0IDEuNjAwMUgxMS4wMzg0QzEwLjY4NSAxLjYwMDEgMTAuMzk4NCAxLjg4NjY0IDEwLjM5ODQgMi4yNDAxVjQuOTYwMUMxMC4zOTg0IDUuMzEzNTYgMTAuNjg1IDUuNjAwMSAxMS4wMzg0IDUuNjAwMUgxMy43NTg0QzE0LjExMTkgNS42MDAxIDE0LjM5ODQgNS4zMTM1NiAxNC4zOTg0IDQuOTYwMVYyLjI0MDFDMTQuMzk4NCAxLjg4NjY0IDE0LjExMTkgMS42MDAxIDEzLjc1ODQgMS42MDAxWiIgZmlsbD0iI2ZmZiIvPgo8cGF0aCBkPSJNNCAxMkwxMiA0TDQgMTJaIiBmaWxsPSIjZmZmIi8%2BCjxwYXRoIGQ9Ik00IDEyTDEyIDQiIHN0cm9rZT0iI2ZmZiIgc3Ryb2tlLXdpZHRoPSIxLjUiIHN0cm9rZS1saW5lY2FwPSJyb3VuZCIvPgo8L3N2Zz4K&logoColor=ffffff)](https://zread.ai/ouywm/summerrs-admin)

[Core Capabilities](#core-capabilities) · [Architecture](#architecture) · [Project Structure](#project-structure) · [Frontend & Docs](#frontend--docs)

</div>

---

## What It Is

`summerrs-admin` is a production-grade admin system **written entirely in Rust**, built on top of the [Summer framework](https://github.com/ouywm/spring-rs) (a Spring-style application skeleton for Rust). It bundles capabilities that usually require a whole backend team to assemble — authentication, multi-tenancy, real-time messaging, object storage, declarative auditing — into a single binary via a **plugin composition** model. Use what you need, ignore what you don't.

It is not a demo, nor a showcase of any single component — it is a **complete, self-contained, deployable** backend foundation.

---

## How It Differs From Similar Projects

`summerrs-admin` leverages **Rust's zero-cost abstractions, type safety, and compile-time guarantees** to integrate capabilities that typically require multiple independent services into a single high-performance binary:

| Capability | Typical | This Project (Rust Advantages) |
|---|---|---|
| **Database Sharding** | Bolted on via ShardingSphere / Vitess | `summer-sharding` rewrites SQL transparently — zero runtime overhead, compile-time type safety |
| **MCP Service** | A standalone MCP server process | `summer-mcp` introspects business schema directly; AI assistants generate CRUD with type-safe guarantees |
| **Declarative Audit & Rate Limiting** | Middleware + handwritten code | `#[login]` `#[has_perm]` `#[rate_limit]` — procedural macros expand at compile time, no reflection overhead |

**Core Rust Advantages**:
- **Single Binary Deployment** — No JVM/interpreter required, instant startup
- **Memory Safety** — Eliminates data races and memory leaks at compile time
- **High Performance** — Zero-cost abstractions, performance close to C/C++
- **Type Safety** — Catches most errors at compile time, reducing runtime failures

---

## Architecture

The system is built around **plugin composition**. `crates/app/src/main.rs` is the assembly point — plugins fed into `App::new()` in order:

```
                    HTTP 8080
                       │
                       ▼
        ┌──────────────────────────────────┐
        │  Tower middleware (CORS /        │
        │  compression / panic guard /     │
        │  client IP extraction)           │
        └──────────────┬───────────────────┘
                       │
                       ▼
                  /api/* (JWT)
                  summer-system
                       │
                       ▼
        ┌──────────────────────────────────┐
        │  Declarative macro layer         │
        │  #[login] #[has_perm]            │
        │  #[has_role] #[rate_limit]       │
        │  #[operation_log]                │
        └──────────────┬───────────────────┘
                       │
                       ▼
        ┌──────────────────────────────────┐
        │  Sharding / SQL rewrite layer    │
        │  Tenant context injection        │
        └──────┬─────────────┬─────────────┘
               ▼             ▼
          PostgreSQL 17    Redis 7
          (primary)        (sessions / cache / rate limit)
                                │
                                ▼
                    Socket.IO / background jobs / S3
```

---

## Core Capabilities

### Authentication & Authorization
- **Multi-algorithm JWT** — HS256 / RS256 / ES256 / EdDSA, key rotation supported
- **Bitmap RBAC** — Permission bitmap compression to reduce transmission size
- **Declarative macros** — `#[login]` `#[has_perm("user:create")]` `#[has_role("admin")]` `#[public]`
- **Session governance** — concurrent login limits, per-device caps, token refresh, force logout

### Multi-Tenancy & Database
- **Four isolation tiers**

  | Mode | Suited for | Mechanism |
  |---|---|---|
  | `shared_row` | Most SaaS | SQL rewrite injects `tenant_id` filter |
  | `separate_table` | Mid isolation | Physical sharding (`user_001` / `user_002`) |
  | `separate_schema` | Strong isolation | PostgreSQL schema-level separation |
  | `separate_database` | Full isolation | Each tenant gets its own physical database |

- **SQL rewrite engine** — tenant context injected transparently, business code untouched
- **CDC pipeline** — change data capture across tenants

### MCP Server Integration
- **Schema discovery** — AI assistants can introspect the database schema
- **Code generation** — generate CRUD modules through dialogue
- **Menu / dictionary auto-deploy** — prompt-driven persistence
- **Underlying** — built on [rmcp](https://github.com/modelcontextprotocol/rust-sdk) (the official Rust MCP SDK), supporting both stdio and streamable-http transports

### Real-time & Background Processing
- **Socket.IO** — bidirectional real-time, session state in Redis (horizontally scalable)
- **Background task queue** — typed jobs, 4 workers by default, capacity 4096
- **Batch logging** — operation logs flushed asynchronously, off the hot path
- **Cron jobs** — driven by `tokio-cron-scheduler`

### Storage & Utilities
- **S3-compatible storage** — AWS S3 / MinIO / RustFS, multipart upload up to 5 GB
- **IP geolocation** — IP2Region xdb embedded, login logs auto-attributed
- **i18n** — compile-time, currently zh / en
- **Rate limiting** — 5 algorithms: fixed window, sliding window, token bucket, leaky bucket, Lua script
- **OpenAPI docs** — at `/docs`, with Swagger UI

---

## Project Structure

```
summerrs-admin/
├── crates/
│   ├── app/                          # binary entry, assembles all plugins
│   ├── summer-admin-macros/          # declarative macros (#[login] / #[has_perm] etc.)
│   ├── summer-auth/                  # JWT auth + path policy
│   ├── summer-common/                # shared types & utilities
│   ├── summer-domain/                # domain models (entities / VOs)
│   ├── summer-sharding/              # sharding / multi-tenancy middleware
│   ├── summer-mcp/                   # MCP server
│   ├── summer-plugins/               # S3 / IP2Region / background jobs etc.
│   └── summer-system/                # system business (RBAC / users / menus / Socket.IO)
│       └── model/
├── config/                           # multi-env configs (dev / prod / test)
├── sql/                              # database source of truth
│   ├── sys/                          # system domain (users / menus / perms / logs)
│   ├── tenant/                       # tenant control plane
│   └── biz/                          # B/C-side business
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
Project documentation and guides:
[https://ouywm.github.io/summer-admin-site/](https://ouywm.github.io/summer-admin-site/)

---

<div align="center">

If this project helps you, a Star is appreciated.

[Report an issue](https://github.com/ouywm/summerrs-admin/issues) · [Start a discussion](https://github.com/ouywm/summerrs-admin/discussions)

</div>
