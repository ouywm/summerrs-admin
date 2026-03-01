# 用户管理 - 重置密码接口文档

## 接口概述

本文档描述前端用户管理页面新增的"重置密码"功能所需的后端接口规范。

## 接口详情

### 重置用户密码

**接口路径：** `PUT /api/user/{id}/reset-password`

**接口描述：** 管理员重置指定用户的密码

**请求方式：** PUT

**路径参数：**

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| id | number | 是 | 用户ID |

**请求体参数（JSON）：**

| 参数名 | 类型 | 必填 | 说明 | 示例 |
|--------|------|------|------|------|
| newPassword | string | 是 | 新密码（长度至少6位） | "123456" |

**请求示例：**

```bash
curl -X PUT http://localhost:8080/api/user/3/reset-password \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer <token>" \
  -d '{
    "newPassword": "newpass123"
  }'
```

**成功响应（200）：**

```json
{
  "code": 200,
  "msg": "密码重置成功",
  "data": null
}
```

**错误响应：**

1. 用户不存在（404）：
```json
{
  "code": 404,
  "msg": "用户不存在",
  "data": null
}
```

2. 密码格式不正确（400）：
```json
{
  "code": 400,
  "msg": "密码长度至少6位",
  "data": null
}
```

3. 权限不足（403）：
```json
{
  "code": 403,
  "msg": "无权限重置该用户密码",
  "data": null
}
```

## Rust 后端实现参考

### DTO 定义

```rust
use serde::{Deserialize, Serialize};
use schemars::JsonSchema;

/// 重置密码请求参数
#[derive(Debug, Clone, Deserialize, JsonSchema)]
pub struct ResetPasswordDto {
    /// 新密码
    #[schemars(example = "example_new_password")]
    pub new_password: String,
}

fn example_new_password() -> &'static str {
    "newpass123"
}
```

### 验证规则

```rust
use validator::Validate;

#[derive(Debug, Clone, Deserialize, Validate, JsonSchema)]
pub struct ResetPasswordDto {
    /// 新密码（长度至少6位）
    #[validate(length(min = 6, message = "密码长度至少6位"))]
    pub new_password: String,
}
```

### 路由定义

```rust
use axum::{
    routing::put,
    Router,
};

pub fn user_routes() -> Router {
    Router::new()
        .route("/api/user/:id/reset-password", put(reset_user_password))
}
```

### Handler 实现示例

```rust
use axum::{
    extract::{Path, State},
    Json,
};
use crate::common::response::ApiResponse;

/// 重置用户密码
pub async fn reset_user_password(
    State(app_state): State<AppState>,
    Path(user_id): Path<i64>,
    Json(dto): Json<ResetPasswordDto>,
) -> Result<ApiResponse<()>, AppError> {
    // 1. 验证参数
    dto.validate()?;

    // 2. 检查用户是否存在
    let user = user_service::get_user_by_id(&app_state.db, user_id).await?;
    if user.is_none() {
        return Err(AppError::NotFound("用户不存在".to_string()));
    }

    // 3. 加密新密码
    let hashed_password = hash_password(&dto.new_password)?;

    // 4. 更新数据库
    user_service::update_password(&app_state.db, user_id, &hashed_password).await?;

    // 5. 可选：记录操作日志
    log_service::log_admin_action(
        &app_state.db,
        "重置用户密码",
        format!("重置用户ID {} 的密码", user_id),
    ).await?;

    Ok(ApiResponse::success(None))
}
```

## 前端调用示例

前端已实现的调用代码位于：
- API 定义：`src/api/system-manage.ts`
- 类型定义：`src/types/api/system.d.ts`
- 页面调用：`src/views/system/user/index.vue`

```typescript
// API 函数
export function fetchResetUserPassword(id: number, params: Api.SystemManage.ResetPasswordParams) {
  return request.put<null>({
    url: `/api/user/${id}/reset-password`,
    params
  })
}

// 类型定义
interface ResetPasswordParams {
  newPassword: string
}

// 页面调用
const handleResetPassword = async (row: UserListItem): Promise<void> => {
  const { value: newPassword } = await ElMessageBox.prompt('请输入新密码', '重置密码', {
    inputPattern: /^.{6,}$/,
    inputErrorMessage: '密码长度至少6位'
  })

  await fetchResetUserPassword(row.id, { newPassword })
  ElMessage.success(`用户 ${row.userName} 的密码已重置`)
}
```

## 安全建议

1. **权限控制**：只有管理员角色才能重置其他用户密码
2. **密码加密**：使用 bcrypt 或 argon2 等安全哈希算法
3. **操作日志**：记录谁在什么时间重置了哪个用户的密码
4. **通知机制**：可选择发送邮件/短信通知用户密码已被重置
5. **密码强度**：建议增加密码复杂度要求（大小写、数字、特殊字符）
6. **频率限制**：防止暴力重置，可添加操作频率限制

## 测试用例

### 正常流程
1. 管理员登录
2. 进入用户管理页面
3. 点击某用户的"重置密码"按钮
4. 输入新密码（至少6位）
5. 确认后显示成功提示

### 异常流程
1. 输入少于6位的密码 → 前端验证失败
2. 重置不存在的用户 → 返回404错误
3. 普通用户尝试重置 → 返回403权限错误

## 更新日志

- 2026-02-28：初始版本，定义重置密码接口规范