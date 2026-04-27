<div align="center">

<img src="docs/static/logo.png" alt="Summerrs Admin Logo" width="200"/>

# Summerrs Admin

[中文](README.zh-CN.md) | **English**

[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.85%2B-orange.svg)](https://www.rust-lang.org)
[![Edition](https://img.shields.io/badge/edition-2024-orange.svg)](https://doc.rust-lang.org/edition-guide/rust-2024/index.html)
[![zread](https://img.shields.io/badge/Ask_Zread-_.svg?style=flat&color=00b0aa&labelColor=000000&logo=data%3Aimage%2Fsvg%2Bxml%3Bbase64%2CPHN2ZyB3aWR0aD0iMTYiIGhlaWdodD0iMTYiIHZpZXdCb3g9IjAgMCAxNiAxNiIgZmlsbD0ibm9uZSIgeG1sbnM9Imh0dHA6Ly93d3cudzMub3JnLzIwMDAvc3ZnIj4KPHBhdGggZD0iTTQuOTYxNTYgMS42MDAxSDIuMjQxNTZDMS44ODgxIDEuNjAwMSAxLjYwMTU2IDEuODg2NjQgMS42MDE1NiAyLjI0MDFWNC45NjAxQzEuNjAxNTYgNS4zMTM1NiAxLjg4ODEgNS42MDAxIDIuMjQxNTYgNS42MDAxSDQuOTYxNTZDNS4zMTUwMiA1LjYwMDEgNS42MDE1NiA1LjMxMzU2IDUuNjAxNTYgNC45NjAxVjIuMjQwMUM1LjYwMTU2IDEuODg2NjQgNS4zMTUwMiAxLjYwMDEgNC45NjE1NiAxLjYwMDFaIiBmaWxsPSIjZmZmIi8%2BCjxwYXRoIGQ9Ik00Ljk2MTU2IDEwLjM5OTlIMi4yNDE1NkMxLjg4ODEgMTAuMzk5OSAxLjYwMTU2IDEwLjY4NjQgMS42MDE1NiAxMS4wMzk5VjEzLjc1OTlDMS42MDE1NiAxNC4xMTM0IDEuODg4MSAxNC4zOTk5IDIuMjQxNTYgMTQuMzk5OUg0Ljk2MTU2QzUuMzE1MDIgMTQuMzk5OSA1LjYwMTU2IDE0LjExMzQgNS42MDE1NiAxMy43NTk5VjExLjAzOTlDNS42MDE1NiAxMC42ODY0IDUuMzE1MDIgMTAuMzk5OSA0Ljk2MTU2IDEwLjM5OTlaIiBmaWxsPSIjZmZmIi8%2BCjxwYXRoIGQ9Ik0xMy43NTg0IDEuNjAwMUgxMS4wMzg0QzEwLjY4NSAxLjYwMDEgMTAuMzk4NCAxLjg4NjY0IDEwLjM5ODQgMi4yNDAxVjQuOTYwMUMxMC4zOTg0IDUuMzEzNTYgMTAuNjg1IDUuNjAwMSAxMS4wMzg0IDUuNjAwMUgxMy43NTg0QzE0LjExMTkgNS42MDAxIDE0LjM5ODQgNS4zMTM1NiAxNC4zOTg0IDQuOTYwMVYyLjI0MDFDMTQuMzk4NCAxLjg4NjY0IDE0LjExMTkgMS42MDAxIDEzLjc1ODQgMS42MDAxWiIgZmlsbD0iI2ZmZiIvPgo8cGF0aCBkPSJNNCAxMkwxMiA0TDQgMTJaIiBmaWxsPSIjZmZmIi8%2BCjxwYXRoIGQ9Ik00IDEyTDEyIDQiIHN0cm9rZT0iI2ZmZiIgc3Ryb2tlLXdpZHRoPSIxLjUiIHN0cm9rZS1saW5lY2FwPSJyb3VuZCIvPgo8L3N2Zz4K&logoColor=ffffff)](https://zread.ai/ouywm/summerrs-admin)

</div>

---

A production-ready full-stack admin system built entirely in Rust using the Summer framework. It provides JWT authentication, RBAC authorization, database sharding, multi-tenant isolation, real-time Socket.IO communication, AI Gateway (LLM Relay), MCP server integration, and declarative code generation—all composed through a modular plugin architecture.

## What Makes This Project Unique

Summerrs-admin integrates four key features:

1. **LLM Relay Gateway** - Unified proxy for multiple AI providers, supporting OpenAI/Claude/Gemini native protocols with automatic failover and billing
2. **Database Sharding Middleware** - SQL parsing, routing, rewriting, and cross-shard result merging
3. **MCP Server** - AI assistants can discover database schemas, generate CRUD modules, and deploy menus/dictionaries
4. **Declarative Macro System** - Reduces authentication checks, operation logging, and rate limiting to single-line attributes

Combined with Socket.IO real-time messaging, S3 file storage, and background task scheduling for complete backend functionality.

## Architecture Overview

The system follows a plugin composition pattern. The binary entry point in `crates/app/src/main.rs` assembles 15 plugins into a single App instance, each responsible for a vertical domain. Request traffic passes through Tower middleware layers (CORS, compression, exception handling, client IP extraction) before reaching Axum routes, where declarative macros enforce authentication and logging at the handler function level, while sharding/SQL rewriting middleware transparently intercepts database calls.

## Core Features

### Authentication & Authorization
- **JWT Support** - HS256/RS256/ES256/EdDSA algorithms with session management
- **RBAC** - Role-based access control with permission bitmaps
- **Declarative Macros** - `#[login]`, `#[has_perm]`, `#[has_role]`, `#[public]`
- **Session Management** - Concurrent login control, device limits, token refresh

### Database & Multi-tenancy
- **Database Sharding** - SQL parsing, routing, and cross-shard merging
- **Four Isolation Levels**:
    - **Shared Row** - All tenants share tables; filter by `tenant_id` column via SQL rewriting
    - **Separate Table** - Each tenant has own tables (e.g., `user_001`, `user_002`)
    - **Separate Schema** - Each tenant has own PostgreSQL schema
    - **Separate Database** - Each tenant has own physical database
- **SQL Rewriting** - Transparent tenant context injection
- **CDC Pipeline** - Change data capture for cross-tenant sync
- **Encryption/Masking/Audit** - Built into sharding layer

### Real-time & Background Processing
- **Socket.IO** - Real-time communication with session state stored in Redis
- **Background Jobs** - Async task scheduling with typed task runners
- **Batch Log Collection** - Asynchronous operation log persistence

### AI Gateway (summer-ai)
- **Protocol Adapters** - 40+ provider ZST (Zero-Sized Type) adapters
- **Dynamic Upstream Routing** - 6 dimensions of runtime decision: protocol family, endpoint, credentials, model mapping, extra headers, routing strategy
- **Multi-Protocol Ingress** - OpenAI, Claude, Gemini native endpoints
- **Three-Phase Billing** - Reserve → Settle → Refund atomic charging
- **Automatic Failover** - Retry with different channels on failure
- **Hot-Reload Configuration** - Database-driven, no restart needed
- **Streaming** - SSE real-time responses (streaming doesn't retry)
- **Request Tracking** - Complete lifecycle logging with retry records

### MCP Server Integration
- **Schema Discovery** - AI can discover database structure
- **Code Generation** - Generate CRUD modules via AI tools
- **Menu/Dictionary Tools** - Deploy menus and dictionaries through prompts
- **Rig LLM Framework** - Support for OpenAI, DeepSeek, Ollama

### Storage & Utilities
- **S3 Storage** - Multipart upload support for large files (AWS S3, MinIO)
- **IP Geolocation** - IP2Region for login logs
- **i18n** - Compile-time internationalization (zh/en)
- **Rate Limiting** - 5 algorithms: fixed window, sliding window, token bucket, leaky bucket, Lua script


## Project Structure

```
summerrs-admin/
├── crates/
│   ├── app/                          # Application entry
│   ├── summer-system/                # Business modules: RBAC, CRUD, Socket.IO
│   ├── summer-auth/                  # JWT authentication & authorization
│   ├── summer-ai/                    # AI Gateway (LLM Relay)
│   ├── summer-sharding/              # Database sharding middleware
│   ├── summer-sql-rewrite/           # SQL rewriting engine
│   ├── summer-mcp/                   # MCP server
│   └── summer-plugins/               # Plugin implementations
├── config/                           # Environment configs
├── sql/                              # Database schemas
└── docs/                             # Project documentation
```
