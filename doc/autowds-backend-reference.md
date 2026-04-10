# autowds-backend 项目参考分析

> 来源: https://github.com/AutoWDS/autowds-backend

## 项目简介

autowds 是一个 Web 爬虫任务管理 SaaS 平台的后端，基于 Rust 构建。用户可以创建、管理和执行数据爬虫任务，支持规则配置和 cron 调度。采用付费订阅 + 积分制的商业模式。

## 技术栈

- **框架**: Spring-RS (spring-web, spring-sea-orm, spring-redis, spring-job, spring-mail)
- **Web**: Axum (通过 spring-web)
- **ORM**: SeaORM 1.0 + PostgreSQL
- **缓存**: Redis
- **认证**: JWT (jsonwebtoken)
- **支付**: 支付宝 SDK + 微信支付 SDK
- **任务调度**: spring-job + Apalis
- **校验**: axum-valid + validator
- **邮件**: spring-mail + Askama 模板

## 业务模块

| 模块 | 功能 |
|------|------|
| 用户管理 | 注册/登录(JWT)、邮箱验证码、密码重置、邀请码、积分体系 |
| 爬虫任务 | CRUD、批量创建(最多10个)、cron 调度、规则配置 |
| 任务模板 | 预置爬虫模板，按主题/语言/版本筛选，收藏功能 |
| 支付 | 支付宝 + 微信支付，QR 码生成，订单管理，支付回调，自动升级版本 |
| 管理后台 | 用户/任务/模板 CRUD，积分调整，统计概览 |
| 积分系统 | 注册奖励(100积分)、邀请奖励、导出消耗(1积分/次)、管理员调整 |

## 版本等级(付费分层)

| 等级 | 类型 | 任务上限 |
|------|------|---------|
| L0 | 免费 | 3 个 |
| L1 | 付费 | 10 个 |
| L2 | 付费 | 50 个 |
| L3 | 付费 | 200 个 |

## 项目结构

```
src/
├── main.rs                 # App 入口，插件初始化
├── router/                 # API 路由处理器
│   ├── user.rs             # 用户认证 & 个人资料
│   ├── task.rs             # 爬虫任务 CRUD & 调度
│   ├── template.rs         # 任务模板 & 收藏
│   ├── pay.rs              # 支付处理
│   ├── pay_query.rs        # 支付查询
│   ├── admin.rs            # 管理后台接口
│   ├── statistics.rs       # 统计分析
│   └── token.rs            # Token/JWT 管理
├── model/                  # 数据模型
│   ├── _entities/          # sea-orm-cli 自动生成(不手动编辑)
│   │   ├── account_user.rs
│   │   ├── scraper_task.rs
│   │   ├── task_template.rs
│   │   ├── pay_order.rs
│   │   ├── favorite.rs
│   │   ├── credit_log.rs
│   │   ├── prelude.rs
│   │   ├── sea_orm_active_enums.rs
│   │   └── mod.rs
│   ├── account_user.rs     # 手写扩展(re-export + ActiveModelBehavior)
│   ├── scraper_task.rs
│   ├── task_template.rs
│   ├── pay_order.rs
│   ├── favorite.rs
│   ├── credit_log.rs
│   └── mod.rs
├── views/                  # 请求/响应类型定义(DTO/VO)
│   ├── admin.rs            # 管理后台请求/响应
│   ├── user.rs             # 用户相关
│   ├── task.rs             # 任务相关
│   ├── template.rs         # 模板相关
│   └── pay.rs              # 支付相关
├── utils/                  # 业务服务 & 工具
│   ├── jwt.rs              # JWT 编解码
│   ├── pay_service.rs      # 支付订单操作
│   ├── user_service.rs     # 用户更新、版本确认
│   ├── credit.rs           # 积分操作 & 校验
│   ├── pay_plugin.rs       # 支付 SDK 初始化
│   ├── mail.rs             # 邮件发送
│   ├── validate_code.rs    # 邮箱验证码生成
│   └── keys/               # JWT RSA 密钥对
├── config/                 # 配置类
│   ├── pay.rs              # 支付网关配置
│   └── mail.rs             # 邮件配置
└── task/                   # 后台任务
    └── pay_check.rs        # 支付状态检查任务
```

## 架构设计要点

### model 两层设计

- `_entities/` — sea-orm-cli 生成的纯数据定义，不含 `ActiveModelBehavior`
- 外层同名文件 — `pub use super::_entities::xxx::*;` re-export 后，补充自定义逻辑(如 `before_save` 自动填充时间戳)
- 重新生成时只覆盖 `_entities/`，不影响手写扩展

### router 直接操作 ORM

- 简单 CRUD 直接在 router 里写，注入 `Component<DbConn>`
- 复杂业务逻辑放 `utils/` 中的 service 函数
- 返回具体类型如 `Result<Json<UserResp>>`，错误用 `KnownWebError`

### views 统一管理请求/响应类型

- 按模块分文件(admin.rs, user.rs, task.rs 等)
- 包含 Query 参数、Request body、Response body
- `From<Model>` 实现放在 views 中，完成 entity → response 的转换
- 使用 `validator` 做请求参数校验

## 对 summerrs-admin 的参考价值

| autowds 模块 | 对应我们的 | 说明 |
|-------------|-----------|------|
| `model/_entities/` | `model/entity/` | 我们的 entity 对应它的 `_entities`，后续接 cli 生成 |
| `model/*.rs` 外层 | 待建 | 扩展 ActiveModelBehavior 等自定义逻辑 |
| `views/` | `model/views/` | 我们已预留 views 目录 |
| `utils/` | `service` crate | 复杂业务逻辑的归属地 |
| `router/` | `app/router/` | 一致 |
| `config/` | 根目录 `config/` | 我们用 toml 配置文件 |
