# 前端适配指南：ProblemDetails 错误响应

> 后端采用 **ApiResponse** 处理成功响应 + **ProblemDetails (RFC 7807)** 处理错误响应。
> 前端需要适配错误响应的解析方式。

---

## 一、响应格式总览

### 成功响应（HTTP 200）

后端使用 `ApiResponse` 包装，格式与前端 `BaseResponse` 完全一致：

```json
{
  "code": 200,
  "msg": "操作成功",
  "data": { ... }
}
```

- `code: 200` → 完全成功
- `code: 非200`（如 1001） → 业务警告（如"部分导入失败，3条记录跳过"）
- **HTTP 状态码始终为 200**

> 成功响应部分前端**无需任何修改**，现有逻辑完全兼容。

### 错误响应（HTTP 4xx/5xx）

后端使用 ProblemDetails (RFC 7807) 格式：

```
HTTP/1.1 401 Unauthorized
Content-Type: application/problem+json

{
  "type": "about:blank",
  "title": "token已过期",
  "status": 401,
  "detail": "token已过期",
  "instance": "/api/user/info"
}
```

**字段说明：**

| 字段 | 类型 | 是否必有 | 说明 |
|------|------|:--------:|------|
| `type` | string | 是 | 问题类型 URI，默认 `"about:blank"`，可自定义如 `"urn:error:token-expired"` |
| `title` | string | 是 | 错误标题（简短描述） |
| `status` | number | 是 | HTTP 状态码（与响应头一致） |
| `detail` | string | 可能 | 错误详情（人类可读，**建议用此字段展示给用户**） |
| `instance` | string | 可能 | 发生错误的请求路径 |

---

## 二、后端错误码清单

| HTTP 状态码 | 错误类型 | 触发场景 |
|:-----------:|---------|---------|
| 400 | BadRequest | 请求参数错误 |
| 401 | Unauthorized | Token 无效/过期、未登录 |
| 403 | Forbidden | 权限不足 |
| 404 | NotFound | 资源不存在 |
| 409 | Conflict | 资源冲突（如用户名已存在） |
| 422 | ValidationFailed | 数据校验失败 |
| 429 | TooManyRequests | 请求频率超限 |
| 500 | Internal | 服务器内部错误 |
| 503 | ServiceUnavailable | 服务不可用 |

---

## 三、各场景响应示例

### 登录成功

```
HTTP/1.1 200 OK
Content-Type: application/json

{
  "code": 200,
  "msg": "",
  "data": {
    "token": "eyJhbGciOiJIUzI1NiJ9...",
    "refreshToken": "eyJhbGciOiJIUzI1NiJ9..."
  }
}
```

### 登录失败（密码错误）

```
HTTP/1.1 400 Bad Request
Content-Type: application/problem+json

{
  "type": "about:blank",
  "title": "用户名或密码错误",
  "status": 400,
  "detail": "用户名或密码错误",
  "instance": "/api/auth/login"
}
```

### Token 过期

```
HTTP/1.1 401 Unauthorized
Content-Type: application/problem+json

{
  "type": "about:blank",
  "title": "token已过期",
  "status": 401,
  "detail": "token已过期",
  "instance": "/api/user/info"
}
```

### 权限不足

```
HTTP/1.1 403 Forbidden
Content-Type: application/problem+json

{
  "type": "about:blank",
  "title": "无权限访问此资源",
  "status": 403,
  "detail": "无权限访问此资源",
  "instance": "/api/user/list"
}
```

### 自定义业务错误码（type 字段）

```
HTTP/1.1 401 Unauthorized
Content-Type: application/problem+json

{
  "type": "urn:error:token-expired",
  "title": "token已过期",
  "status": 401,
  "detail": "token已过期",
  "instance": "/api/user/info"
}
```

> `type` 字段可用于区分同一 HTTP 状态码下的不同业务错误。
> 如 401 可能是 `token-expired`（Token 过期）或 `token-invalid`（Token 无效），前端可据此做差异化处理。

---

## 四、前端改动指南

### 核心原理

Axios 对 HTTP 状态码的处理：

```
HTTP 2xx  →  response 拦截器（成功回调）  →  解析 ApiResponse {code, msg, data}
HTTP 4xx/5xx  →  error 拦截器（错误回调）  →  解析 ProblemDetails {type, title, status, detail}
```

两种格式被 HTTP 状态码天然隔开，不会混淆。

### 4.1 修改错误拦截器

**文件：** `src/utils/http/index.ts`（响应 error 拦截器部分）

**改动前**（当前逻辑，从 error body 中读取 `{code, msg}`）：

```typescript
// 当前可能的处理方式
if (error.response) {
  const { code, msg } = error.response.data
  if (code === 401) {
    // 自动登出
  }
  showMessage(msg || '请求失败')
}
```

**改动后**（适配 ProblemDetails）：

```typescript
if (error.response) {
  const { status, detail, title, type } = error.response.data

  // 401 → 自动登出（与原逻辑一致）
  if (status === 401) {
    // 执行原有的防抖登出逻辑
    handleLogout()
    return Promise.reject(error)
  }

  // 其他错误 → 展示 detail（优先）或 title 作为错误提示
  const message = detail || title || '请求失败'
  showMessage(message)
}
```

### 4.2 成功拦截器（无需修改）

成功响应仍然是 `{code: 200, msg, data}` 格式，现有逻辑完全兼容：

```typescript
// 这段逻辑不需要改
if (response.data.code === 200) {
  return response.data
}
// code !== 200 的业务警告处理也不需要改
```

### 4.3 修改错误消息映射（可选）

**文件：** `src/utils/http/error.ts`

如果现有的 `HttpError` 类或错误消息映射基于 `{code, msg}` 格式，可以简化为直接使用 ProblemDetails 的 `detail` 字段，因为后端已经返回中文错误消息。

**改动前：**

```typescript
const errorMessages: Record<number, string> = {
  400: '请求错误',
  401: '未授权，请重新登录',
  403: '拒绝访问',
  404: '请求地址出错',
  500: '服务器内部错误',
  // ...
}
const msg = errorMessages[status] || '未知错误'
```

**改动后：**

```typescript
// 直接使用后端返回的 detail，只在没有 detail 时使用默认映射
const msg = detail || errorMessages[status] || '未知错误'
```

### 4.4 TypeScript 类型定义（可选）

**文件：** `src/types/common/response.ts` 或 `src/types/api/api.d.ts`

新增 ProblemDetails 类型：

```typescript
/** RFC 7807 ProblemDetails 错误响应 */
interface ProblemDetails {
  type: string       // 问题类型 URI
  title: string      // 错误标题
  status: number     // HTTP 状态码
  detail?: string    // 错误详情（展示给用户）
  instance?: string  // 请求路径
}
```

---

## 五、改动总结

| 文件 | 改动内容 | 必要性 |
|------|---------|:------:|
| `src/utils/http/index.ts` | error 拦截器解析 ProblemDetails | **必须** |
| `src/utils/http/error.ts` | 错误消息优先使用 `detail` 字段 | 建议 |
| `src/types/common/response.ts` | 新增 ProblemDetails 类型定义 | 建议 |

**成功拦截器和所有 API 调用处均不需要修改。**

---

## 六、快速验证

前端改完后，可按以下顺序验证：

1. **正常登录** → 检查返回 `{code: 200, msg, data: {token, refreshToken}}`
2. **错误密码登录** → 检查返回 ProblemDetails，status=400，提示信息正确
3. **携带过期 Token 访问** → 检查返回 ProblemDetails，status=401，自动登出
4. **访问无权限接口** → 检查返回 ProblemDetails，status=403，提示信息正确

---

*文档版本：v1.0*
*更新日期：2026-02-26*