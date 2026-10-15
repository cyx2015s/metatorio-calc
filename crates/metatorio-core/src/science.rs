//! 科研包识别与容量：普通物品用 science_capacity，tool 用 durability。
use crate::IdWithQuality;
use metatorio_data::store::{PrototypeGroup, PrototypeStore};
use metatorio_data::{ItemComponent, QualityComponent, ToolComponent};
use std::collections::BTreeSet;

pub fn science_pack_names(store: &PrototypeStore) -> &BTreeSet<String> {
    store.science_pack_names()
}

/// 每个包可提供的科研单位；不叠加实验室品质、速度或产能。
pub fn science_value(store: &PrototypeStore, item: &IdWithQuality) -> Option<f64> {
    let record = store.item(&item.id)?;
    let quality = store
        .get(PrototypeGroup::Quality, &item.quality)
        .and_then(|record| record.component::<QualityComponent>());
    let (capacity, multiplier) = if let Some(tool) = record.component::<ToolComponent>() {
        (
            tool.durability?,
            quality.map_or(1.0, |q| q.tool_durability_multiplier()),
        )
    } else {
        (
            record.component::<ItemComponent>()?.science_capacity,
            quality.map_or(1.0, |q| q.science_capacity_multiplier()),
        )
    };
    let value = capacity * multiplier;
    (value.is_finite() && value > 0.0).then_some(value)
}
