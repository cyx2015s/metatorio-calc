//! 使用仓库内置真实游戏数据（assets/data-raw-dump.json）对比
//! 「全量枚举」与「延迟列生成（Column Generation）」在不同品质生产需求下的
//! 候选规模、计算耗时与最优成本一致性。
//!
//! 运行方式：
//! `cargo test --release -p metatorio-runtime --no-default-features --features highs --test column_gen_bench -- --nocapture`

#![cfg(feature = "highs")]

use std::time::Instant;

use metatorio_core::mechanic::BeaconConfig;
use metatorio_core::{DualVar, IdWithQuality, ModuleConfig};
use metatorio_data::store::PrototypeStore;
use metatorio_runtime::document::{
    AppDocument, AutoBeaconPlan, ExternalInput, FactoryDocument, FactorySettings, FlowTarget,
    PlanningPreferences, ProjectDocument, ProjectSettings,
};
use metatorio_runtime::id::{ExternalInputId, FactoryId, ProjectId, TargetId};
use metatorio_runtime::solve::{Runtime, SolveStatus};
use metatorio_solver::LpSolver;

fn load_real_store() -> PrototypeStore {
    let dump_path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/data-raw-dump.json");
    let raw = std::fs::read(dump_path).expect("读取 assets/data-raw-dump.json 失败");
    let dump: serde_json::Value = serde_json::from_slice(&raw).expect("解析 dump JSON 失败");
    PrototypeStore::load(&dump).expect("加载 PrototypeStore 失败")
}

fn make_nauvis_project(
    item_id: &str,
    target_quality: &str,
    quality_limit: &str,
    amount: f64,
    beacons: Vec<AutoBeaconPlan>,
) -> AppDocument {
    let project_id = ProjectId(1);
    let factory_id = FactoryId(2);
    let mut settings = ProjectSettings::default();
    settings.all_accessible = true;
    settings.quality_limit = Some(quality_limit.to_string());
    settings.major_quality = quality_limit.to_string();
    settings.solver = LpSolver::Highs;

    let planning = PlanningPreferences {
        alternative_count: 2,
        machine_preferences: Vec::new(),
        enumerate_modules: vec![
            IdWithQuality::new("productivity-module-3", quality_limit),
            IdWithQuality::new("quality-module-3", quality_limit),
            IdWithQuality::new("speed-module-3", quality_limit),
            IdWithQuality::new("efficiency-module-3", quality_limit),
        ],
        enumerate_beacons: beacons,
        preserve_existing_mechanics: false,
    };

    let factory = FactoryDocument {
        id: factory_id,
        name: format!("{item_id}-{target_quality}"),
        settings: FactorySettings {
            planet: Some("nauvis".to_string()),
            surface: None,
            ..FactorySettings::default()
        },
        strict_source: true,
        strict_sink: false,
        targets: vec![FlowTarget {
            id: TargetId(1),
            flow: DualVar::Item(IdWithQuality::new(item_id, target_quality)),
            amount,
        }],
        target_expressions: Vec::new(),
        external_inputs: vec![
            ExternalInput {
                id: ExternalInputId(1),
                flow: DualVar::Entity(IdWithQuality::new("iron-ore", "normal")),
                penalty: 1.0,
            },
            ExternalInput {
                id: ExternalInputId(2),
                flow: DualVar::Entity(IdWithQuality::new("copper-ore", "normal")),
                penalty: 1.0,
            },
            ExternalInput {
                id: ExternalInputId(3),
                flow: DualVar::Entity(IdWithQuality::new("coal", "normal")),
                penalty: 1.0,
            },
            ExternalInput {
                id: ExternalInputId(4),
                flow: DualVar::Entity(IdWithQuality::new("crude-oil", "normal")),
                penalty: 1.0,
            },
            ExternalInput {
                id: ExternalInputId(5),
                flow: DualVar::Entity(IdWithQuality::new("stone", "normal")),
                penalty: 1.0,
            },
            ExternalInput {
                id: ExternalInputId(6),
                flow: DualVar::Electricity,
                penalty: 1e-6,
            },
        ],
        mechanics: Vec::new(),
    };

    AppDocument {
        schema_version: 1,
        projects: vec![ProjectDocument {
            id: project_id,
            name: "bench".to_string(),
            settings,
            planning,
            context_id: Some("bench".to_string()),
            factories: vec![factory],
            universes: Vec::new(),
        }],
    }
}

fn run_one(
    label: &str,
    store: &PrototypeStore,
    doc: &AppDocument,
) {
    let project_id = doc.projects[0].id;
    let factory_id = doc.projects[0].factories[0].id;
    let ctx_id = doc.projects[0]
        .context_id
        .clone()
        .unwrap_or_else(|| "bench".to_string());

    // 1) 全量枚举
    unsafe {
        std::env::set_var("METATORIO_FULL_ENUM_AUTOPLAN", "1");
    }
    let mut rt_full = Runtime::from_document(doc.clone());
    rt_full.install_context(ctx_id.clone(), store.clone());
    let t0 = Instant::now();
    let full_res = rt_full.auto_plan(project_id, factory_id);
    let full_ms = t0.elapsed().as_secs_f64() * 1000.0;

    // 2) 列生成
    unsafe {
        std::env::remove_var("METATORIO_FULL_ENUM_AUTOPLAN");
    }
    let mut rt_cg = Runtime::from_document(doc.clone());
    rt_cg.install_context(ctx_id, store.clone());
    let t1 = Instant::now();
    let cg_res = rt_cg.auto_plan(project_id, factory_id);
    let cg_ms = t1.elapsed().as_secs_f64() * 1000.0;

    let (full_cost, full_cands, full_used) = match &full_res {
        Ok(r) => match &r.status {
            SolveStatus::Solved { cost, .. } => {
                (Some(*cost), r.report.candidates_total, r.report.candidates_used)
            }
            SolveStatus::NotSolved { description, .. } => {
                eprintln!("[{label}] FULL NotSolved: {description}");
                (None, r.report.candidates_total, 0)
            }
        },
        Err(e) => {
            eprintln!("[{label}] FULL Err: {e}");
            (None, 0, 0)
        }
    };

    let (cg_cost, cg_cols, cg_used) = match &cg_res {
        Ok(r) => match &r.status {
            SolveStatus::Solved { cost, .. } => {
                (Some(*cost), r.report.candidates_total, r.report.candidates_used)
            }
            SolveStatus::NotSolved { description, .. } => {
                eprintln!("[{label}] CG NotSolved: {description}");
                (None, r.report.candidates_total, 0)
            }
        },
        Err(e) => {
            eprintln!("[{label}] CG Err: {e}");
            (None, 0, 0)
        }
    };

    let speedup = if cg_ms > 0.0 { full_ms / cg_ms } else { 0.0 };
    let rel_diff = match (full_cost, cg_cost) {
        (Some(fc), Some(cc)) => Some((cc - fc).abs() / fc.abs().max(1.0)),
        _ => None,
    };

    println!(
        "| {label:<28} | Full: {full_cands:>6} cols, {full_ms:>8.1} ms, cost={:<10} (used={full_used:>2}) | CG: {cg_cols:>5} cols, {cg_ms:>7.1} ms, cost={:<10} (used={cg_used:>2}) | speedup={speedup:>5.2}x | rel_diff={:<9} |",
        full_cost.map(|c| format!("{c:.4}")).unwrap_or_else(|| "N/A".into()),
        cg_cost.map(|c| format!("{c:.4}")).unwrap_or_else(|| "N/A".into()),
        rel_diff.map(|d| format!("{d:.2e}")).unwrap_or_else(|| "N/A".into()),
    );

    if let (Some(fc), Some(cc)) = (full_cost, cg_cost) {
        let diff = (cc - fc).abs() / fc.abs().max(1.0);
        assert!(
            diff < 1e-4,
            "[{label}] 列生成与全量枚举最优成本偏差过大: full={fc}, cg={cc}, rel_diff={diff}"
        );
    }
}

#[test]
fn benchmark_quality_tiers_real_dump() {
    let store = load_real_store();

    let beacon_speed_8 = AutoBeaconPlan {
        module_config: ModuleConfig {
            modules: Vec::new(),
            beacons: vec![BeaconConfig {
                beacon: IdWithQuality::new("beacon", "legendary"),
                count: 8,
                share: 2.0,
                modules: vec![(IdWithQuality::new("speed-module-3", "legendary"), 2)],
            }],
        },
    };

    println!("\n=== Real Factorio Space Age Dump Benchmark (Full Enumeration vs Column Generation) ===");

    // 1. 同一目标（processing-unit 1.0/s）在不同品质需求下的对比
    for quality in ["normal", "uncommon", "rare", "epic", "legendary"] {
        let doc = make_nauvis_project("processing-unit", quality, quality, 1.0, Vec::new());
        run_one(&format!("proc-unit ({quality})"), &store, &doc);
    }

    // 2. 传说品质 + 插件塔方案（组合数翻倍）
    let doc_beacon = make_nauvis_project(
        "processing-unit",
        "legendary",
        "legendary",
        1.0,
        vec![beacon_speed_8],
    );
    run_one("proc-unit (legendary+beacon)", &store, &doc_beacon);

    // 3. 仓库自带的传说虫卵回归工程（使用 HiGHS 后端）
    let mut biter_doc: AppDocument =
        serde_json::from_str(include_str!("data/legendary-biter-egg.json"))
            .expect("解析 legendary-biter-egg.json 失败");
    biter_doc.projects[0].settings.solver = LpSolver::Highs;
    run_one("legendary-biter-egg fixture", &store, &biter_doc);
}


fn make_universe_project(
    item_id: &str,
    target_location: metatorio_core::Location,
    target_quality: &str,
    quality_limit: &str,
    amount: f64,
    beacons: Vec<AutoBeaconPlan>,
) -> AppDocument {
    use metatorio_core::{LocatedDualVar, Location};
    use metatorio_runtime::document::{LocatedTarget, UniverseDocument};
    use metatorio_runtime::id::UniverseId;

    let project_id = ProjectId(1);
    let universe_id = UniverseId(10);

    let mut settings = ProjectSettings::default();
    settings.all_accessible = true;
    settings.quality_limit = Some(quality_limit.to_string());
    settings.major_quality = quality_limit.to_string();
    settings.solver = LpSolver::Highs;

    let planning = PlanningPreferences {
        alternative_count: 1,
        machine_preferences: Vec::new(),
        enumerate_modules: vec![
            IdWithQuality::new("productivity-module-3", quality_limit),
            IdWithQuality::new("quality-module-3", quality_limit),
            IdWithQuality::new("speed-module-3", quality_limit),
        ],
        enumerate_beacons: beacons,
        preserve_existing_mechanics: false,
    };

    // 宇宙工厂：行星天然矿藏与太空星块由隐式来源自动提供，这里只给各星球少量电网/外部基准
    let mut external_inputs = Vec::new();
    let mut next_ext = 1u64;
    for loc in [
        Location::planet("nauvis"),
        Location::planet("vulcanus"),
        Location::planet("fulgora"),
        Location::planet("gleba"),
        Location::planet("aquilo"),
        Location::Space,
    ] {
        external_inputs.push(metatorio_runtime::document::LocatedExternalInput {
            id: ExternalInputId(next_ext),
            flow: LocatedDualVar::new(loc, DualVar::Electricity),
            penalty: 1e-6,
        });
        next_ext += 1;
    }

    let universe = UniverseDocument {
        id: universe_id,
        name: format!("universe-{item_id}-{target_quality}"),
        external_inputs,
        mechanics: Vec::new(),
        targets: vec![LocatedTarget {
            id: TargetId(1),
            flow: LocatedDualVar::new(
                target_location,
                DualVar::Item(IdWithQuality::new(item_id, target_quality)),
            ),
            amount,
        }],
        target_expressions: Vec::new(),
        strict_source: true,
        strict_sink: false,
    };

    AppDocument {
        schema_version: 1,
        projects: vec![ProjectDocument {
            id: project_id,
            name: "universe-bench".to_string(),
            settings,
            planning,
            context_id: Some("bench".to_string()),
            factories: Vec::new(),
            universes: vec![universe],
        }],
    }
}

fn run_one_universe(label: &str, store: &PrototypeStore, doc: &AppDocument) {
    let project_id = doc.projects[0].id;
    let universe_id = doc.projects[0].universes[0].id;
    let ctx_id = doc.projects[0]
        .context_id
        .clone()
        .unwrap_or_else(|| "bench".to_string());

    // 1) 列生成（先跑并打印）
    unsafe {
        std::env::remove_var("METATORIO_FULL_ENUM_AUTOPLAN");
    }
    let mut rt_cg = Runtime::from_document(doc.clone());
    rt_cg.install_context(ctx_id.clone(), store.clone());
    let t1 = Instant::now();
    let cg_res = rt_cg.auto_plan_universe(project_id, universe_id);
    let cg_ms = t1.elapsed().as_secs_f64() * 1000.0;
    eprintln!("[{label}] CG done in {cg_ms:.1} ms");

    // 2) 全量枚举
    unsafe {
        std::env::set_var("METATORIO_FULL_ENUM_AUTOPLAN", "1");
    }
    let mut rt_full = Runtime::from_document(doc.clone());
    rt_full.install_context(ctx_id, store.clone());
    let t0 = Instant::now();
    let full_res = rt_full.auto_plan_universe(project_id, universe_id);
    let full_ms = t0.elapsed().as_secs_f64() * 1000.0;
    eprintln!("[{label}] FULL done in {full_ms:.1} ms");

    let (full_cost, full_cands, full_used) = match &full_res {
        Ok(r) => match &r.status {
            metatorio_runtime::universe::UniverseSolveStatus::Solved { cost, .. } => {
                (Some(*cost), r.report.candidates_total, r.report.candidates_used)
            }
            metatorio_runtime::universe::UniverseSolveStatus::NotSolved { description, .. } => {
                eprintln!("[{label}] FULL NotSolved: {description}");
                (None, r.report.candidates_total, 0)
            }
        },
        Err(e) => {
            eprintln!("[{label}] FULL Err: {e}");
            (None, 0, 0)
        }
    };

    let (cg_cost, cg_cols, cg_used) = match &cg_res {
        Ok(r) => match &r.status {
            metatorio_runtime::universe::UniverseSolveStatus::Solved { cost, .. } => {
                (Some(*cost), r.report.candidates_total, r.report.candidates_used)
            }
            metatorio_runtime::universe::UniverseSolveStatus::NotSolved { description, .. } => {
                eprintln!("[{label}] CG NotSolved: {description}");
                (None, r.report.candidates_total, 0)
            }
        },
        Err(e) => {
            eprintln!("[{label}] CG Err: {e}");
            (None, 0, 0)
        }
    };

    let speedup = if cg_ms > 0.0 { full_ms / cg_ms } else { 0.0 };
    let rel_diff = match (full_cost, cg_cost) {
        (Some(fc), Some(cc)) => Some((cc - fc).abs() / fc.abs().max(1.0)),
        _ => None,
    };

    println!(
        "| {label:<32} | Full: {full_cands:>6} cols, {full_ms:>8.1} ms, cost={:<11} (used={full_used:>3}) | CG: {cg_cols:>5} cols, {cg_ms:>7.1} ms, cost={:<11} (used={cg_used:>3}) | speedup={speedup:>5.2}x | rel_diff={:<9} |",
        full_cost.map(|c| format!("{c:.4}")).unwrap_or_else(|| "N/A".into()),
        cg_cost.map(|c| format!("{c:.4}")).unwrap_or_else(|| "N/A".into()),
        rel_diff.map(|d| format!("{d:.2e}")).unwrap_or_else(|| "N/A".into()),
    );

    assert!(full_cost.is_some(), "[{label}] Full enumeration should solve");
    assert!(cg_cost.is_some(), "[{label}] Column generation should solve");
    if let (Some(fc), Some(cc)) = (full_cost, cg_cost) {
        let diff = (cc - fc).abs() / fc.abs().max(1.0);
        assert!(
            diff < 1e-4,
            "[{label}] 宇宙列生成与全量枚举最优成本偏差过大: full={fc}, cg={cc}, rel_diff={diff}"
        );
    }
}

#[test]
fn benchmark_universe_em_science_real_dump() {
    use metatorio_core::Location;
    let store = load_real_store();
    println!("\n=== Real Space Age UNIVERSE Factory Benchmark: EM Science (6 Locations) ===");
    for quality in ["normal", "rare", "legendary"] {
        let doc = make_universe_project(
            "electromagnetic-science-pack",
            Location::planet("nauvis"),
            quality,
            quality,
            1.0,
            Vec::new(),
        );
        run_one_universe(&format!("em-science@nauvis ({quality})"), &store, &doc);
    }
}

#[test]
fn benchmark_universe_quantum_processor_real_dump() {
    use metatorio_core::Location;
    let store = load_real_store();
    println!("\n=== Real Space Age UNIVERSE Factory Benchmark: Quantum Processor (6 Locations) ===");
    for quality in ["normal", "rare", "legendary"] {
        let doc = make_universe_project(
            "quantum-processor",
            Location::planet("nauvis"),
            quality,
            quality,
            0.5,
            Vec::new(),
        );
        run_one_universe(&format!("quantum-proc@nauvis ({quality})"), &store, &doc);
    }
}
