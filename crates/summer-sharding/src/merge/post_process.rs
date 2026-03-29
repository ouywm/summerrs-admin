use std::sync::Arc;

use sea_orm::QueryResult;

use crate::{
    config::{EncryptRuleConfig, MaskingRuleConfig, ShardingConfig},
    connector::{hint::should_skip_masking, statement::StatementContext},
    encrypt::{AesGcmEncryptor, EncryptAlgorithm},
    error::Result,
    masking,
    merge::row::{from_values, row_value_owned, value_as_string},
};

pub fn apply(
    rows: Vec<QueryResult>,
    analysis: &StatementContext,
    config: &ShardingConfig,
) -> Result<Vec<QueryResult>> {
    let encrypt_rules = matching_encrypt_rules(config, analysis);
    let masking_rules = matching_masking_rules(config, analysis);
    let skip_masking =
        should_skip_masking(analysis.hint.as_ref(), analysis.access_context.as_ref());

    if encrypt_rules.is_empty() && (masking_rules.is_empty() || skip_masking) {
        return Ok(rows);
    }

    let encryptors = encrypt_rules
        .iter()
        .map(|rule| {
            Ok((
                (*rule).clone(),
                Arc::new(AesGcmEncryptor::from_env(rule.key_env.as_str())?)
                    as Arc<dyn EncryptAlgorithm>,
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    let maskers = masking_rules
        .iter()
        .map(|rule| Ok(((*rule).clone(), masking::build_algorithm(rule)?)))
        .collect::<Result<Vec<_>>>()?;

    rows.into_iter()
        .map(|row| {
            let mut proxy_row = sea_orm::from_query_result_to_proxy_row(&row);

            for (rule, encryptor) in &encryptors {
                let source_value = row_value_owned(&proxy_row, rule.column.as_str())
                    .or_else(|| row_value_owned(&proxy_row, rule.cipher_column.as_str()));
                let Some(ciphertext) = source_value.as_ref().and_then(value_as_string) else {
                    continue;
                };
                let plaintext = encryptor.decrypt(ciphertext.as_str())?;
                proxy_row
                    .values
                    .insert(rule.column.clone(), sea_orm::Value::String(Some(plaintext)));
            }

            if !skip_masking {
                for (rule, masker) in &maskers {
                    let Some(plain) = row_value_owned(&proxy_row, rule.column.as_str())
                        .as_ref()
                        .and_then(value_as_string)
                    else {
                        continue;
                    };
                    proxy_row.values.insert(
                        rule.column.clone(),
                        sea_orm::Value::String(Some(masker.mask(plain.as_str()))),
                    );
                }
            }

            Ok(from_values(proxy_row.values))
        })
        .collect()
}

fn matching_encrypt_rules<'a>(
    config: &'a ShardingConfig,
    analysis: &StatementContext,
) -> Vec<&'a EncryptRuleConfig> {
    if !config.encrypt.enabled {
        return Vec::new();
    }
    config
        .encrypt
        .rules
        .iter()
        .filter(|rule| {
            analysis.tables.iter().any(|table| {
                rule.table.eq_ignore_ascii_case(table.full_name().as_str())
                    || rule.table.eq_ignore_ascii_case(table.table.as_str())
            })
        })
        .collect()
}

fn matching_masking_rules<'a>(
    config: &'a ShardingConfig,
    analysis: &StatementContext,
) -> Vec<&'a MaskingRuleConfig> {
    if !config.masking.enabled {
        return Vec::new();
    }
    config
        .masking
        .rules
        .iter()
        .filter(|rule| {
            analysis.tables.iter().any(|table| {
                rule.table.eq_ignore_ascii_case(table.full_name().as_str())
                    || rule.table.eq_ignore_ascii_case(table.table.as_str())
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use sea_orm::{DbBackend, QueryResult, Statement};

    use crate::{
        config::ShardingConfig,
        connector::{analyze_statement, hint::ShardingAccessContext},
        merge::post_process::apply,
    };

    fn make_row(phone: &str) -> QueryResult {
        sea_orm::ProxyRow::new(BTreeMap::from([(
            "phone".to_string(),
            sea_orm::Value::String(Some(phone.to_string())),
        )]))
        .into()
    }

    fn masking_config() -> ShardingConfig {
        ShardingConfig::from_test_str(
            r#"
            [datasources.ds_sys]
            uri = "mock://sys"
            schema = "sys"
            role = "primary"

            [masking]
            enabled = true

              [[masking.rules]]
              table = "sys.user"
              column = "phone"
              algorithm = "phone"
            "#,
        )
        .expect("config")
    }

    #[test]
    fn post_process_masks_without_privilege_context() {
        let config = masking_config();
        let stmt = Statement::from_string(DbBackend::Postgres, "SELECT phone FROM sys.user");
        let analysis = analyze_statement(&stmt).expect("analysis");

        let rows = apply(vec![make_row("13812341234")], &analysis, &config).expect("apply");
        assert_eq!(
            rows[0].try_get::<String>("", "phone").expect("phone"),
            "138****1234"
        );
    }

    #[test]
    fn post_process_skips_masking_with_access_context() {
        let config = masking_config();
        let stmt = Statement::from_string(DbBackend::Postgres, "SELECT phone FROM sys.user");
        let mut analysis = analyze_statement(&stmt).expect("analysis");
        analysis.access_context =
            Some(ShardingAccessContext::default().with_permission("masking:skip"));

        let rows = apply(vec![make_row("13812341234")], &analysis, &config).expect("apply");

        assert_eq!(
            rows[0].try_get::<String>("", "phone").expect("phone"),
            "13812341234"
        );
    }
}
