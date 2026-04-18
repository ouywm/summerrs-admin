use sea_orm::{Statement, Values};

use super::ShardingConnectionInner;
use crate::{
    connector::statement::StatementContext,
    error::{Result, ShardingError},
    execute::RawStatementExecutor,
    lookup::{
        LookupDefinition, normalize_column, query_result_to_sharding_value,
        sharding_value_to_sea_value, split_qualified_name, update_assignment_value,
    },
    router::SqlOperation,
};

impl ShardingConnectionInner {
    pub(super) async fn resolve_lookup_sharding_conditions(
        &self,
        raw: &dyn RawStatementExecutor,
        analysis: &mut StatementContext,
    ) -> Result<()> {
        if matches!(
            analysis.operation,
            SqlOperation::Insert | SqlOperation::Other
        ) {
            return Ok(());
        }
        let Some(primary_table) = analysis.primary_table().cloned() else {
            return Ok(());
        };
        for index in self
            .config
            .lookup_indexes_for(primary_table.full_name().as_str())
        {
            if analysis
                .sharding_condition(index.sharding_column.as_str())
                .is_some()
            {
                continue;
            }
            let Some(lookup_value) = analysis
                .exact_condition_value(index.lookup_column.as_str())
                .cloned()
            else {
                continue;
            };
            let definition = LookupDefinition::from_config(index);
            self.lookup_index.register(definition.clone());
            let resolved = self
                .lookup_index
                .resolve(
                    primary_table.full_name().as_str(),
                    index.lookup_column.as_str(),
                    &lookup_value,
                )
                .or(self
                    .query_lookup_sharding_value(
                        raw,
                        &definition,
                        primary_table.schema.as_deref(),
                        &lookup_value,
                    )
                    .await?);
            if let Some(sharding_value) = resolved {
                analysis.sharding_conditions.insert(
                    normalize_column(index.sharding_column.as_str()),
                    crate::algorithm::ShardingCondition::Exact(sharding_value.clone()),
                );
                self.lookup_index.insert(
                    primary_table.full_name().as_str(),
                    index.lookup_column.as_str(),
                    &lookup_value,
                    sharding_value,
                );
            }
        }
        Ok(())
    }

    async fn query_lookup_sharding_value(
        &self,
        raw: &dyn RawStatementExecutor,
        definition: &LookupDefinition,
        fallback_schema: Option<&str>,
        lookup_value: &crate::algorithm::ShardingValue,
    ) -> Result<Option<crate::algorithm::ShardingValue>> {
        let datasource =
            self.lookup_datasource(definition.lookup_table.as_str(), fallback_schema)?;
        let backend = self
            .pool
            .connection(datasource.as_str())?
            .get_database_backend();
        let statement = Statement::from_sql_and_values(
            backend,
            definition.lookup_select_sql(),
            [sharding_value_to_sea_value(lookup_value)],
        );
        let row = raw.query_one_for(datasource.as_str(), statement).await?;
        Ok(row.and_then(|row| {
            query_result_to_sharding_value(&row, definition.sharding_column.as_str())
        }))
    }

    pub(super) async fn sync_lookup_table(
        &self,
        raw: &dyn RawStatementExecutor,
        analysis: &StatementContext,
        values: Option<&Values>,
    ) -> Result<()> {
        if !matches!(
            analysis.operation,
            SqlOperation::Insert | SqlOperation::Update | SqlOperation::Delete
        ) {
            return Ok(());
        }
        let Some(primary_table) = analysis.primary_table().cloned() else {
            return Ok(());
        };
        for index in self
            .config
            .lookup_indexes_for(primary_table.full_name().as_str())
        {
            let definition = LookupDefinition::from_config(index);
            self.lookup_index.register(definition.clone());
            match analysis.operation {
                SqlOperation::Insert => {
                    let lookup_values = analysis.insert_values(index.lookup_column.as_str());
                    let sharding_values = analysis.insert_values(index.sharding_column.as_str());
                    for (lookup_value, sharding_value) in
                        lookup_values.iter().zip(sharding_values.iter())
                    {
                        self.upsert_lookup_entry(
                            raw,
                            &primary_table,
                            &definition,
                            lookup_value,
                            sharding_value,
                        )
                        .await?;
                    }
                }
                SqlOperation::Update => {
                    let old_lookup_value = analysis
                        .exact_condition_value(index.lookup_column.as_str())
                        .cloned();
                    let next_lookup_value = update_assignment_value(
                        &analysis.ast,
                        values,
                        index.lookup_column.as_str(),
                    );
                    let mut sharding_value = update_assignment_value(
                        &analysis.ast,
                        values,
                        index.sharding_column.as_str(),
                    )
                    .or_else(|| {
                        analysis
                            .exact_condition_value(index.sharding_column.as_str())
                            .cloned()
                    });
                    if sharding_value.is_none()
                        && let Some(old_lookup) = old_lookup_value.as_ref()
                    {
                        sharding_value = self
                            .query_lookup_sharding_value(
                                raw,
                                &definition,
                                primary_table.schema.as_deref(),
                                old_lookup,
                            )
                            .await?;
                    }

                    if let (Some(old_lookup), Some(next_lookup)) =
                        (old_lookup_value.as_ref(), next_lookup_value.as_ref())
                        && old_lookup != next_lookup
                    {
                        self.delete_lookup_entry(raw, &primary_table, &definition, old_lookup)
                            .await?;
                    }

                    if let (Some(lookup_value), Some(sharding_value)) = (
                        next_lookup_value.or(old_lookup_value),
                        sharding_value.as_ref(),
                    ) {
                        self.upsert_lookup_entry(
                            raw,
                            &primary_table,
                            &definition,
                            &lookup_value,
                            sharding_value,
                        )
                        .await?;
                    }
                }
                SqlOperation::Delete => {
                    let Some(lookup_value) = analysis
                        .exact_condition_value(index.lookup_column.as_str())
                        .cloned()
                    else {
                        continue;
                    };
                    self.delete_lookup_entry(raw, &primary_table, &definition, &lookup_value)
                        .await?;
                }
                _ => {}
            }
        }
        Ok(())
    }

    async fn upsert_lookup_entry(
        &self,
        raw: &dyn RawStatementExecutor,
        primary_table: &crate::router::QualifiedTableName,
        definition: &LookupDefinition,
        lookup_value: &crate::algorithm::ShardingValue,
        sharding_value: &crate::algorithm::ShardingValue,
    ) -> Result<()> {
        let datasource = self.lookup_datasource(
            definition.lookup_table.as_str(),
            primary_table.schema.as_deref(),
        )?;
        let backend = self
            .pool
            .connection(datasource.as_str())?
            .get_database_backend();
        let statement = Statement::from_sql_and_values(
            backend,
            definition.lookup_upsert_sql(),
            [
                sharding_value_to_sea_value(lookup_value),
                sharding_value_to_sea_value(sharding_value),
            ],
        );
        raw.execute_for(datasource.as_str(), statement).await?;
        self.lookup_index.insert(
            primary_table.full_name().as_str(),
            definition.lookup_column.as_str(),
            lookup_value,
            sharding_value.clone(),
        );
        Ok(())
    }

    async fn delete_lookup_entry(
        &self,
        raw: &dyn RawStatementExecutor,
        primary_table: &crate::router::QualifiedTableName,
        definition: &LookupDefinition,
        lookup_value: &crate::algorithm::ShardingValue,
    ) -> Result<()> {
        let datasource = self.lookup_datasource(
            definition.lookup_table.as_str(),
            primary_table.schema.as_deref(),
        )?;
        let backend = self
            .pool
            .connection(datasource.as_str())?
            .get_database_backend();
        let statement = Statement::from_sql_and_values(
            backend,
            definition.lookup_delete_sql(),
            [sharding_value_to_sea_value(lookup_value)],
        );
        raw.execute_for(datasource.as_str(), statement).await?;
        self.lookup_index.remove(
            primary_table.full_name().as_str(),
            definition.lookup_column.as_str(),
            lookup_value,
        );
        Ok(())
    }

    fn lookup_datasource(
        &self,
        lookup_table: &str,
        fallback_schema: Option<&str>,
    ) -> Result<String> {
        let (schema, _) = split_qualified_name(lookup_table);
        let schema = schema.or_else(|| fallback_schema.map(str::to_string));
        schema
            .as_deref()
            .and_then(|schema| self.config.schema_primary_datasource(schema))
            .or_else(|| self.config.default_datasource_name())
            .ok_or_else(|| ShardingError::Route("default datasource is not configured".to_string()))
            .map(str::to_string)
    }
}
