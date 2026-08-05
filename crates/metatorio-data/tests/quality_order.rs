//! 品质顺序测试：等级顺序由 next 链定义（normal → uncommon → rare），
//! 与 order 字段无关（测试中 order 故意与链序相反）。

use metatorio_data::store::PrototypeStore;
use serde_json::{Value, json};

fn quality_dump() -> Value {
    json!({
        "quality": {
            // order 与链序相反：链序 normal → uncommon → rare
            "rare":     { "level": 2, "color": [1, 0, 0, 1], "order": "a" },
            "normal":   { "level": 0, "color": [0, 0, 0, 1], "order": "c", "next": "uncommon" },
            "uncommon": { "level": 1, "color": [0, 0, 1, 1], "order": "b", "next": "rare" }
        }
    })
}

#[test]
fn quality_order_follows_next_chain_not_order() {
    let store = PrototypeStore::load(&quality_dump()).expect("dump 加载失败");
    // 链序（next 链）：normal → uncommon → rare
    assert_eq!(
        store.quality_order(),
        &[
            "normal".to_string(),
            "uncommon".to_string(),
            "rare".to_string()
        ][..]
    );
}

#[test]
fn quality_order_without_normal_uses_chain_head() {
    // 无 "normal" 品质：链头 = 不是任何 next 目标的品质
    let dump = json!({
        "quality": {
            "epic":   { "level": 2, "color": [1, 0, 0, 1], "order": "a" },
            "uncommon": { "level": 1, "color": [0, 0, 1, 1], "order": "b", "next": "epic" }
        }
    });
    let store = PrototypeStore::load(&dump).expect("dump 加载失败");
    assert_eq!(
        store.quality_order(),
        &["uncommon".to_string(), "epic".to_string()][..]
    );
}

#[test]
fn quality_order_cycle_protected() {
    // mod 数据成环：normal ↔ uncommon 互指 → 不无限循环，只取链头一次
    let dump = json!({
        "quality": {
            "normal":   { "level": 0, "color": [0, 0, 0, 1], "order": "a", "next": "uncommon" },
            "uncommon": { "level": 1, "color": [0, 0, 1, 1], "order": "b", "next": "normal" }
        }
    });
    let store = PrototypeStore::load(&dump).expect("dump 加载失败");
    let order = store.quality_order();
    assert_eq!(order.len(), 2, "环内品质各出现一次：{order:?}");
    assert!(order.contains(&"normal".to_string()));
    assert!(order.contains(&"uncommon".to_string()));
}
