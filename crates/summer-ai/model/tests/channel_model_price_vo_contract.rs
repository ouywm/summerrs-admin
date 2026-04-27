use summer_ai_model::dto::channel_model_price::ChannelModelPriceConfig;
use summer_ai_model::entity::routing::channel_model_price::{
    self, ChannelModelPriceBillingMode, ChannelModelPriceStatus,
};
use summer_ai_model::entity::routing::channel_model_price_version::{
    self, ChannelModelPriceVersionStatus,
};
use summer_ai_model::vo::channel_model_price::{ChannelModelPriceVersionVo, ChannelModelPriceVo};

#[test]
fn channel_model_price_vo_uses_temporal_types() {
    let now = chrono::Utc::now().fixed_offset();
    let vo = ChannelModelPriceVo::from_model(channel_model_price::Model {
        id: 1,
        channel_id: 2,
        model_name: "gpt-4o-mini".into(),
        billing_mode: ChannelModelPriceBillingMode::ByToken,
        currency: "USD".into(),
        price_config: ChannelModelPriceConfig {
            input_per_million: "0.15".into(),
            output_per_million: "0.60".into(),
            cache_read_per_million: Some("0.075".into()),
            cache_write_per_million: None,
            reasoning_per_million: None,
        }
        .to_json_value(),
        reference_id: "ref-1".into(),
        status: ChannelModelPriceStatus::Enabled,
        remark: "official".into(),
        create_by: "creator".into(),
        create_time: now,
        update_by: "updater".into(),
        update_time: now,
    });

    assert_eq!(vo.create_time, now);
    assert_eq!(vo.update_time, now);
}

#[test]
fn channel_model_price_version_vo_uses_temporal_types() {
    let now = chrono::Utc::now().fixed_offset();
    let vo = ChannelModelPriceVersionVo::from_model(channel_model_price_version::Model {
        id: 1,
        channel_model_price_id: 2,
        channel_id: 3,
        model_name: "gpt-4o-mini".into(),
        version_no: 1,
        reference_id: "ref-1".into(),
        price_config: ChannelModelPriceConfig {
            input_per_million: "0.15".into(),
            output_per_million: "0.60".into(),
            cache_read_per_million: Some("0.075".into()),
            cache_write_per_million: None,
            reasoning_per_million: None,
        }
        .to_json_value(),
        effective_start_at: now,
        effective_end_at: Some(now),
        status: ChannelModelPriceVersionStatus::Effective,
        create_time: now,
    });

    assert_eq!(vo.effective_start_at, now);
    assert_eq!(vo.effective_end_at, Some(now));
    assert_eq!(vo.create_time, now);
}
