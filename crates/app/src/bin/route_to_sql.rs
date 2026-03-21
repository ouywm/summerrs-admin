use serde::{Deserialize, Serialize};
use std::fs;

#[derive(Debug, Deserialize, Serialize)]
struct RouteMeta {
    title: String,
    #[serde(default)]
    icon: String,
    #[serde(default, rename = "showBadge")]
    show_badge: bool,
    #[serde(default, rename = "showTextBadge")]
    show_text_badge: String,
    #[serde(default, rename = "isHide")]
    is_hide: bool,
    #[serde(default, rename = "isHideTab")]
    is_hide_tab: bool,
    #[serde(default)]
    link: String,
    #[serde(default, rename = "isIframe")]
    is_iframe: bool,
    #[serde(default, rename = "keepAlive")]
    keep_alive: bool,
    #[serde(default, rename = "fixedTab")]
    fixed_tab: bool,
    #[serde(default, rename = "activePath")]
    active_path: String,
    #[serde(default, rename = "isFullPage")]
    is_full_page: bool,
    #[serde(default, rename = "isFirstLevel")]
    is_first_level: bool,
    #[serde(default, rename = "authList")]
    auth_list: Vec<AuthItem>,
    #[serde(default)]
    roles: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct AuthItem {
    title: String,
    #[serde(rename = "authMark")]
    auth_mark: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct RouteRecord {
    name: String,
    path: String,
    #[serde(default)]
    component: String,
    #[serde(default)]
    redirect: String,
    meta: RouteMeta,
    #[serde(default)]
    children: Vec<RouteRecord>,
}

struct SqlGenerator {
    current_id: i64,
    statements: Vec<String>,
}

impl SqlGenerator {
    fn new(start_id: i64) -> Self {
        Self {
            current_id: start_id,
            statements: Vec::new(),
        }
    }

    fn generate(&mut self, routes: Vec<RouteRecord>) -> String {
        for (idx, route) in routes.into_iter().enumerate() {
            self.process_route(route, 0, idx as i32 + 1);
        }

        let mut result = String::new();
        result.push_str("-- ============================================================\n");
        result.push_str("-- 自动生成的菜单数据\n");
        result.push_str("-- ============================================================\n\n");
        result.push_str(&self.statements.join("\n\n"));
        result.push_str("\n\n-- 重置序列\n");
        result.push_str("SELECT setval('sys.menu_id_seq', (SELECT MAX(id) FROM sys.menu));\n");
        result
    }

    fn process_route(&mut self, route: RouteRecord, parent_id: i64, sort: i32) {
        let menu_id = self.current_id;
        self.current_id += 1;

        // 生成菜单 INSERT 语句
        let sql = self.generate_menu_insert(menu_id, parent_id, &route, sort);
        self.statements.push(sql);

        // 处理按钮权限
        if !route.meta.auth_list.is_empty() {
            for (idx, auth) in route.meta.auth_list.iter().enumerate() {
                let auth_id = self.current_id;
                self.current_id += 1;
                let auth_sql = self.generate_auth_insert(auth_id, menu_id, auth, idx as i32 + 1);
                self.statements.push(auth_sql);
            }
        }

        // 递归处理子路由
        for (idx, child) in route.children.into_iter().enumerate() {
            self.process_route(child, menu_id, idx as i32 + 1);
        }
    }

    fn generate_menu_insert(
        &self,
        id: i64,
        parent_id: i64,
        route: &RouteRecord,
        sort: i32,
    ) -> String {
        let component = if route.component.is_empty() {
            String::new()
        } else {
            route.component.clone()
        };

        format!(
            "INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, \
             is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, \
             show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) \
             VALUES ({}, {}, 1, '{}', '{}', '{}', '{}', '{}', '{}', '{}', {}, {}, {}, {}, {}, {}, {}, {}, '{}', '{}', '', '', {}, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP);",
            id,
            parent_id,
            escape_sql(&route.name),
            escape_sql(&route.path),
            escape_sql(&component),
            escape_sql(&route.redirect),
            escape_sql(&route.meta.icon),
            escape_sql(&route.meta.title),
            escape_sql(&route.meta.link),
            route.meta.is_iframe,
            route.meta.is_hide,
            route.meta.is_hide_tab,
            route.meta.is_full_page,
            route.meta.is_first_level,
            route.meta.keep_alive,
            route.meta.fixed_tab,
            route.meta.show_badge,
            escape_sql(&route.meta.show_text_badge),
            escape_sql(&route.meta.active_path),
            sort
        )
    }

    fn generate_auth_insert(&self, id: i64, parent_id: i64, auth: &AuthItem, sort: i32) -> String {
        format!(
            "INSERT INTO sys.menu (id, parent_id, menu_type, name, path, component, redirect, icon, title, link, \
             is_iframe, is_hide, is_hide_tab, is_full_page, is_first_level, keep_alive, fixed_tab, \
             show_badge, show_text_badge, active_path, auth_name, auth_mark, sort, enabled, create_time, update_time) \
             VALUES ({}, {}, 2, '', '', '', '', '', '{}', '', false, false, false, false, false, false, false, false, '', '', '{}', '{}', {}, true, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP);",
            id,
            parent_id,
            escape_sql(&auth.title),
            escape_sql(&auth.title),
            escape_sql(&auth.auth_mark),
            sort
        )
    }
}

fn escape_sql(s: &str) -> String {
    s.replace('\'', "''")
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("用法: route_to_sql <json文件路径> [起始ID] [输出sql路径]");
        eprintln!("示例: route_to_sql routes.json 1 output.sql");
        eprintln!("      route_to_sql routes.json 1  (输出到标准输出)");
        std::process::exit(1);
    }

    let json_path = &args[1];
    let start_id = if args.len() >= 3 {
        args[2].parse::<i64>().unwrap_or(1)
    } else {
        1
    };
    let output_file = if args.len() >= 4 {
        Some(&args[3])
    } else {
        None
    };

    // 读取 JSON 文件
    let content = fs::read_to_string(json_path).unwrap_or_else(|e| {
        eprintln!("读取文件失败: {}", e);
        std::process::exit(1);
    });

    // 解析 JSON
    let routes: Vec<RouteRecord> = serde_json::from_str(&content).unwrap_or_else(|e| {
        eprintln!("解析 JSON 失败: {}", e);
        std::process::exit(1);
    });

    // 生成 SQL
    let mut generator = SqlGenerator::new(start_id);
    let sql = generator.generate(routes);

    // 输出
    if let Some(output_path) = output_file {
        fs::write(output_path, &sql).unwrap_or_else(|e| {
            eprintln!("写入文件失败: {}", e);
            std::process::exit(1);
        });
        eprintln!(
            "✅ 生成成功: {} -> {} (起始ID: {})",
            json_path, output_path, start_id
        );
    } else {
        println!("{}", sql);
    }
}
