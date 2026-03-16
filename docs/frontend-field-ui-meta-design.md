# `field_ui_meta` 组件协议说明

本文档定义 `generate_frontend_page_from_table` / `generate_frontend_bundle_from_table` 的稳定输入协议 `field_ui_meta`。

目标只有一个：让 AI 只决定字段组件语义，不接管页面布局。

## 适用范围

本协议只控制三个区域：

- 搜索区字段组件
- 表单区字段组件
- 表格区字段展示方式

本协议不负责：

- 页面布局
- 弹窗 / 抽屉选择
- 字段顺序
- 字段是否进入搜索区 / 表单区 / 表格区
- 任意 Vue 代码注入

这些仍由生成器默认策略或 `search_fields` / `table_fields` / `form_fields` 控制。

## 输入结构

```json
{
  "field_ui_meta": {
    "field_name": {
      "dict_type": "user_status",
      "semantic": "plain",
      "search": {
        "component": "select",
        "placeholder": "请选择状态",
        "props": {
          "clearable": true
        }
      },
      "form": {
        "component": "select",
        "required": true,
        "placeholder": "请选择状态",
        "props": {
          "clearable": true
        }
      },
      "table": {
        "component": "dict_tag"
      }
    }
  }
}
```

字段说明：

- `dict_type`
  绑定字典类型编码。
- `semantic`
  覆盖字段语义推断。
- `search.component`
  指定搜索区组件。
- `search.placeholder`
  指定搜索区占位文案。
- `search.props`
  透传给搜索组件的 JSON 对象。
- `form.component`
  指定表单区组件。
- `form.required`
  覆盖表单必填规则。
- `form.placeholder`
  指定表单区占位文案。
- `form.props`
  透传给表单组件的 JSON 对象。
- `table.component`
  指定表格展示组件。

## 优先级

字段最终生效顺序如下：

1. `field_ui_meta`
2. `field_hints`
3. `dict_bindings`
4. 生成器自动推断

说明：

- `field_ui_meta` 是正式协议。
- `field_hints` 只作为兼容层保留。
- `dict_bindings` 仍可单独使用，但优先级低于 `field_ui_meta.dict_type`。

## 边界

`field_ui_meta` 只控制“怎么渲染”，不控制“渲染到哪里”。

例如：

- 某字段写了 `search.component`，不代表它一定会进入搜索区。
- 是否进入搜索区，仍取决于 `search_fields` 或生成器默认选择。
- 表单使用 `dialog` 还是 `drawer`，由生成器决定，不由 `field_ui_meta` 指定。

## 稳定支持的语义枚举

`semantic` 当前稳定支持：

- `plain`
- `password`
- `email`
- `phone`
- `url`
- `avatar`
- `image`
- `file`
- `icon`
- `rich_text`

用途：

- 影响默认组件推断
- 影响表格展示方式
- 影响上传控件默认行为

## 稳定支持的搜索组件

`search.component` 当前稳定支持：

- `input`
- `number`
- `select`
- `radio_group`
- `checkbox_group`
- `cascader`
- `tree_select`
- `date`
- `date_range`
- `time`
- `date_time`
- `date_time_range`

说明：

- `radio_group` / `checkbox_group` 适合枚举、布尔、字典选项字段。
- `date_range` / `date_time_range` 会映射为 `xxxStart` / `xxxEnd` 查询参数。
- `cascader` / `tree_select` 只定义协议输入，不负责构造复杂树结构，所需数据仍需通过 `props` 或后续前端实现提供。

## 稳定支持的表单组件

`form.component` 当前稳定支持：

- `input`
- `password`
- `textarea`
- `switch`
- `select`
- `radio_group`
- `checkbox_group`
- `cascader`
- `tree_select`
- `input_number`
- `date`
- `time`
- `date_time`
- `editor`
- `image_upload`
- `file_upload`

说明：

- `editor` 当前映射到 `ArtWangEditor`。
- `image_upload` / `file_upload` 当前映射到 `ArtFileUpload`。
- `textarea` 仍是普通多行输入，不会自动升级为富文本。

## 稳定支持的表格展示组件

`table.component` 当前稳定支持：

- `text`
- `boolean_tag`
- `dict_tag`
- `local_tag`
- `image`
- `link`

说明：

- `dict_tag` 依赖 `dict_type`。
- `local_tag` 依赖本地枚举选项。
- `image` 适合头像、图片字段。
- `link` 适合 URL / 邮箱类字段。

## `props` 透传规则

`search.props` 和 `form.props` 必须是 JSON 对象。

生成器规则：

- 会先生成默认 props
- 再把 `props` 透传对象合并进去
- 如果有同名键，以 `props` 中的值为准

这意味着你可以用 `props` 覆盖默认值，例如：

- `rows`
- `clearable`
- `multiple`
- `checkStrictly`
- `height`
- `valueFormat`

## 推荐用法

### 1. 字典状态字段

```json
{
  "field_ui_meta": {
    "status": {
      "dict_type": "user_status",
      "search": { "component": "select" },
      "form": { "component": "select" },
      "table": { "component": "dict_tag" }
    }
  }
}
```

### 2. 图片字段

```json
{
  "field_ui_meta": {
    "avatar": {
      "semantic": "avatar",
      "form": {
        "component": "image_upload",
        "props": {
          "buttonText": "上传头像"
        }
      },
      "table": {
        "component": "image"
      }
    }
  }
}
```

### 3. 枚举性别字段

```json
{
  "field_ui_meta": {
    "gender": {
      "search": {
        "component": "radio_group"
      },
      "form": {
        "component": "radio_group"
      },
      "table": {
        "component": "local_tag"
      }
    }
  }
}
```

### 4. 富文本字段

```json
{
  "field_ui_meta": {
    "content": {
      "semantic": "rich_text",
      "form": {
        "component": "editor",
        "props": {
          "height": "320px"
        }
      },
      "table": {
        "component": "text"
      }
    }
  }
}
```

## 当前不支持

以下能力不在当前稳定协议内：

- `remote_select`
- 直接传 Vue 模板片段
- 直接传渲染函数
- 控制整页布局
- 控制弹窗 / 抽屉形态
- 控制字段排序

原因很简单：

- 这些能力目前没有统一前端落点，或者会明显破坏生成结果稳定性。

## 与旧参数的关系

当前调用仍可同时传：

- `dict_bindings`
- `field_hints`
- `search_fields`
- `table_fields`
- `form_fields`

建议：

- 新调用优先使用 `field_ui_meta`
- `field_hints` 仅用于兼容旧调用
- 字段集合继续用 `search_fields` / `table_fields` / `form_fields` 显式约束

## 结论

`field_ui_meta` 的稳定定位是：

- AI 决定字段组件语义
- 生成器决定默认布局和页面结构
- 模板只负责消费归一化结果

这也是当前前端页面生成器对外推荐的正式输入方式。
