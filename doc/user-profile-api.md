# 个人中心 - API 接口文档

## 接口概述

本文档描述个人中心相关的后端接口规范，包括修改个人密码、编辑个人信息、查看登录日志三个功能模块。

---

## 1. 修改个人密码

### 接口信息

**接口路径：** `PUT /api/user/profile/password`

**接口描述：** 用户修改自己的登录密码

**请求方式：** PUT

### 请求参数

**请求体参数（JSON）：**

| 参数名         | 类型     | 必填 | 说明          | 示例           |
|-------------|--------|----|-------------|--------------|
| oldPassword | string | 是  | 当前密码        | "123456"     |
| newPassword | string | 是  | 新密码（长度至少6位） | "newpass123" |

**请求示例：**

```bash
curl -X PUT http://localhost:8080/api/user/profile/password \
  -H "Content-Type: application/json" \
  -H "Authorization: <token>" \
  -d '{
    "oldPassword": "123456",
    "newPassword": "newpass123"
  }'
```

### 响应结果

**成功响应（200）：**

```json
{
  "code": 200,
  "msg": "密码修改成功",
  "data": null
}
```

**错误响应：**

1. 旧密码错误（400）：

```json
{
  "code": 400,
  "msg": "当前密码不正确",
  "data": null
}
```

2. 新密码格式不正确（400）：

```json
{
  "code": 400,
  "msg": "新密码长度至少6位",
  "data": null
}
```

3. 未登录（401）：

```json
{
  "code": 401,
  "msg": "未登录或登录已过期",
  "data": null
}
```

---

## 2. 编辑个人信息

### 接口信息

**接口路径：** `PUT /api/user/profile`

**接口描述：** 用户编辑自己的个人信息

**请求方式：** PUT

### 请求参数

**请求体参数（JSON）：**

| 参数名      | 类型     | 必填 | 说明                 | 示例                     |
|----------|--------|----|--------------------|------------------------|
| realName | string | 否  | 真实姓名               | "张三"                   |
| nickName | string | 否  | 昵称                 | "皮卡丘"                  |
| email    | string | 否  | 邮箱地址               | "zhangsan@example.com" |
| phone    | string | 否  | 手机号码               | "13800138000"          |
| gender   | number | 否  | 性别（0=未知, 1=男, 2=女） | 1                      |

**请求示例：**

```bash
curl -X PUT http://localhost:8080/api/user/profile \
  -H "Content-Type: application/json" \
  -H "Authorization: <token>" \
  -d '{
    "realName": "张三",
    "nickName": "皮卡丘",
    "email": "zhangsan@example.com",
    "mobile": "13800138000",
    "address": "广东省深圳市",
    "gender": 1,
    "description": "专注于前端开发"
  }'
```

### 响应结果

**成功响应（200）：**

```json
{
  "code": 200,
  "msg": "个人信息更新成功",
  "data": {
    "userId": 1,
    "userName": "zhangsan",
    "realName": "张三",
    "nickName": "皮卡丘",
    "email": "zhangsan@example.com",
    "mobile": "13800138000",
    "address": "广东省深圳市",
    "gender": 1,
    "description": "专注于前端开发",
    "avatar": "https://example.com/avatar.jpg",
    "updateTime": "2026-02-28T10:30:00Z"
  }
}
```

**错误响应：**

1. 邮箱格式不正确（400）：

```json
{
  "code": 400,
  "msg": "邮箱格式不正确",
  "data": null
}
```

2. 手机号格式不正确（400）：

```json
{
  "code": 400,
  "msg": "手机号格式不正确",
  "data": null
}
```

3. 邮箱已被其他用户使用（409）：

```json
{
  "code": 409,
  "msg": "该邮箱已被其他用户使用",
  "data": null
}
```

---

## 3. 查看登录日志

### 接口信息

**接口路径：** `GET /api/user/profile/login-logs`

**接口描述：** 查询当前用户的登录日志记录

**请求方式：** GET

### 请求参数

**查询参数（Query）：**

| 参数名       | 类型     | 必填 | 说明               | 示例                     |
|-----------|--------|----|------------------|------------------------|
| current   | number | 否  | 当前页码（默认1）        | 1                      |
| size      | number | 否  | 每页条数（默认20）       | 20                     |
| startTime | string | 否  | 开始时间（ISO 8601格式） | "2026-01-01T00:00:00Z" |
| endTime   | string | 否  | 结束时间（ISO 8601格式） | "2026-02-28T23:59:59Z" |
| status    | number | 否  | 登录状态（1=成功, 2=失败） | 1                      |

**请求示例：**

```bash
curl -X GET "http://localhost:8080/api/user/profile/login-logs?current=1&size=20" \
  -H "Authorization: Bearer <token>"
```

### 响应结果

**成功响应（200）：**

```json
{
  "code": 200,
  "msg": "查询成功",
  "data": {
    "records": [
      {
        "id": 1001,
        "userId": 1,
        "userName": "zhangsan",
        "loginTime": "2026-02-28T10:30:00Z",
        "loginIp": "192.168.1.100",
        "loginLocation": "广东省深圳市",
        "userAgent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36",
        "browser": "Chrome 120.0",
        "os": "Windows 10",
        "device": "PC",
        "status": 1,
        "statusText": "登录成功",
        "failReason": null
      },
      {
        "id": 1000,
        "userId": 1,
        "userName": "zhangsan",
        "loginTime": "2026-02-27T15:20:00Z",
        "loginIp": "192.168.1.100",
        "loginLocation": "广东省深圳市",
        "userAgent": "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)",
        "browser": "Safari 17.0",
        "os": "macOS 14.0",
        "device": "Mac",
        "status": 2,
        "statusText": "登录失败",
        "failReason": "密码错误"
      }
    ],
    "current": 1,
    "size": 20,
    "total": 156
  }
}
```

**字段说明：**

| 字段名           | 类型     | 说明                     |
|---------------|--------|------------------------|
| id            | number | 日志记录ID                 |
| userId        | number | 用户ID                   |
| userName      | string | 用户名                    |
| loginTime     | string | 登录时间（ISO 8601格式）       |
| loginIp       | string | 登录IP地址                 |
| loginLocation | string | 登录地理位置（根据IP解析）         |
| userAgent     | string | 浏览器User-Agent          |
| browser       | string | 浏览器名称和版本               |
| os            | string | 操作系统                   |
| device        | string | 设备类型（PC/Mobile/Tablet） |
| status        | number | 登录状态（1=成功, 2=失败）       |
| statusText    | string | 状态文本                   |
| failReason    | string | 失败原因（仅失败时有值）           |

---

## Rust 后端实现参考

### 1. 修改密码 DTO

```rust
use serde::{Deserialize, Serialize};
use schemars::JsonSchema;
use validator::Validate;

/// 修改密码请求参数
#[derive(Debug, Clone, Deserialize, Validate, JsonSchema)]
pub struct ChangePasswordDto {
    /// 当前密码
    #[validate(length(min = 1, message = "请输入当前密码"))]
    pub old_password: String,

    /// 新密码（长度至少6位）
    #[validate(length(min = 6, message = "新密码长度至少6位"))]
    pub new_password: String,
}
```

### 2. 更新个人信息 DTO

```rust
use serde::{Deserialize, Serialize};
use schemars::JsonSchema;
use validator::Validate;

/// 更新个人信息请求参数
#[derive(Debug, Clone, Deserialize, Validate, JsonSchema)]
pub struct UpdateProfileDto {
    /// 真实姓名
    #[validate(length(max = 50, message = "姓名长度不能超过50个字符"))]
    pub real_name: Option<String>,

    /// 昵称
    #[validate(length(max = 50, message = "昵称长度不能超过50个字符"))]
    pub nick_name: Option<String>,

    /// 邮箱
    #[validate(email(message = "邮箱格式不正确"))]
    pub email: Option<String>,

    /// 手机号
    #[validate(regex(path = "PHONE_REGEX", message = "手机号格式不正确"))]
    pub mobile: Option<String>,

    /// 地址
    pub address: Option<String>,

    /// 性别（0=未知, 1=男, 2=女）
    pub gender: Option<i32>,

    /// 个人介绍
    #[validate(length(max = 500, message = "个人介绍不能超过500个字符"))]
    pub description: Option<String>,
}

lazy_static! {
    static ref PHONE_REGEX: Regex = Regex::new(r"^1[3-9]\d{9}$").unwrap();
}
```

### 3. 登录日志 VO

```rust
use serde::{Deserialize, Serialize};
use schemars::JsonSchema;

/// 登录日志响应
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct LoginLogVo {
    pub id: i64,
    pub user_id: i64,
    pub user_name: String,
    pub login_time: String,
    pub login_ip: String,
    pub login_location: String,
    pub user_agent: String,
    pub browser: String,
    pub os: String,
    pub device: String,
    pub status: i32,
    pub status_text: String,
    pub fail_reason: Option<String>,
}
```

### 4. 路由定义

```rust
use axum::{
    routing::{get, put},
    Router,
};

pub fn profile_routes() -> Router {
    Router::new()
        .route("/api/user/profile", put(update_profile))
        .route("/api/user/profile/password", put(change_password))
        .route("/api/user/profile/login-logs", get(get_login_logs))
}
```

### 5. Handler 实现示例

```rust
use axum::{
    extract::{Query, State},
    Json,
};
use crate::common::response::ApiResponse;

/// 修改密码
pub async fn change_password(
    State(app_state): State<AppState>,
    提取登录用户id,
    ValidatedJson(dto): ValidatedJson<ChangePasswordDto>,
) -> Result<ApiResponse<()>, AppError> {
    // 1. 验证参数
    dto.validate()?;

    // 2. 获取用户信息
    let user = user_service::get_user_by_id(&app_state.db, user_id).await?
        .ok_or(AppError::NotFound("用户不存在".to_string()))?;

    // 3. 验证旧密码
    if !verify_password(&dto.old_password, &user.password_hash)? {
        return Err(AppError::BadRequest("当前密码不正确".to_string()));
    }

    // 4. 加密新密码
    let new_password_hash = hash_password(&dto.new_password)?;

    // 5. 更新数据库
    user_service::update_password(&app_state.db, user_id, &new_password_hash).await?;

    // 6. 记录操作日志
    log_service::log_user_action(
        &app_state.db,
        user_id,
        "修改密码",
        "用户修改了登录密码",
    ).await?;

    Ok(ApiResponse::success_with_msg(None, "密码修改成功"))
}

/// 更新个人信息
pub async fn update_profile(
    State(app_state): State<AppState>,
        提取登录用户id,
    ValidatedJson(dto): ValidatedJson<UpdateProfileDto>,
) -> Result<ApiResponse<UserProfileVo>, AppError> {
    // 1. 验证参数
    dto.validate()?;

    // 2. 检查邮箱是否被其他用户使用
    if let Some(ref email) = dto.email {
        if user_service::is_email_taken(&app_state.db, email, Some(user_id)).await? {
            return Err(AppError::Conflict("该邮箱已被其他用户使用".to_string()));
        }
    }

    // 3. 更新用户信息
    user_service::update_profile(&app_state.db, user_id, dto).await?;

    // 4. 获取更新后的用户信息
    let profile = user_service::get_user_profile(&app_state.db, user_id).await?;

    Ok(ApiResponse::success_with_msg(Some(profile), "个人信息更新成功"))
}

/// 获取登录日志
pub async fn get_login_logs(
    State(app_state): State<AppState>,
            提取登录用户id,
    Query(params): Query<LoginLogQueryDto>,
) -> Result<ApiResponse<PaginatedResponse<LoginLogVo>>, AppError> {
    let logs = login_log_service::get_user_login_logs(
        &app_state.db,
        user_id,
        params,
    ).await?;

    Ok(ApiResponse::success(Some(logs)))
}
```

---

## 数据库表设计参考

### 登录日志表（login_logs）

```sql
CREATE TABLE login_logs
(
    id             BIGINT PRIMARY KEY AUTO_INCREMENT,
    user_id        BIGINT      NOT NULL COMMENT '用户ID',
    user_name      VARCHAR(50) NOT NULL COMMENT '用户名',
    login_time     DATETIME    NOT NULL COMMENT '登录时间',
    login_ip       VARCHAR(50) NOT NULL COMMENT '登录IP',
    login_location VARCHAR(200) COMMENT '登录地理位置',
    user_agent     TEXT COMMENT '浏览器User-Agent',
    browser        VARCHAR(100) COMMENT '浏览器',
    os             VARCHAR(100) COMMENT '操作系统',
    device         VARCHAR(50) COMMENT '设备类型',
    status         TINYINT     NOT NULL COMMENT '登录状态（1=成功, 2=失败）',
    fail_reason    VARCHAR(200) COMMENT '失败原因',
    create_time    DATETIME    NOT NULL DEFAULT CURRENT_TIMESTAMP,
    INDEX          idx_user_id (user_id),
    INDEX          idx_login_time (login_time),
    INDEX          idx_status (status)
) COMMENT='用户登录日志表';
```

---

## 安全建议

1. **密码修改**
    - 必须验证旧密码正确性
    - 新密码需要加密存储（bcrypt/argon2）
    - 修改成功后可选择：强制重新登录或保持当前会话
    - 记录密码修改操作日志

2. **个人信息更新**
    - 邮箱/手机号需要验证唯一性
    - 敏感信息修改可要求二次验证（短信验证码）
    - 记录信息修改操作日志

3. **登录日志**
    - 只能查看自己的登录日志
    - 记录详细的登录信息（IP、设备、浏览器等）
    - 异常登录可发送通知提醒用户

4. **权限控制**
    - 所有接口都需要登录认证
    - 从 Token 中提取用户 ID，不允许用户指定
    - 防止用户查看或修改其他用户的信息

---

## 前端集成说明

前端代码位于：`src/views/system/user-center/index.vue`

需要添加的 API 函数（`src/api/user.ts`）：

```typescript
/** 修改个人密码 */
export function fetchChangePassword(params: {
  oldPassword: string
  newPassword: string
}) {
  return request.put<null>({
    url: '/api/user/profile/password',
    params
  })
}

/** 更新个人信息 */
export function fetchUpdateProfile(params: {
  realName?: string
  nickName?: string
  email?: string
  mobile?: string
  address?: string
  gender?: number
  description?: string
}) {
  return request.put<UserProfileVo>({
    url: '/api/user/profile',
    params
  })
}

/** 获取登录日志 */
export function fetchLoginLogs(params: {
  current?: number
  size?: number
  startTime?: string
  endTime?: string
  status?: number
}) {
  return request.get<PaginatedResponse<LoginLogVo>>({
    url: '/api/user/profile/login-logs',
    params
  })
}
```

---

## 测试用例

### 1. 修改密码测试

**正常流程：**

1. 用户登录
2. 进入个人中心
3. 输入当前密码和新密码
4. 提交后显示成功提示

**异常流程：**

1. 旧密码输入错误 → 返回400错误
2. 新密码少于6位 → 前端验证失败
3. 未登录状态 → 返回401错误

### 2. 更新个人信息测试

**正常流程：**

1. 用户登录
2. 修改个人信息字段
3. 提交后显示成功提示并刷新显示

**异常流程：**

1. 邮箱格式错误 → 返回400错误
2. 邮箱已被占用 → 返回409错误

### 3. 查看登录日志测试

**正常流程：**

1. 用户登录
2. 进入个人中心
3. 查看登录日志列表
4. 可以按时间、状态筛选

---

## 更新日志

- 2026-02-28：初始版本，定义个人中心相关接口规范