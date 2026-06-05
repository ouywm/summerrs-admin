# 内置任务 handler —— ratch-job 控制台录入参考

应用已从 `summer-job-dynamic`(应用内自带调度器 + `sys.job` 表)切换到
`summer-xxl-job` 执行器,任务定义与触发改由 ratch-job / xxl-job-admin 控制台
统一管理。应用只负责执行 handler,**不再有内置任务的调度配置**。

下表列出原 `summer-job-dynamic` 里写死的内置任务调度参数,迁移后需要在
ratch-job 控制台手动创建对应任务,绑定下面的 `handler 名`(即 admin 侧的
`JobHandler` 值)。

## 执行器接入信息

- 执行器 app name:`summerrs-admin-executor`(见 `config/app-*.toml` 的 `[xxl-job].app_name`)
- admin 地址:`http://ratchjob:8725/xxl-job-admin`(容器内)/ `http://127.0.0.1:8725/xxl-job-admin`(本机)
- access token:`default_token`(见 `[xxl-job].access_token`)
- 控制台:http://localhost:8825/ratchjob/ (默认 admin/admin)

## 任务清单

| handler 名 | 原调度 | 原说明 | 备注 |
|---|---|---|---|
| `summer_system::s3_multipart_cleanup` | CRON `0 0 * * * *`(每小时整点) | 扫描并清理过期的 S3 分片上传碎片 | 按 `S3Config.multipart_max_age` 判定过期 |
| `summer_system::socket_session_gc` | CRON `0 */10 * * * *`(每 10 分钟) | 清理 socket 会话索引中的幽灵条目 | Redis 残留的失效会话记录 |
| `summer_system::test_panic` | 无(原仅手动触发) | [debug 专用] 故意 panic,验证执行器 panic 处理链路 | 仅 debug 构建注册,release 不可见 |

## 控制台创建任务示例(open-api)

以 s3 清理为例,通过 ratch-job open-api 创建:

```bash
curl -X POST "http://127.0.0.1:8725/ratch/v1/job/create" \
  -H 'Content-Type: application/json' \
  -d '{
    "appName": "summerrs-admin-executor",
    "namespace": "xxl",
    "handleName": "summer_system::s3_multipart_cleanup",
    "scheduleType": "CRON",
    "cronValue": "0 0 * * * *",
    "blockingStrategy": "SERIAL_EXECUTION"
  }'
```

`socket_session_gc` 同理,把 `handleName` 换成 `summer_system::socket_session_gc`、
`cronValue` 换成 `0 */10 * * * *`。

> 注:cron 表达式格式以 ratch-job / xxl-job 控制台为准(均为 6 段 Quartz 风格,
> 与原 `summer-job-dynamic` 一致)。
