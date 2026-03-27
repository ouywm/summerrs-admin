use regex::Regex;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GhostTableNames {
    pub ghost_table: String,
    pub old_table: String,
    pub slot: String,
    pub publication: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GhostTablePlan {
    pub snapshot_statements: Vec<String>,
    pub catch_up_statements: Vec<String>,
    pub cutover_statements: Vec<String>,
    pub cleanup_statements: Vec<String>,
}

impl GhostTablePlan {
    pub fn flatten(self) -> Vec<String> {
        let mut all = Vec::new();
        all.extend(self.snapshot_statements);
        all.extend(self.catch_up_statements);
        all.extend(self.cutover_statements);
        all.extend(self.cleanup_statements);
        all
    }
}

#[derive(Debug, Clone, Default)]
pub struct GhostTablePlanner;

impl GhostTablePlanner {
    pub fn plan_staged(&self, table: &str, alter_sql: &str, batch_size: usize) -> GhostTablePlan {
        let (_schema, table_name) = split_qualified_table_name(table);
        let names = ghost_table_names(table);

        GhostTablePlan {
            snapshot_statements: vec![
                format!("CREATE TABLE {} (LIKE {table} INCLUDING ALL)", names.ghost_table),
                rewrite_alter_table_target(alter_sql, names.ghost_table.as_str()),
                format!(
                    "INSERT INTO {} SELECT * FROM {table} WHERE id BETWEEN :start AND :end /* batch_size={batch_size} */",
                    names.ghost_table
                ),
            ],
            catch_up_statements: vec![
                format!("ALTER TABLE {table} REPLICA IDENTITY FULL"),
                format!(
                    "SELECT CASE WHEN EXISTS (SELECT 1 FROM pg_replication_slots WHERE slot_name = '{}') THEN '{}' ELSE (SELECT slot_name FROM pg_create_logical_replication_slot('{}', 'pgoutput')) END",
                    names.slot, names.slot, names.slot
                ),
                format!("DROP PUBLICATION IF EXISTS {}", names.publication),
                format!("CREATE PUBLICATION {} FOR TABLE {table}", names.publication),
            ],
            cutover_statements: vec![
                format!("LOCK TABLE {table} IN ACCESS EXCLUSIVE MODE"),
                format!("ALTER TABLE {table} RENAME TO {}__old", table_name),
                format!("ALTER TABLE {} RENAME TO {table_name}", names.ghost_table),
            ],
            cleanup_statements: vec![
                format!("DROP PUBLICATION IF EXISTS {}", names.publication),
                format!(
                    "SELECT pg_drop_replication_slot('{}') FROM pg_replication_slots WHERE slot_name = '{}'",
                    names.slot, names.slot
                ),
                format!("DROP TABLE {}", names.old_table),
            ],
        }
    }

    pub fn plan(&self, table: &str, alter_sql: &str, batch_size: usize) -> Vec<String> {
        self.plan_staged(table, alter_sql, batch_size).flatten()
    }
}

#[cfg(test)]
mod tests {
    use crate::ddl::GhostTablePlanner;

    #[test]
    fn ghost_planner_generates_swap_sequence() {
        let planner = GhostTablePlanner;
        let steps = planner.plan("ai.log", "ALTER TABLE ai.log ADD COLUMN extra text", 1000);
        assert_eq!(steps.len(), 13);
        assert!(steps[0].contains("ai.log__ghost"));
        assert!(steps[2].contains("batch_size=1000"));
    }

    #[test]
    fn ghost_planner_generates_staged_plan() {
        let planner = GhostTablePlanner;
        let plan = planner.plan_staged("ai.log", "ALTER TABLE ai.log ADD COLUMN extra text", 1000);
        assert_eq!(plan.snapshot_statements.len(), 3);
        assert_eq!(plan.catch_up_statements.len(), 4);
        assert_eq!(plan.cutover_statements.len(), 3);
        assert_eq!(plan.cleanup_statements.len(), 3);
    }

    #[test]
    fn ghost_planner_generates_replication_setup_statements() {
        let planner = GhostTablePlanner;
        let plan = planner.plan_staged("ai.log", "ALTER TABLE ai.log ADD COLUMN extra text", 1000);

        assert!(
            plan.catch_up_statements
                .iter()
                .any(|statement| statement.contains("pg_create_logical_replication_slot"))
        );
        assert!(
            plan.catch_up_statements
                .iter()
                .any(|statement| statement.contains("CREATE PUBLICATION"))
        );
        assert!(
            plan.catch_up_statements
                .iter()
                .all(|statement| !statement.contains("-- logical replication apply"))
        );
    }
}

fn split_qualified_table_name(table: &str) -> (Option<String>, String) {
    match table.rsplit_once('.') {
        Some((schema, table_name)) => (Some(schema.to_string()), table_name.to_string()),
        None => (None, table.to_string()),
    }
}

fn qualify_table_name(schema: Option<&str>, table: &str) -> String {
    match schema {
        Some(schema) => format!("{schema}.{table}"),
        None => table.to_string(),
    }
}

pub(crate) fn ghost_table_names(table: &str) -> GhostTableNames {
    let (schema, table_name) = split_qualified_table_name(table);
    let ghost_name = format!("{table_name}__ghost");
    let old_name = format!("{table_name}__old");
    let replication_key = sanitize_identifier(format!("{}_ddl", table.replace('.', "_")));
    GhostTableNames {
        ghost_table: qualify_table_name(schema.as_deref(), ghost_name.as_str()),
        old_table: qualify_table_name(schema.as_deref(), old_name.as_str()),
        slot: format!("{replication_key}_slot"),
        publication: format!("{replication_key}_pub"),
    }
}

fn rewrite_alter_table_target(alter_sql: &str, target_table: &str) -> String {
    Regex::new(r"(?i)^(\s*ALTER\s+TABLE\s+(?:ONLY\s+)?)\S+")
        .expect("valid alter table regex")
        .replace(alter_sql, format!("${{1}}{target_table}"))
        .into_owned()
}

fn sanitize_identifier(value: String) -> String {
    let mut sanitized = value
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect::<String>();
    if sanitized.is_empty() {
        sanitized.push('_');
    }
    if sanitized
        .chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_digit())
    {
        sanitized.insert(0, '_');
    }
    sanitized
}
