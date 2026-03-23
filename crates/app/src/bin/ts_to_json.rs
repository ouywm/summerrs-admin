use regex::Regex;
use serde_json::{Value, json};
use std::fs;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("用法: ts_to_json <ts文件路径> [输出json路径]");
        eprintln!("示例: ts_to_json route/modules/dashboard.ts output.json");
        eprintln!("      ts_to_json route/modules/dashboard.ts  (输出到标准输出)");
        std::process::exit(1);
    }

    let ts_file = &args[1];
    let output_file = if args.len() >= 3 {
        Some(&args[2])
    } else {
        None
    };

    // 读取 TS 文件
    let content = fs::read_to_string(ts_file).unwrap_or_else(|e| {
        eprintln!("读取文件失败: {}", e);
        std::process::exit(1);
    });

    // 清理内容
    let cleaned = clean_typescript(&content);

    // 提取路由对象
    match extract_route_object(&cleaned) {
        Ok(route_str) => {
            // 转换为 JSON
            match convert_to_json(&route_str) {
                Ok(json_value) => {
                    // 包装为数组
                    let result = json!([json_value]);
                    let json_str = serde_json::to_string_pretty(&result).unwrap();

                    // 输出
                    if let Some(output_path) = output_file {
                        fs::write(output_path, json_str).unwrap_or_else(|e| {
                            eprintln!("写入文件失败: {}", e);
                            std::process::exit(1);
                        });
                        eprintln!("✅ 转换成功: {} -> {}", ts_file, output_path);
                    } else {
                        println!("{}", json_str);
                    }
                }
                Err(e) => {
                    eprintln!("转换失败: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Err(e) => {
            eprintln!("提取路由对象失败: {}", e);
            std::process::exit(1);
        }
    }
}

fn clean_typescript(content: &str) -> String {
    let mut cleaned = content.to_string();

    // 移除 import 语句
    let import_re = Regex::new(r#"import\s+.*?from\s+['"].*?['"];?"#).unwrap();
    cleaned = import_re.replace_all(&cleaned, "").to_string();

    // 移除 export 关键字
    let export_re = Regex::new(r"export\s+(const|let|var)\s+").unwrap();
    cleaned = export_re.replace_all(&cleaned, "").to_string();

    // 移除类型注解
    let type_re = Regex::new(r":\s*AppRouteRecord(\[\])?\s*=").unwrap();
    cleaned = type_re.replace_all(&cleaned, "=").to_string();

    // 处理动态 import
    let import_comp_re = Regex::new(r"component:\s*\(\)\s*=>\s*import\([^)]+\)").unwrap();
    cleaned = import_comp_re
        .replace_all(&cleaned, r#"component: """#)
        .to_string();

    cleaned
}

fn extract_route_object(content: &str) -> Result<String, String> {
    // 匹配 变量名 = { ... } 或 变量名 = [ ... ]
    let re = Regex::new(r"(\w+)\s*=\s*(\{[\s\S]*\}|\[[\s\S]*\])\s*;?\s*$").unwrap();

    if let Some(caps) = re.captures(content)
        && let Some(obj) = caps.get(2)
    {
        return Ok(obj.as_str().to_string());
    }

    Err("无法找到路由对象".to_string())
}

fn convert_to_json(js_str: &str) -> Result<Value, String> {
    // 将 JavaScript 对象字面量转换为 JSON
    let mut json_str = js_str.to_string();

    // 处理单引号为双引号
    json_str = json_str.replace('\'', "\"");

    // 处理对象键名（添加引号）- 更精确的匹配
    // 匹配行首或逗号后的键名
    let key_re = Regex::new(r#"([\n,]\s*)([a-zA-Z_][a-zA-Z0-9_]*)\s*:"#).unwrap();
    json_str = key_re.replace_all(&json_str, r#"$1"$2":"#).to_string();

    // 处理对象开头的键名
    let obj_start_key_re = Regex::new(r#"(\{\s*)([a-zA-Z_][a-zA-Z0-9_]*)\s*:"#).unwrap();
    json_str = obj_start_key_re
        .replace_all(&json_str, r#"$1"$2":"#)
        .to_string();

    // 移除尾随逗号
    let trailing_comma_re = Regex::new(r",(\s*[}\]])").unwrap();
    json_str = trailing_comma_re.replace_all(&json_str, "$1").to_string();

    // 尝试解析 JSON
    serde_json::from_str(&json_str).map_err(|e| {
        // 输出调试信息
        eprintln!("转换后的字符串:\n{}", json_str);
        format!("JSON 解析失败: {}", e)
    })
}
