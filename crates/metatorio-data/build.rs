//! 编译期生成：读取 schema/prototype-api.json → 生成组件结构体到 OUT_DIR。

use std::path::Path;

fn main() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let schema_path = manifest_dir.join("schema/prototype-api.json");
    let json = std::fs::read_to_string(&schema_path)
        .unwrap_or_else(|e| panic!("读取 schema 失败 ({schema_path:?}): {e}"));

    let schema = metatorio_data_codegen::Schema::parse(&json)
        .unwrap_or_else(|e| panic!("解析 schema 失败: {e}"));

    let config = metatorio_data_codegen::Config::default();
    let (code, stats) = metatorio_data_codegen::generate(&schema, &config);

    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR 未设置");
    std::fs::write(Path::new(&out_dir).join("generated.rs"), code)
        .expect("写入 generated.rs 失败");

    println!(
        "cargo:warning=metatorio-data codegen: schema {}, {} 个关注类型, {} 个组件, {} 个字段, {} 个字段被忽略",
        schema.application_version,
        stats.concerned_typenames,
        stats.component_structs,
        stats.fields,
        stats.skipped_fields,
    );

    // schema 或生成器变化时重新生成
    println!("cargo:rerun-if-changed=schema/prototype-api.json");
    println!("cargo:rerun-if-changed=../metatorio-data-codegen/src");
}
