# Entity-First, DTO, VO And Schema Sync Patterns

这部分聚焦本仓库当前的数据模型约定：SeaORM 2.0、entity-first、`entity_gen + entity`、无数据库外键。

## Canonical examples

- system entity 扩展入口：`crates/summer-system-model/src/entity/sys_user.rs`
- system raw entity：`crates/summer-system-model/src/entity_gen/sys_user.rs`
- system DTO：`crates/summer-system-model/src/dto/sys_user.rs`
- system VO：`crates/summer-system-model/src/vo/sys_user.rs`
- schema sync 插件：`crates/summer-system/src/plugins/schema_sync.rs`

## 模型 crate 怎么分

- `crates/summer-system-model`：system 的 entity / dto / vo
- `crates/summer-ai-model`：AI 的表和共享契约
- `crates/summer-biz-model`：以后如果 biz/customer 再拆，就沿用同一套模式

## `entity_gen + entity` 约定

### 1. `src/entity_gen`

- 存放原始实体源码
- 允许被 codegen / MCP / CLI 覆盖
- 不要把稳定业务逻辑直接写进去

### 2. `src/entity`

- 这是应用代码唯一应该依赖的实体入口
- 每个模块通过 `include!("../entity_gen/<name>.rs")` 引入 raw entity
- `ActiveModelBehavior`、扩展方法、稳定 helper 写在这里

### 3. 导入规则

- 业务代码只用 `summer_system_model::entity::sys_user`
- 不要在业务代码里直接依赖 `entity_gen`

## SeaORM 2.0 entity-first 规则

### 关系规则

本仓库不使用数据库外键。

所以：

- `has_many` / `has_many, via = "..."` 用来声明关系导航
- `belongs_to` 仍然可以写，但默认加 `skip_fk`

示例：

```rust
#[sea_orm(belongs_to, from = "user_id", to = "id", skip_fk)]
pub user: Option<super::sys_user::Entity>,
```

这样可以：

- 保留 SeaORM 的 relation / join / find_also_related 能力
- 避免 schema sync 在数据库里创建外键

### 命名与重命名规则

- 只是 Rust 字段名想更短：用 `column_name = "..."`
- 真正想改数据库列名：必须用 `renamed_from = "..."`

不要直接改字段名后指望 sync 猜到“这是重命名”。

## schema sync 能做什么

在当前项目里，把它理解成“补结构”，不是完整 diff 引擎。

### 通常会做

- 新增表
- 新增字段
- `renamed_from` 驱动的列重命名
- 新增普通索引
- 新增唯一索引 / 组合唯一键
- 新增外键（但本项目通过 `skip_fk` 禁掉了）

### 不要指望它做

- 删除表
- 删除字段
- 删除外键
- 自动同步注释
- 自动同步字段类型变化
- 自动同步可空/非空变化
- 自动同步默认值变化

这类变更要走明确 SQL / migration。

## Entity 模式

本仓库实体常见特征：

- `#[sea_orm::model]`
- `DeriveEntityModel`
- 业务枚举用 `DeriveActiveEnum`
- `Serialize` / `Deserialize`
- 需要时加 `JsonSchema`
- 时间戳逻辑写在 `entity` 层的 `ActiveModelBehavior`

## DTO 模式

### Create DTO

Create DTO 负责：

- 参数校验
- 默认值收口
- 转 `ActiveModel`

```rust
impl CreateUserDto {
    pub fn into_active_model(
        self,
        hashed_password: String,
        operator: String,
    ) -> sys_user::ActiveModel {
        sys_user::ActiveModel {
            user_name: Set(self.user_name),
            password: Set(hashed_password),
            create_by: Set(operator.clone()),
            update_by: Set(operator),
            ..Default::default()
        }
    }
}
```

### Update DTO

Update DTO 负责：

- 只处理可更新字段
- 通过 `apply_to()` 写入 `ActiveModel`

### Query DTO

Query DTO 优先转成 `Condition`，这样 service 里可以直接：

```rust
sys_user::Entity::find().filter(query)
```

## VO 模式

VO 是前端契约，不必和 entity 一模一样。

当前常见做法：

- `#[serde(rename_all = "camelCase")]`
- 用 `from_model()` 做展示转换
- 需要时把枚举转成前端友好的文本
- 时间字段走统一 serializer

## 新增实体时的最小流程

1. 决定 raw entity 是手写还是生成到 `entity_gen`
2. 在 `entity` 层补 `ActiveModelBehavior` 和稳定扩展
3. DTO 负责输入和校验
4. VO 负责输出契约
5. Query DTO 负责 `Condition`
6. 若开启 schema sync，确认本次变更属于“可补结构”的范围

## 反模式

- 不要在业务代码里直接 import `entity_gen`
- 不要在实体里建立数据库外键
- 不要把前端展示字段塞回 entity
- 不要让 router 直接操作 `ActiveModel`
- 不要把危险 schema 变更全交给 sync
