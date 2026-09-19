//! 诊断工具：把一份 `data-raw-dump.json` 按我们的类型系统完整加载一遍。
//!
//! 用途：狂野 mod 的 dump 在游戏里能跑、在我们这里加载失败时，用它直接看到
//! **哪个原型 / 哪个组件 / 哪个字段**出的问题——而不是只得到一句「加载失败」。
//!
//! ```text
//! cargo run -p metatorio-data --release --example load_dump -- <dump.json>
//! ```
//!
//! 退出码：0 = 全部原型加载成功；1 = 有失败（明细打到 stderr）。

use metatorio_data::store::{PrototypeGroup, PrototypeStore};

/// 失败明细最多打印多少条（全量可能上千条，先看头几条定位形态）。
const MAX_PRINTED: usize = 20;

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(path) = args.next() else {
        eprintln!("用法: load_dump <data-raw-dump.json>");
        std::process::exit(2);
    };

    let text = std::fs::read_to_string(&path).unwrap_or_else(|error| {
        eprintln!("读取 dump 失败: {path}: {error}");
        std::process::exit(2);
    });
    let dump: serde_json::Value = serde_json::from_str(&text).unwrap_or_else(|error| {
        eprintln!("dump 不是合法 JSON: {path}: {error}");
        std::process::exit(2);
    });

    match PrototypeStore::load(&dump) {
        Ok(store) => {
            println!(
                "加载成功：{} 条原型记录（Entity {} / Item {} / 其它 {}）",
                store.len(),
                store.group(PrototypeGroup::Entity).count(),
                store.group(PrototypeGroup::Item).count(),
                store.len()
                    - store.group(PrototypeGroup::Entity).count()
                    - store.group(PrototypeGroup::Item).count(),
            );
        }
        Err(error) => {
            eprintln!(
                "加载失败：{} / {} 个原型（{} 条失败明细，打印前 {} 条）",
                error.failures.len(),
                error.total,
                error.failures.len(),
                MAX_PRINTED.min(error.failures.len()),
            );
            for (typename, name, detail) in error.failures.iter().take(MAX_PRINTED) {
                eprintln!("  {typename}/{name}: {detail}");
            }
            std::process::exit(1);
        }
    }
}
