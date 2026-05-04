#!/bin/bash

# 按正确顺序合并所有 JSON 文件
# 顺序：仪表盘、模版中心、组件中心、功能示例、系统管理、文章管理、结果页面、异常页面、运维管理、官方文档、精简版本、v2.6.1版本、更新日志、调度

cd "$(dirname "$0")"

# 创建临时文件
temp_file="all_routes.json"

# 使用 jq 合并所有 JSON 文件（扁平化数组）
jq -s 'add' \
  dashboard.json \
  template.json \
  widgets.json \
  examples.json \
  system.json \
  article.json \
  result.json \
  exception.json \
  safeguard.json \
  help.json \
  scheduler.json \
  > "$temp_file"

echo "✅ 已生成合并的 JSON 文件: $temp_file"
echo "现在可以运行: cargo run --bin route_to_sql sql/sys/menu_data/$temp_file 1 sql/sys/menu_data_all.sql"
