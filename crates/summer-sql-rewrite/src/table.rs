use sqlparser::ast::{Ident, ObjectName};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct QualifiedTableName {
    pub schema: Option<String>,
    pub table: String,
}

impl QualifiedTableName {
    pub fn parse(value: &str) -> Self {
        match value.rsplit_once('.') {
            Some((schema, table)) => Self {
                schema: Some(schema.to_string()),
                table: table.to_string(),
            },
            None => Self {
                schema: None,
                table: value.to_string(),
            },
        }
    }

    pub fn full_name(&self) -> String {
        match &self.schema {
            Some(schema) => format!("{schema}.{}", self.table),
            None => self.table.clone(),
        }
    }

    pub fn to_object_name(&self) -> ObjectName {
        match &self.schema {
            Some(schema) => ObjectName(
                schema
                    .split('.')
                    .filter(|value| !value.is_empty())
                    .map(Ident::new)
                    .chain(std::iter::once(Ident::new(&self.table)))
                    .collect(),
            ),
            None => ObjectName(vec![Ident::new(&self.table)]),
        }
    }

    pub fn matches_object_name(&self, name: &ObjectName) -> bool {
        match name.0.as_slice() {
            [table] => table.value.eq_ignore_ascii_case(self.table.as_str()),
            items if items.len() >= 2 => {
                let (schema_parts, table) = items.split_at(items.len() - 1);
                let schema = schema_parts
                    .iter()
                    .map(|item| item.value.as_str())
                    .collect::<Vec<_>>()
                    .join(".");
                self.schema
                    .as_deref()
                    .is_some_and(|value| value.eq_ignore_ascii_case(schema.as_str()))
                    && table[0].value.eq_ignore_ascii_case(self.table.as_str())
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use sqlparser::ast::{Ident, ObjectName};

    use super::QualifiedTableName;

    #[test]
    fn parse_uses_last_segment_as_table_for_multi_part_name() {
        let table = QualifiedTableName::parse("catalog.schema.orders");

        assert_eq!(table.schema.as_deref(), Some("catalog.schema"));
        assert_eq!(table.table, "orders");
        assert_eq!(table.full_name(), "catalog.schema.orders");
        assert_eq!(table.to_object_name().to_string(), "catalog.schema.orders");
    }

    #[test]
    fn matches_object_name_supports_multi_part_schema_prefix() {
        let table = QualifiedTableName::parse("catalog.schema.orders");
        let object = ObjectName(vec![
            Ident::new("catalog"),
            Ident::new("schema"),
            Ident::new("orders"),
        ]);

        assert!(table.matches_object_name(&object));
    }
}
