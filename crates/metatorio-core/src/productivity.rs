//! 从可达性自动推算配方产能与采矿产能（复刻原版语义）。
//!
//! 原版中所有配方/采矿产能加成都来自**科技 effect**（`Modifier`）：
//! - `ChangeRecipeProductivity { change, recipe }`：给某配方 +change 产能（每级）；
//! - `MiningDrillProductivityBonus { modifier }`：给采矿 +modifier 产能（每级）。
//! 
//! 这两类 effect 通常挂在 `max_level = infinite` 的无限科技上（如
//! 
//! `mining-productivity-3`、`steel-plate-productivity`），每研究一级 +0.1。
//!
//! 因此"从可达性推算"即：遍历全部科技，对**可达的**产能科技按其
//! **等级**累计贡献；默认一个可达的无限科技视为研究 1 级（level 1）。
//! 用户可对某些无限科技单独指定研究次数（2.b 覆盖该科技等级），该覆盖
//! 不论是否"忽略产能"都生效。

use metatorio_data::TechnologyComponent;
use metatorio_data::store::{PrototypeGroup, PrototypeStore};
use metatorio_data::types::Modifier;

use crate::accessibility::{Accessibility, Accessible};
use crate::prim_var::AIndexMap;

/// 自动推算的产能结果。
#[derive(Debug, Clone, Default)]
pub struct ProductivityResult {
    /// 配方名 → 产能加成（小数）。
    pub recipe_productivity: AIndexMap<String, f64>,
    /// 采矿产出加成（小数）。
    pub mining_productivity: f64,
}

/// 单个无限科技的用户覆盖等级（2.b）。
pub type InfiniteTechLevel = (String, u32);

/// 从可达性推算配方/采矿产能。
///
/// - `level(tech)`：若 `infinite_levels` 中指定了该科技等级 → 用用户等级
///   （不论是否忽略产能，恒生效）；否则若**可达**且**未忽略产能** → 视为 1；
///   否则 0。
/// - 累计各科技 effect 的 `change`/`modifier` × 等级，得到配方与采矿产能。
pub fn compute_productivity(
    store: &PrototypeStore,
    accessibility: &Accessibility,
    infinite_levels: &[InfiniteTechLevel],
    ignore: bool,
) -> ProductivityResult {
    let mut out = ProductivityResult::default();
    for record in store.group(PrototypeGroup::Technology) {
        let Some(tech) = record.component::<TechnologyComponent>() else {
            continue;
        };
        let name = record.name.clone();

        let level = if let Some((_, lvl)) = infinite_levels.iter().find(|(t, _)| *t == name) {
            // 用户覆盖（2.b）恒生效（0 即无贡献）。
            *lvl
        } else if !ignore && accessibility.is_accessible(&Accessible::Tech(name)) {
            1
        } else {
            0
        };
        if level == 0 {
            continue;
        }

        let level_f = level as f64;
        for effect in &tech.effects {
            match effect {
                Modifier::MiningDrillProductivityBonus(simple) => {
                    out.mining_productivity += simple.modifier * level_f;
                }
                Modifier::ChangeRecipeProductivity(change) => {
                    *out.recipe_productivity
                        .entry(change.recipe.clone())
                        .or_insert(0.0) += change.change * level_f;
                }
                _ => {}
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accessibility::{AccessibilityOptions, compute_accessibility};
    use metatorio_data::store::PrototypeStore;

    const REAL_DUMP: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../assets/data-raw-dump.json"
    );

    fn load_real_dump() -> Option<PrototypeStore> {
        if !std::path::Path::new(REAL_DUMP).exists() {
            eprintln!("[skip] 无真实 dump（{REAL_DUMP}），跳过");
            return None;
        }
        let raw = std::fs::read(REAL_DUMP).expect("读 dump");
        let dump: serde_json::Value = serde_json::from_slice(&raw).expect("解析 dump");
        match PrototypeStore::load(&dump) {
            Ok(store) => Some(store),
            Err(error) => {
                for failure in &error.failures {
                    eprintln!("[load 失败] {:?}", failure);
                }
                panic!("dump 加载失败: {:?}", error);
            }
        }
    }

    fn near(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    /// 全可达：采矿产能 = mining-productivity-1/2/3（各 0.1）= 0.3；
    /// 配方产能 = steel-plate-productivity（change 0.1）→ steel-plate = 0.1。
    #[test]
    fn compute_productivity_from_reachability_all_accessible() {
        let Some(store) = load_real_dump() else {
            return;
        };
        let options = AccessibilityOptions {
            all_accessible: true,
            ..Default::default()
        };
        let accessibility = compute_accessibility(&store, &options);

        let result = compute_productivity(&store, &accessibility, &[], false);
        assert!(
            near(result.mining_productivity, 0.3),
            "全可达采矿产能应为 0.3，实际 {}",
            result.mining_productivity
        );
        assert!(
            near(result.recipe_productivity["steel-plate"], 0.1),
            "steel-plate 应为 0.1，实际 {}",
            result.recipe_productivity["steel-plate"]
        );
    }

    /// 2.b：用户覆盖无限科技等级（mining-productivity-3 → 50）应替换默认等级 1，
    /// 采矿产能增加 0.1×49 = 4.9。
    #[test]
    fn compute_productivity_respects_user_infinite_level() {
        let Some(store) = load_real_dump() else {
            return;
        };
        let options = AccessibilityOptions {
            all_accessible: true,
            ..Default::default()
        };
        let accessibility = compute_accessibility(&store, &options);

        let base = compute_productivity(&store, &accessibility, &[], false);
        let overridden = compute_productivity(
            &store,
            &accessibility,
            &[("mining-productivity-3".into(), 50)],
            false,
        );
        let delta = overridden.mining_productivity - base.mining_productivity;
        assert!(
            near(delta, 4.9),
            "设置 mining-productivity-3 等级 50 应增加 4.9，实际 {delta}"
        );
        assert!(overridden.mining_productivity > base.mining_productivity);
    }

    /// 2.c：忽略产能时丢弃自动推算（采矿/配方均 0），但仍保留用户覆盖的无限等级。
    #[test]
    fn compute_productivity_ignore_drops_auto_keeps_user() {
        let Some(store) = load_real_dump() else {
            return;
        };
        let options = AccessibilityOptions {
            all_accessible: true,
            ..Default::default()
        };
        let accessibility = compute_accessibility(&store, &options);

        let ignored = compute_productivity(&store, &accessibility, &[], true);
        assert!(near(ignored.mining_productivity, 0.0), "忽略时采矿应 0");
        assert!(ignored.recipe_productivity.is_empty(), "忽略时配方应空");

        // 用户覆盖的无限等级仍生效（忽略时也计入）。
        let ignored_user = compute_productivity(
            &store,
            &accessibility,
            &[("mining-productivity-3".into(), 10)],
            true,
        );
        assert!(
            near(ignored_user.mining_productivity, 1.0),
            "忽略但用户设 mining-productivity-3=10 应得 1.0，实际 {}",
            ignored_user.mining_productivity
        );
    }
}
