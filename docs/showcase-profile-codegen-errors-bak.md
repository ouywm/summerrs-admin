# showcase_profile 自动生成代码 - 问题清单

> **模块**: 展示档案 (showcase_profile)
> **生成工具**: summerrs_admin MCP `generate_frontend_bundle_from_table`
> **检查时间**: 2026-03-15
> **状态**: 格式问题已前端临时修复，生成器独有问题需后端修复模板

---

## 问题总览

| # | 问题 | 严重级别 | 分类 | 状态 |
|---|------|---------|------|------|
| 1 | Create/Update DTO 包含不应由前端提交的审计字段 | **High** | 类型设计 | 待修复 |
| 2 | 列配置同时设置 `width` 和 `minWidth`，语义冲突 | **Medium** | UI 问题 | 待修复 |
| 3 | `metadata` 类型 `unknown \| null` 等价于 `unknown` | **Low** | 类型设计 | 待修复 |
| 4 | Prettier 格式化（多余空行/空格）| **Low** | 格式 | 已修复 |
| 5 | 空接口 `ShowcaseProfileDetailVo extends ShowcaseProfileVo {}` | **Low** | 格式 | 已修复 |

> **说明**: 以下模式经对比 user、role、dict 等现有模块，确认为**项目统一写法**，不计入生成器问题：
> - Dialog 双 watch（role 模块同样使用）
> - `validate(async (valid) => {})` 回调模式（dict-type、dict-data、user 均如此）
> - SearchFormModel 父子组件重复定义（项目统一做法）
> - defaultAvatar 兜底导致的 `if (!src)` 死代码（user 模块相同写法）
> - initForm 无竞态保护（属于双 watch 的延伸问题，项目统一模式）

---

## High 级别

### 问题 1: Create/Update DTO 包含不应由前端提交的审计字段

**文件**: `src/types/api/showcase-profile.d.ts`

**问题**: `CreateShowcaseProfileParams` 和 `UpdateShowcaseProfileParams` 中包含了 `createdAt` 和 `updatedAt` 字段：

```typescript
interface CreateShowcaseProfileParams {
  // ... 业务字段 ...
  createdAt?: string    // ← 应由后端自动生成
  updatedAt?: string    // ← 应由后端自动生成
}
```

**对比**: 项目中 user、role、dict 模块的 Create/Update DTO 均**不包含**这两个审计字段。这是生成器独有的问题。

**风险**: 虽然标记为 `optional` 不会导致编译报错，但如果前端意外传值可能覆盖后端自动维护的时间戳数据。

**修复建议**: 生成器模板应将 `createdAt`、`updatedAt` 等审计字段从 Create/Update DTO 中排除。

---

## Medium 级别

### 问题 2: 列配置同时设置 `width` 和 `minWidth`，语义冲突

**文件**: `src/views/system/showcase-profile/index.vue`

以下列同时设置了 `width` 和 `minWidth`：

| 列 | width | minWidth | 问题 |
|----|-------|----------|------|
| avatar (行 155-157) | 96 | 140 | minWidth > width，width 无效 |
| coverImage (行 176-178) | 96 | 140 | 同上 |
| officialUrl (行 329-331) | 220 | 220 | 重复设置，冗余 |

**对比**: 项目中 user、role 模块的列配置只设置 `width` 或 `minWidth` 其中之一，不会同时设置。

**修复建议**: 图片列使用 `width` 即可（固定宽度），删除 `minWidth`。

---

## Low 级别

### 问题 3: `metadata` 类型 `unknown | null` 等价于 `unknown`

**文件**: `src/types/api/showcase-profile.d.ts:91`

```typescript
metadata: unknown | null
```

TypeScript 中 `unknown | null` 等价于 `unknown`，因为 `unknown` 已经是所有类型的超类型（包含 null）。`| null` 是冗余的。

**修复建议**: 如果 metadata 是 JSON 对象，建议使用 `Record<string, unknown> | null`。如果确实是任意类型，直接用 `unknown`。

---

## 已修复的格式问题（仅供参考）

### 已修复 4: Prettier 格式化（199 个错误）

已通过 `npx eslint --fix` 自动修复。根因是生成器模板在字段之间使用了双空行 `\n\n`。

### 已修复 5: 空接口继承

已手动将 `interface ShowcaseProfileDetailVo extends ShowcaseProfileVo {}` 改为 `type ShowcaseProfileDetailVo = ShowcaseProfileVo`。

---

## 需要后端生成器修复的模板汇总

| 优先级 | 模板位置 | 问题 | 修复方式 |
|--------|---------|------|---------|
| **High** | d.ts 模板 Create/Update DTO | 包含 createdAt/updatedAt | 排除审计字段 |
| **Medium** | index.vue 列配置模板 | width/minWidth 同时输出 | 图片列只输出 width，不要同时输出 minWidth |
| **Low** | d.ts 模板 | `unknown \| null` 冗余 | JSON 字段用 `Record<string, unknown> \| null` |
| **Low** | 全部模板 | 字段间双空行 | 分隔符 `\n\n` → `\n` |
| **Low** | d.ts 模板 | 空接口继承 | DetailVo 改用 `type X = Y` |