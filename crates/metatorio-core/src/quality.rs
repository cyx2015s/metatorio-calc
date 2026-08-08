//! 品质分布计算（迁移自 metatorio-egui `calc_quality_distribution`）。

use crate::context::Context;
use metatorio_data::generated_components::QualityComponent;
use metatorio_data::store::PrototypeGroup;

/// 按 order 排序的品质组件列表（0 = normal）。
pub(crate) fn sorted_qualities<'a>(ctx: &'a Context<'a>) -> Vec<&'a QualityComponent> {
    // 名字顺序由 PrototypeStore 惰性缓存（quality_order），避免每次调用重复排序
    ctx.prototype
        .quality_order()
        .iter()
        .filter_map(|name| {
            ctx.prototype
                .get(PrototypeGroup::Quality, name)
                .and_then(|r| r.component::<QualityComponent>())
        })
        .collect()
}

/// 计算品质分布：base_quality 的产品按 `quality_bonus` 升级/降级到各级品质的概率
/// （返回与品质列表等长的概率向量，和为 1）。
///
/// `quality_bonus > 0`：正向升级（沿 next 链，chain_probability）；
/// `quality_bonus < 0`：降级（沿 previous 链）。`maximum_quality` 钳制升级上限。
pub fn calc_quality_distribution(
    ctx: &Context,
    quality_bonus: f64,
    base_quality: usize,
    maximum_quality: usize,
) -> Vec<f64> {
    let qualities = sorted_qualities(ctx);
    if qualities.is_empty() {
        // 无品质原型（异常数据）：按 normal 单品质处理
        return vec![1.0];
    }
    let mut result = vec![0.0; qualities.len()];
    let base_quality = base_quality.min(qualities.len() - 1);
    let maximum_quality = maximum_quality.clamp(base_quality, qualities.len() - 1);
    if quality_bonus == 0.0 || !quality_bonus.is_finite() {
        result[base_quality] = 1.0;
        return result;
    }

    if quality_bonus > 0.0 && maximum_quality > base_quality {
        // Do not clamp this probability. Factorio lets the excess borrow
        // lower-quality results and pays that debt back from the bottom.
        let initial = qualities[base_quality].next_probability * quality_bonus;
        let mut cumulative = vec![0.0; qualities.len()];
        cumulative[base_quality] = initial;
        let mut chain = 1.0;
        for level in base_quality..maximum_quality {
            cumulative[level + 1] = cumulative[level] * chain;
            chain = qualities[level].chain_probability();
        }
        result.copy_from_slice(&cumulative);
        for level in base_quality + 1..qualities.len() {
            result[level - 1] -= result[level];
        }
        settle_upgrade_distribution(&mut result, base_quality);
    } else if quality_bonus < 0.0 && base_quality > 0 {
        let initial = qualities[base_quality].previous_probability * quality_bonus.abs();
        let mut cumulative = vec![0.0; qualities.len()];
        cumulative[base_quality] = initial;
        let mut chain = 1.0;
        for level in (1..=base_quality).rev() {
            cumulative[level - 1] = cumulative[level] * chain;
            chain = qualities[level].previous_chain_probability();
        }
        result.copy_from_slice(&cumulative);
        for level in (1..=base_quality).rev() {
            result[level] -= result[level - 1];
        }
        settle_downgrade_distribution(&mut result, base_quality);
    } else {
        result[base_quality] = 1.0;
    }
    result
}

/// Settle an upgrade distribution.
///
/// An upgrade overflow borrows low-quality items. The game pays that debt
/// back from low to high, so high-quality results are retained first.
fn settle_upgrade_distribution(result: &mut [f64], base_quality: usize) {
    let mut sum = 0.0;
    for level in 0..result.len() {
        if result[level] < 0.0 {
            let deficit = result[level];
            if let Some(next) = result.get_mut(level + 1) {
                *next += deficit;
            }
            result[level] = 0.0;
        }
        sum += result[level];
    }

    if sum > 1.0 {
        let mut tail = 0.0;
        for level in (0..result.len()).rev() {
            tail += result[level];
            if tail > 1.0 {
                result[level] -= tail - 1.0;
                tail = 1.0;
            }
        }
    } else {
        result[base_quality] += (1.0 - sum).clamp(0.0, 1.0);
    }
}

/// Settle a downgrade distribution.
///
/// A downgrade overflow borrows high-quality items. Unlike an upgrade, the
/// excess must therefore be removed from high to low, retaining the lower
/// quality results first.
fn settle_downgrade_distribution(result: &mut [f64], base_quality: usize) {
    let mut sum = 0.0;
    for level in (0..result.len()).rev() {
        if result[level] < 0.0 {
            let deficit = result[level];
            if let Some(previous) = level.checked_sub(1).and_then(|level| result.get_mut(level)) {
                *previous += deficit;
            }
            result[level] = 0.0;
        }
        sum += result[level];
    }

    if sum > 1.0 {
        let mut retained = 0.0;
        for amount in result {
            retained += *amount;
            if retained > 1.0 {
                *amount -= retained - 1.0;
                retained = 1.0;
            }
        }
    } else {
        result[base_quality] += (1.0 - sum).clamp(0.0, 1.0);
    }
}

#[test]
fn test_calc_quality_distribution() {
    fn load_dump() -> serde_json::Value {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/data-raw-dump.json"
        );
        let text = std::fs::read_to_string(path)
            .expect("modded dump 不存在（assets/data-raw-dump-heavily-modded.json）");
        serde_json::from_str(&text).expect("modded dump 解析失败")
    }
    fn store() -> metatorio_data::store::PrototypeStore {
        let dump = load_dump();
        metatorio_data::store::PrototypeStore::load(&dump).expect("dump 加载失败")
    }
    let s = store();
    let game = crate::context::GameState::default();
    let ctx = Context::new(&s, &game);
    let qualities = sorted_qualities(&ctx);
    assert!(!qualities.is_empty());
    let distribution = calc_quality_distribution(&ctx, 0.5, 0, 4);
    let expected = [0.5, 0.45, 0.045, 0.0045, 0.0005];
    assert_eq!(distribution.len(), expected.len());
    for (actual, expected) in distribution.iter().zip(expected) {
        assert!(
            (actual - expected).abs() < 1e-12,
            "distribution: {distribution:?}"
        );
    }

    let overflow = calc_quality_distribution(&ctx, 20.0, 0, 4);
    let expected_overflow = [0.0, 0.0, 0.8, 0.18, 0.02];
    for (actual, expected) in overflow.iter().zip(expected_overflow) {
        assert!((actual - expected).abs() < 1e-12, "overflow: {overflow:?}");
    }
}

#[test]
fn downgrade_overflow_keeps_lower_qualities_first() {
    let mut qualities = serde_json::Map::new();
    for level in 0..5 {
        let name = if level == 0 {
            "normal".to_string()
        } else {
            format!("quality-{level}")
        };
        qualities.insert(
            name.clone(),
            serde_json::json!({
                "name": name,
                "level": level,
                "next": if level < 4 {
                    Some(format!("quality-{}", level + 1))
                } else {
                    None
                },
                "next_probability": 1.0,
                "chain_probability": 0.1,
                "previous_probability": 1.0,
                "previous_chain_probability": 0.1
            }),
        );
    }
    let store = metatorio_data::store::PrototypeStore::load(&serde_json::json!({
        "quality": qualities
    }))
    .unwrap();
    let game = crate::context::GameState {
        qualities: vec![
            "normal".to_string(),
            "quality-1".to_string(),
            "quality-2".to_string(),
            "quality-3".to_string(),
            "quality-4".to_string(),
        ],
        max_quality: 4,
        ..Default::default()
    };
    let ctx = Context::new(&store, &game);
    let distribution = calc_quality_distribution(&ctx, -20.0, 4, 4);
    let expected = [0.02, 0.18, 0.8, 0.0, 0.0];
    for (actual, expected) in distribution.iter().zip(expected) {
        assert!(
            (actual - expected).abs() < 1e-12,
            "distribution: {distribution:?}"
        );
    }
}
