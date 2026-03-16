# showcase_profile 自动生成代码 - 错误清单

> **模块**: 展示档案 (showcase_profile)
> **生成工具**: summerrs_admin MCP `generate_frontend_bundle_from_table`
> **检查时间**: 2026-03-15
> **总错误数**: ESLint 200 errors / TypeScript 0 errors
> **状态**: 需要后端代码生成器修复模板后重新生成

---

## 一、错误总览

| 分类 | 数量 | 严重级别 | 谁来修 |
|------|------|----------|--------|
| Prettier 格式化 (多余空行/空格) | 199 | Low | 后端生成器模板 |
| 空接口 `@typescript-eslint/no-empty-object-type` | 1 | Medium | 后端生成器模板 |

---

## 二、详细错误说明

### 错误 1: 类型定义文件全局多余空行

**文件**: `src/types/api/showcase-profile.d.ts`

**问题**: 生成器在每个字段之间插入了 2-3 个空行，不符合 Prettier 规范（应为最多 1 个空行）。

**错误示例** (当前生成的代码):
```typescript
interface ShowcaseProfileVo {

      // <- 多余空行
      /** 主键 */

      id: number     // <- 字段前多余空行


      /** 展示编码 */

      showcaseCode: string  // <- 字段前多余空行
}
```

**期望代码**:
```typescript
interface ShowcaseProfileVo {
  /** 主键 */
  id: number
  /** 展示编码 */
  showcaseCode: string
}
```

**涉及接口**: `ShowcaseProfileVo`, `ShowcaseProfileSearchParams`, `CreateShowcaseProfileParams`, `UpdateShowcaseProfileParams` — 所有接口都有此问题。

**受影响行数**: 约 80+ 处

---

### 错误 2: 空接口继承

**文件**: `src/types/api/showcase-profile.d.ts:128`

**ESLint 规则**: `@typescript-eslint/no-empty-object-type`

**问题**: `ShowcaseProfileDetailVo` 是空接口，直接继承 `ShowcaseProfileVo` 但没有添加任何额外字段。

**当前代码**:
```typescript
interface ShowcaseProfileDetailVo extends ShowcaseProfileVo {}
```

**修复方案** (二选一):
```typescript
// 方案 A: 使用 type alias
type ShowcaseProfileDetailVo = ShowcaseProfileVo

// 方案 B: 如果详情确实会有额外字段，补充字段
interface ShowcaseProfileDetailVo extends ShowcaseProfileVo {
  // 详情独有字段...
}
```

---

### 错误 3: API 文件函数参数格式

**文件**: `src/api/showcase-profile.ts:4, :19`

**ESLint 规则**: `prettier/prettier`

**问题**: 函数参数过长时未自动换行。

**当前代码**:
```typescript
export function fetchGetShowcaseProfileList(params: Api.ShowcaseProfile.ShowcaseProfileSearchParams) {
```

**期望代码**:
```typescript
export function fetchGetShowcaseProfileList(
  params: Api.ShowcaseProfile.ShowcaseProfileSearchParams
) {
```

---

### 错误 4: Vue 页面文件多余空行

**文件**: `src/views/system/showcase-profile/index.vue`

**问题**: `<script>` 中 SearchFormModel 类型定义、searchForm 初始化、columnsFactory 等多处存在多余空行，与 Prettier 规范不符。

**错误数量**: 约 30+ 处

**典型位置**:
- 类型定义中字段之间 (行 68-104)
- searchForm 初始值中字段之间 (行 110-126)
- columnsFactory 列配置中字段之间 (行 160-498)
- excludeParams 数组后多余逗号换行 (行 158-159)

---

### 错误 5: 搜索组件多余空行

**文件**: `src/views/system/showcase-profile/modules/showcase-profile-search.vue`

**问题**: 与 index.vue 相同的多余空行问题，分布在类型定义和 formItems 配置中。

**错误数量**: 约 40+ 处

---

### 错误 6: 弹窗组件多余空行

**文件**: `src/views/system/showcase-profile/modules/showcase-profile-dialog.vue`

**问题**: FormModel 类型定义、createDefaultForm 函数、initForm 函数、handleSubmit payload 构建等处均有多余空行。

**错误数量**: 约 50+ 处

---

## 三、根因分析

所有 199 个 `prettier/prettier` 错误的根本原因相同:

> **代码生成器模板中，字段之间使用了 `\n\n` (双空行) 作为分隔符，而项目 Prettier 配置不允许连续空行。**

生成器模板需要统一将字段分隔符从 `\n\n` 改为 `\n`。

---

## 四、需要后端生成器修复的模板位置

| 模板输出目标 | 问题 | 修复方式 |
|-------------|------|---------|
| `types/api/*.d.ts` 接口字段间 | 双空行 → 单空行 | 模板中字段循环的 separator 去掉多余 `\n` |
| `types/api/*.d.ts` 空接口 | `interface X extends Y {}` | 改用 `type X = Y` |
| `api/*.ts` 函数签名 | 长参数未换行 | 参数超长时添加换行 |
| `views/**/index.vue` script 内 | 类型定义/配置项间双空行 | 同 d.ts 修复 |
| `views/**/modules/*-search.vue` | 类型定义/formItems 间双空行 | 同上 |
| `views/**/modules/*-dialog.vue` | FormModel/表单逻辑间双空行 | 同上 |

---

## 五、临时解决方案

在后端修复生成器模板之前，前端可以执行以下命令自动修复所有格式问题:

```bash
npx eslint --fix src/views/system/showcase-profile/ src/api/showcase-profile.ts src/types/api/showcase-profile.d.ts
```

> 注意: 这只是临时修复。如果后端重新生成代码，相同的格式问题会再次出现。根本解决方案是修复生成器模板。

---

## 六、附录 - 涉及文件清单

| 文件路径 | 错误数 |
|---------|--------|
| `src/types/api/showcase-profile.d.ts` | ~90 |
| `src/views/system/showcase-profile/modules/showcase-profile-dialog.vue` | ~50 |
| `src/views/system/showcase-profile/modules/showcase-profile-search.vue` | ~40 |
| `src/views/system/showcase-profile/index.vue` | ~18 |
| `src/api/showcase-profile.ts` | 2 |
| **合计** | **200** |