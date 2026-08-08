//! 品质分布计算（迁移自 metatorio-egui `calc_quality_distribution`）。

use crate::context::{Context, GameState};
use metatorio_data::generated_components::QualityComponent;
use metatorio_data::store::{PrototypeGroup, PrototypeStore};

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
#[allow(clippy::needless_range_loop)] // 公式结构忠实迁移自 egui，索引访问保持可读性
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
    if quality_bonus > 0.0 {
        let mut multiplier = qualities[base_quality].next_probability * quality_bonus;
        result[base_quality] = multiplier; // 有这么多能参与品质转移
        multiplier = 1.0;
        for idx in base_quality..maximum_quality {
            let jdx = idx + 1;
            result[jdx] = result[idx] * multiplier;
            multiplier = qualities[idx].chain_probability();
        }
        for idx in (base_quality + 1)..result.len() {
            let hdx = idx - 1;
            result[hdx] -= result[idx];
        }
        let mut sum = 0.0;
        for idx in 0..(result.len() - 1) {
            if result[idx] < 0.0 {
                result[idx + 1] += result[idx];
                result[idx] = 0.0;
            }
            sum += result[idx];
        }
        sum += result[result.len() - 1];
        if sum > 1.0 {
            let mut sum_alt = 0.0;
            for idx in (0..result.len()).rev() {
                sum_alt += result[idx];
                if sum_alt > 1.0 {
                    result[idx] -= sum_alt - 1.0;
                    sum_alt = 1.0;
                }
            }
        }
        result[base_quality] += (1.0 - sum).clamp(0.0, 1.0);
        result
    } else {
        let mut multiplier = qualities[base_quality].previous_probability * quality_bonus.abs();
        result[base_quality] = multiplier; // 有这么多能参与品质转移
        multiplier = 1.0;
        for idx in (1..=base_quality).rev() {
            let jdx = idx - 1;
            result[jdx] = result[idx] * multiplier;
            multiplier = qualities[idx].previous_chain_probability();
        }
        for idx in (1..=base_quality).rev() {
            let hdx = idx - 1;
            result[hdx] -= result[idx];
        }
        let mut sum = 0.0;
        for idx in (1..result.len()).rev() {
            if result[idx] < 0.0 {
                result[idx - 1] += result[idx];
                result[idx] = 0.0;
            } else {
                sum += result[idx];
            }
        }
        sum += result[0];
        if sum > 1.0 {
            let mut sum_alt = 0.0;
            for idx in 0..result.len() {
                sum_alt += result[idx];
                if sum_alt > 1.0 {
                    result[idx] -= sum_alt - 1.0;
                    sum_alt = 1.0;
                }
            }
        }
        result[base_quality] += (1.0 - sum).clamp(0.0, 1.0);
        result
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
    fn store() -> PrototypeStore {
        let dump = load_dump();
        PrototypeStore::load(&dump).expect("dump 加载失败")
    }
    let s = store();
    let game = GameState::default();
    let ctx = Context::new(&s, &game);
    let qualities = sorted_qualities(&ctx);
    assert!(!qualities.is_empty());
    assert_eq!(
        calc_quality_distribution(&ctx, 0.5, 0, 4),
        vec![
            0.5,
            0.45,
            0.045,
            0.0045000000000000005,
            0.0005000000000000001
        ]
    );
}
