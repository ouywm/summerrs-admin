# 参考项目文件模块总结

## 目的

本文基于参考项目返回的文件模块接口数据，整理其数据模型、接口语义与可借鉴点，用于后续梳理 `summerrs-admin` 的文件表设计与文件中心能力。

## 一、参考项目暴露出来的核心能力

从返回数据可以看出，参考项目的文件模块已经不是“简单上传一个文件然后存一条记录”，而是一个相对完整的文件中心。

当前至少包含以下能力：

1. 文件列表分页查询
2. 文件夹树查询
3. 文件详情查询
4. 上传后获取下载 URL
5. 公开分享链接生成
6. 公开文件访问令牌
7. 文件可见性控制
8. 软删除与异步清理预留
9. 文件摘要统计

## 二、参考项目的文件主记录长什么样

从文件列表和详情返回可以总结出一条文件记录大致包含以下信息：

### 1. 业务标识

- `id`
- `fileNo`

说明：

- `id` 是数据库主键
- `fileNo` 是对外业务编号，适合给前端、日志、导出、客服排查使用

### 2. 存储定位信息

- `provider`
- `bucket`
- `objectKey`
- `etag`

说明：

- 这部分描述的是“真实对象存储层”
- `provider` 表示文件来自哪个存储服务，例如 `ALIYUN_OSS`
- `objectKey` 明确是对象存储里的真实 key，不再混成“文件路径/文件名”二义性字段

### 3. 文件展示信息

- `originalName`
- `displayName`
- `extension`
- `mimeType`
- `kind`
- `size`

说明：

- `originalName` 是上传原始名称
- `displayName` 是系统允许用户看到或自定义的展示名称
- `kind` 是更高一层的业务分类，例如 `IMAGE`

这里比单纯的 `suffix + mime + size` 更完整，因为它明确区分了：

- 文件原始名
- 文件展示名
- 文件类型分类

### 4. 媒体扩展信息

- `width`
- `height`
- `duration`
- `pageCount`

说明：

- 图片、视频、文档这几类文件，后续通常都需要不同的扩展属性
- 参考项目已经提前为多媒体和文档能力预留了结构

### 5. 访问控制信息

- `visibility`
- `status`
- `publicToken`
- `publicUrlExpiresAt`

说明：

- `visibility` 控制公开/私有
- `status` 控制文件记录当前生命周期状态
- `publicToken` 用来生成公开访问链接
- `publicUrlExpiresAt` 说明系统已经考虑“公开链接是否有时效”这一层语义

### 6. 管理与扩展信息

- `tags`
- `remark`
- `metadata`

说明：

- `tags` 适合做运营分类
- `remark` 适合人工备注
- `metadata` 适合存放结构化扩展数据

### 7. 删除与清理信息

- `deletedAt`
- `deletedBy`
- `purgeStatus`
- `purgedAt`
- `purgeError`

说明：

- 这是典型的“软删除 + 异步物理清理”设计
- 删除文件不代表立刻从对象存储物理删除
- 可以先打删除标记，再交给后台任务清理

### 8. 关联信息

- `folder`
- `creator`

说明：

- 文件直接挂载到文件夹
- 文件返回时直接带创建人摘要
- 这意味着接口层已经把“文件中心视图模型”做好了，而不是只返回裸表字段

### 9. 审计时间

- `createdAt`
- `updatedAt`

说明：

- 文件记录是标准业务实体，而不是一次性上传日志

## 三、文件夹模型长什么样

文件夹树接口返回的数据说明参考项目是把“文件夹”当成独立实体来建模的。

字段包括：

- `id`
- `parentId`
- `name`
- `slug`
- `visibility`
- `sort`
- `fileCount`
- `children`

从这个结构可以得到几个关键信号：

1. 文件夹是树结构，不是简单标签
2. 文件夹有自己的可见性
3. 文件夹有排序能力
4. 文件夹会聚合文件数量
5. 文件和文件夹关系是显式关系，不是把路径字符串硬拆出来

## 四、接口设计上的明显特点

### 1. 文件列表接口不只是返回 `records`

还返回：

- `current`
- `size`
- `total`
- `summary`

其中 `summary` 又包含：

- `total`
- `privateCount`
- `publicCount`

这说明参考项目在列表接口里已经把运营统计需求考虑进来了。

### 2. 上传结果分层明确

参考数据里可以看到至少三类结果：

1. 下载 URL 响应
   返回 `url` 与 `expiresAt`

2. 文件详情响应
   返回完整文件实体

3. 公开链接生成响应
   返回 `token`、`visibility`、`publicUrl`

说明：

- 它没有把“文件本体信息”和“下载动作结果”和“公开分享动作结果”混成一个响应结构

### 3. 文件和对象存储是解耦的

文件记录里既有：

- 业务信息
- 访问控制
- 删除清理状态

也有：

- provider
- bucket
- objectKey
- etag

这意味着它是“业务文件中心”，不是简单上传 SDK 封装。

## 五、对我们当前 `sys.file` 设计的启发

结合当前 [file.sql](/Volumes/990pro/code/rust/summerrs-admin/sql/sys/file.sql)，参考项目最值得借鉴的地方主要有下面几类。

### 1. 当前表缺少业务编号

我们现在只有：

- `id`

参考项目额外有：

- `fileNo`

建议：

- 增加稳定的业务编号字段，用于外部系统、前端、日志和客服定位

### 2. 当前表的存储字段表达不够清晰

我们现在是：

- `file_name`
- `file_path`

参考项目是：

- `objectKey`
- `bucket`
- `provider`
- `etag`

建议：

- 明确把“对象存储定位信息”建模出来
- `file_path` 最好收敛成明确的 `object_key`
- 增加 `provider`
- 增加 `etag`

### 3. 当前表没有文件夹模型

我们现在文件只有路径，没有独立文件夹实体。

参考项目说明：

- 文件夹是独立表
- 文件记录通过关联指向文件夹

建议：

- 如果你们未来要做文件中心，而不只是附件上传，就应该拆出文件夹表

### 4. 当前表没有展示层字段

我们现在只有：

- `original_name`
- `file_name`

建议补充：

- `display_name`
- `kind`
- `visibility`
- `status`

### 5. 当前表没有软删除/清理状态

参考项目已经把删除链路分成：

- 逻辑删除
- 物理清理

建议补充：

- `deleted_at`
- `deleted_by`
- `purge_status`
- `purged_at`
- `purge_error`

### 6. 当前表对多媒体扩展不友好

建议预留：

- `width`
- `height`
- `duration`
- `page_count`

### 7. 当前表缺少公开分享能力字段

参考项目说明：

- 公开访问不是直接把原始对象地址暴露出去
- 而是通过 `publicToken` 和独立公开访问地址来做

建议补充：

- `public_token`
- `public_url_expires_at`

### 8. 当前表扩展性不足

建议补充：

- `tags`
- `metadata`
- `remark`

## 六、我对参考项目模型的整体判断

参考项目的设计更接近“文件中心”而不是“上传记录表”。

它已经清楚地区分了：

1. 对象存储信息
2. 文件业务信息
3. 展示信息
4. 权限与分享信息
5. 删除与清理生命周期
6. 文件夹归属
7. 创建人信息

这比当前 `sys.file` 的表达力高很多。

## 七、对我们当前演进方向的建议

如果只是满足“上传文件并能下载”：

- 当前表可以继续用，但要补幂等、索引和字段语义

如果要做真正的文件中心：

建议往下面两层模型演进：

### 1. 文件对象层

描述真正落在 OSS / S3 上的对象：

- provider
- bucket
- object_key
- etag
- size
- mime_type
- extension
- file_md5

### 2. 文件业务层

描述业务视角的文件记录：

- file_no
- original_name
- display_name
- kind
- visibility
- status
- folder_id
- creator_id
- public_token
- deleted_at / purge_status
- metadata / tags / remark

如果不想一步拆两张表，也建议至少把当前单表先往“文件中心表”靠，而不是继续停留在“附件表”状态。

## 八、建议优先级

如果按优先级排序，我建议优先做：

1. 明确 `file_name` / `file_path` 语义
2. 增加 `provider`、`object_key`、`etag`
3. 增加 `visibility`、`status`
4. 增加 `display_name`
5. 增加软删除 / 清理字段
6. 再考虑文件夹表与公开分享能力

## 九、结论

这份参考项目返回的数据说明，对方做的已经不是“文件上传接口”，而是一个有：

- 文件列表
- 文件夹树
- 公开分享
- 可见性控制
- 生命周期管理
- 统计摘要

的完整文件中心。

如果我们未来要对齐这类能力，当前 `sys.file` 需要继续演进，而且重点不是只补 1~2 个字段，而是先统一文件模型语义。
