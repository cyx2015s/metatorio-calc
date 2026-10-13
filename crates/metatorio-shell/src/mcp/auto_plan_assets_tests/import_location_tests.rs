//! Import metadata is a discovery rule, not a promise of local self-sufficiency.
//! Keep failures visible while checking every successful plan and metamorphic
//! properties of best-module selection. Never import the target to force a pass.
use super::*;
use metatorio_data::ItemComponent;
use std::io::Write;

const QUALITIES: [&str; 5] = ["normal", "uncommon", "rare", "epic", "legendary"];

#[derive(Debug, Clone)]
struct ImportTarget {
    asset: &'static str,
    item: String,
    planet: String,
    kind: String,
}

fn import_targets() -> &'static [ImportTarget] {
    static TARGETS: OnceLock<Vec<ImportTarget>> = OnceLock::new();
    TARGETS.get_or_init(|| {
        let mut targets = Vec::new();
        for asset in [BASE, MODDED] {
            let raw: Value = serde_json::from_slice(
                &std::fs::read(
                    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                        .join("../../assets")
                        .join(asset),
                )
                .unwrap(),
            )
            .unwrap();
            for (kind, group) in raw.as_object().unwrap() {
                let Some(group) = group.as_object() else {
                    continue;
                };
                for (item, prototype) in group {
                    // ItemComponent supplies a default of nauvis even when the
                    // field was absent. Inspect the raw field first, deliberately.
                    let Some(planet) = prototype["default_import_location"].as_str() else {
                        continue;
                    };
                    if planet == "aquilo" {
                        continue;
                    }
                    if store(asset)
                        .get(PrototypeGroup::Item, item)
                        .and_then(|r| r.component::<ItemComponent>())
                        .is_none()
                    {
                        continue;
                    }
                    assert!(
                        store(asset).get(PrototypeGroup::Planet, planet).is_some(),
                        "{asset}/{item}: unknown import planet {planet}"
                    );
                    targets.push(ImportTarget {
                        asset,
                        item: item.clone(),
                        planet: planet.into(),
                        kind: kind.clone(),
                    });
                }
            }
        }
        targets.sort_by(|a, b| (a.asset, &a.planet, &a.item).cmp(&(b.asset, &b.planet, &b.item)));
        targets
    })
}

fn env_number(name: &str, fallback: usize) -> usize {
    std::env::var(name)
        .ok()
        .map(|s| {
            s.parse()
                .unwrap_or_else(|_| panic!("{name} must be an integer"))
        })
        .unwrap_or(fallback)
}

fn selected_targets() -> Vec<&'static ImportTarget> {
    let wanted = std::env::var("AUTOPLAN_ITEMS").ok();
    let asset = std::env::var("AUTOPLAN_ASSET").ok();
    let targets: Vec<_> = import_targets()
        .iter()
        .filter(|target| {
            asset.as_deref().is_none_or(|asset| target.asset == asset)
                && wanted
                    .as_deref()
                    .is_none_or(|names| names.split(',').any(|name| name.trim() == target.item))
        })
        .collect();
    assert!(
        !targets.is_empty(),
        "no import-location targets match the selection"
    );
    targets
}

fn needs_remote_biter_eggs(item: &ImportTarget) -> bool {
    item.asset == BASE
        && matches!(
            item.item.as_str(),
            "overgrowth-jellynut-soil" | "overgrowth-yumako-soil"
        )
}

// The count is all speed modules in the group, not modules per tower.
fn beacon_layout(count: usize, share: f64, beacon_quality: &str, module_quality: &str) -> Value {
    assert!(count > 0 && share >= 1.0);
    json!([{"beacon":{"id":"beacon","quality":beacon_quality},"count":count,"share":share,
        "modules":[{"module":{"id":"speed-module-3","quality":module_quality},"count":count*2}]}])
}

#[derive(Debug, Clone)]
struct Configuration {
    major: &'static str,
    target: &'static str,
    best: bool,
    layout: usize,
}

impl Configuration {
    fn arguments(&self, item: &ImportTarget) -> Value {
        let beacons = match self.layout {
            0 => json!([]),
            1 => beacon_layout(8, 8.0, self.major, self.major),
            2 => beacon_layout(6, 6.0, self.major, "normal"),
            3 => beacon_layout(12, 8.0, "legendary", self.major),
            _ => unreachable!(),
        };
        json!({"planet":item.planet,"major_quality":self.major,
            "targets":[{"item":item.item,"quality":self.target,"amount":1.0}],
            "modules":{"best":self.best,"quality":self.major},"beacons":beacons})
    }
}

fn report_file(label: &str, offset: usize) -> std::fs::File {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(format!("../../local/auto-plan-{label}-{offset}.jsonl"));
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    eprintln!("report: {}", path.display());
    std::fs::File::create(path).unwrap()
}

struct CaseOutcome {
    cost: Option<f64>,
    internal_error: bool,
}

async fn run_case(
    item: &ImportTarget,
    config: &Configuration,
    output: &mut std::fs::File,
) -> CaseOutcome {
    let mcp = handler(item.asset);
    let args = config.arguments(item);
    let begun = Instant::now();
    eprintln!(
        "planning {}/{}/{}: {config:?}",
        item.asset, item.planet, item.item
    );
    let ids = start(&mcp, args.clone()).await;
    let state = finish(&mcp, &ids).await;
    let result = &state["auto_plan"]["result"];
    let failure_kind = if state["auto_plan"]["status"] == "done" {
        Value::Null
    } else if state["auto_plan"]["error"]
        .as_str()
        .unwrap()
        .contains("内部错误")
    {
        json!("solver-error")
    } else {
        json!("infeasible")
    };
    // Flush the terminal state BEFORE asserting, so the failing case survives
    // even a panic in feasibility verification.
    let row = json!({"asset":item.asset,"kind":item.kind,"item":item.item,"planet":item.planet,
        "major_quality":config.major,"target_quality":config.target,"use_best_modules":config.best,
        "layout":config.layout,"arguments":args,"elapsed_ms":begun.elapsed().as_millis(),
        "status":state["auto_plan"]["status"],"error":state["auto_plan"]["error"],"failure_kind":failure_kind,
        "cost":result["status"]["solved"]["cost"],"report":result["report"]});
    writeln!(output, "{row}").unwrap();
    output.flush().unwrap();
    if state["auto_plan"]["status"] == "done" {
        let cost = assert_solved(&mcp, &ids, &state);
        let runtime = mcp.host.state().runtime.lock().unwrap();
        let project = runtime
            .state
            .project(ProjectId(ids["project"].as_u64().unwrap()))
            .unwrap();
        let factory = &project.factories[0];
        assert_eq!(
            factory.settings.planet.as_deref(),
            Some(item.planet.as_str())
        );
        assert_eq!(factory.settings.major_quality, config.major);
        assert_eq!(
            factory.targets[0].flow,
            DualVar::Item(IdWithQuality::new(&item.item, config.target))
        );
        assert_eq!(
            project.planning.enumerate_beacons.len(),
            args["beacons"].as_array().unwrap().len()
        );
        for (plan, requested) in project
            .planning
            .enumerate_beacons
            .iter()
            .zip(args["beacons"].as_array().unwrap())
        {
            let beacon = &plan.module_config.beacons[0];
            assert_eq!(beacon.count, requested["count"].as_u64().unwrap() as usize);
            assert_eq!(beacon.share, requested["share"].as_f64().unwrap());
            assert_eq!(beacon.modules[0].1, beacon.count * 2);
            assert_eq!(beacon.beacon.quality, requested["beacon"]["quality"]);
            assert_eq!(
                beacon.modules[0].0.quality,
                requested["modules"][0]["module"]["quality"]
            );
        }
        assert_eq!(!project.planning.enumerate_modules.is_empty(), config.best);
        CaseOutcome {
            cost: Some(cost),
            internal_error: false,
        }
    } else {
        assert_eq!(state["auto_plan"]["status"], "failed");
        let error = state["auto_plan"]["error"].as_str().unwrap();
        assert!(!error.is_empty());
        assert!(
            error.starts_with("自动规划无解："),
            "infrastructure error must not be counted as an infeasible target: {error}"
        );
        CaseOutcome {
            cost: None,
            internal_error: error.contains("内部错误"),
        }
    }
}

pub(super) async fn run_import_location_sweep() {
    let _permit = GATE.acquire().await.unwrap();
    let offset = env_number("AUTOPLAN_SWEEP_OFFSET", 0);
    let limit = env_number("AUTOPLAN_SWEEP_LIMIT", usize::MAX);
    let mut output = report_file("import-locations", offset);
    let config = Configuration {
        major: "normal",
        target: "normal",
        best: false,
        layout: 0,
    };
    let mut tested = 0;
    let mut solved = 0;
    let mut unexpected = Vec::new();
    for item in selected_targets().into_iter().skip(offset).take(limit) {
        let outcome = run_case(item, &config, &mut output).await;
        let cost = outcome.cost;
        solved += usize::from(cost.is_some());
        if outcome.internal_error || (cost.is_none() && !needs_remote_biter_eggs(item)) {
            unexpected.push(format!("{}/{}/{}", item.asset, item.planet, item.item));
        }
        tested += 1;
    }
    assert!(
        tested > 0 && solved > 0,
        "empty or entirely infeasible import-location survey"
    );
    eprintln!(
        "import-location totals: {solved} solved, {} failed, {tested} tested",
        tested - solved
    );
    assert!(
        unexpected.is_empty(),
        "normal-quality local supply regression(s): {unexpected:#?}"
    );
}

/// Full = 5 major qualities x 5 target qualities x best on/off x 4
/// beacon layouts. Smoke = all 5 qualities on each axis in 20 configurations.
fn configurations(profile: &str) -> Vec<Configuration> {
    let mut cases = Vec::new();
    match profile {
        "full" => {
            for major in QUALITIES {
                for target in QUALITIES {
                    for layout in 0..4 {
                        for best in [false, true] {
                            cases.push(Configuration {
                                major,
                                target,
                                best,
                                layout,
                            });
                        }
                    }
                }
            }
        }
        "smoke" => {
            for quality in QUALITIES {
                for (major, target, layout) in [(quality, "normal", 1), ("normal", quality, 0)] {
                    for best in [false, true] {
                        cases.push(Configuration {
                            major,
                            target,
                            best,
                            layout,
                        });
                    }
                }
            }
        }
        _ => panic!("AUTOPLAN_MATRIX_PROFILE must be full or smoke"),
    }
    cases
}

#[tokio::test]
#[ignore = "import-location major/target quality and beacon matrix; run with --release"]
async fn stress_import_location_quality_matrix() {
    let _permit = GATE.acquire().await.unwrap();
    let offset = env_number("AUTOPLAN_MATRIX_OFFSET", 0);
    let limit = env_number("AUTOPLAN_MATRIX_LIMIT", usize::MAX);
    let profile = std::env::var("AUTOPLAN_MATRIX_PROFILE").unwrap_or_else(|_| "full".into());
    let mut output = report_file(&format!("quality-{profile}"), offset);
    let targets = selected_targets();
    let cases = configurations(&profile);
    let mut previous = std::collections::HashMap::new();
    let mut unexpected = Vec::new();
    let mut tested = 0;
    let mut solved = 0;
    let mut ordinal = 0;
    for config in cases {
        for item in &targets {
            let index = ordinal;
            ordinal += 1;
            if index < offset || tested >= limit {
                continue;
            }
            let outcome = run_case(item, &config, &mut output).await;
            let cost = outcome.cost;
            solved += usize::from(cost.is_some());
            tested += 1;
            if outcome.internal_error {
                unexpected.push(format!(
                    "{}/{}/{}: solver internal error under {config:?}",
                    item.asset, item.planet, item.item
                ));
            }
            if config.target == "normal" && cost.is_none() && !needs_remote_biter_eggs(item) {
                unexpected.push(format!(
                    "{}/{}/{}: normal target became infeasible under {config:?}",
                    item.asset, item.planet, item.item
                ));
            }
            let key = (
                item.asset,
                item.item.clone(),
                config.major,
                config.target,
                config.layout,
            );
            if !config.best {
                previous.insert(key, cost);
            } else if let Some(Some(baseline)) = previous.get(&key) {
                // With best modules the empty-module candidate remains available:
                // adding candidates cannot turn a solved task into an infeasible
                // one or increase the optimal cost.
                match cost {
                    Some(cost) if cost <= *baseline + baseline.abs().max(1.0) * 1e-5 => {}
                    _ => unexpected.push(format!(
                        "{key:?}: without modules={baseline}, with modules={cost:?}"
                    )),
                }
            }
        }
    }
    assert!(tested > 0, "empty quality matrix shard");
    eprintln!(
        "quality matrix totals: {solved} solved, {} failed, {tested} tested",
        tested - solved
    );
    assert!(
        unexpected.is_empty(),
        "quality matrix solver/normal-target/best-module regression(s): {unexpected:#?}"
    );
}

async fn standard_beacon_regression(item_name: &str, best: bool) {
    let _permit = GATE.acquire().await.unwrap();
    let item = import_targets()
        .iter()
        .find(|t| t.asset == BASE && t.item == item_name)
        .unwrap();
    let mut output = report_file(&format!("regression-{item_name}"), 0);
    let config = Configuration {
        major: "normal",
        target: "normal",
        best,
        layout: 1,
    };
    assert!(
        run_case(item, &config, &mut output).await.cost.is_some(),
        "locally feasible normal target failed with the standard 8/8/16 beacon layout: {item_name}, best={best}"
    );
}

/// The specialty smoke sweep previously left these cases
/// unproved (singular hints, dense row guards, or exhausted search budgets).
#[tokio::test]
#[ignore = "full specialty recovery sweep, including the unfinished legendary seed case"]
async fn specialty_smoke_internal_errors_have_certified_outcomes() {
    let _permit = GATE.acquire().await.unwrap();
    let mut output = report_file("specialty-certified-outcomes", 0);
    let mut cases = vec![
        (BASE, "recycler", "uncommon", false),
        (BASE, "overgrowth-jellynut-soil", "epic", true),
        (BASE, "overgrowth-yumako-soil", "epic", true),
        (BASE, "artificial-yumako-soil", "epic", true),
        (BASE, "overgrowth-yumako-soil", "legendary", true),
        (BASE, "overgrowth-jellynut-soil", "legendary", true),
    ];
    for target in ["rare", "epic", "legendary"] {
        for best in [false, true] {
            cases.push((MODDED, "stack-inserter", target, best));
        }
    }
    cases.push((BASE, "yumako-seed", "legendary", true));
    let mut internal_errors = Vec::new();
    for (asset, name, target, best) in cases {
        let item = import_targets()
            .iter()
            .find(|t| t.asset == asset && t.item == name)
            .unwrap();
        let config = Configuration {
            major: "normal",
            target,
            best,
            layout: 0,
        };
        let outcome = run_case(item, &config, &mut output).await;
        if outcome.internal_error {
            internal_errors.push(format!("{name}, {config:?}"));
        }
    }
    assert!(
        internal_errors.is_empty(),
        "uncertified specialty outcomes: {internal_errors:?}"
    );
}

#[tokio::test]
#[ignore = "full heavily modded asset solver regression"]
async fn modded_stack_inserter_quality_solver_regression() {
    let _permit = GATE.acquire().await.unwrap();
    let item = import_targets()
        .iter()
        .find(|t| t.asset == MODDED && t.item == "stack-inserter")
        .unwrap();
    let mut output = report_file("specialty-modded-stack", 0);
    let mut internal_errors = Vec::new();
    for target in ["rare", "epic", "legendary"] {
        for best in [false, true] {
            let config = Configuration {
                major: "normal",
                target,
                best,
                layout: 0,
            };
            if run_case(item, &config, &mut output).await.internal_error {
                internal_errors.push(format!("{config:?}"));
            }
        }
    }
    assert!(
        internal_errors.is_empty(),
        "uncertified modded stack outcomes: {internal_errors:?}"
    );
}

#[tokio::test]
async fn agricultural_science_best_modules_standard_beacons_regression() {
    standard_beacon_regression("agricultural-science-pack", true).await;
}

#[tokio::test]
async fn electromagnetic_science_no_modules_standard_beacons_regression() {
    standard_beacon_regression("electromagnetic-science-pack", false).await;
}

#[tokio::test]
async fn legendary_machines_do_not_supply_missing_product_quality() {
    let _permit = GATE.acquire().await.unwrap();
    for name in [
        "metallurgic-science-pack",
        "agricultural-science-pack",
        "electromagnetic-science-pack",
    ] {
        let item = import_targets()
            .iter()
            .find(|t| t.asset == BASE && t.item == name)
            .unwrap();
        for quality in ["uncommon", "legendary"] {
            let config = Configuration {
                major: "legendary",
                target: quality,
                best: false,
                layout: 0,
            };
            let mcp = handler(BASE);
            let ids = start(&mcp, config.arguments(item)).await;
            let state = finish(&mcp, &ids).await;
            assert_eq!(
                state["auto_plan"]["status"], "failed",
                "machines upgraded product quality without a quality source: {name}/{quality}"
            );
            assert!(
                state["auto_plan"]["error"]
                    .as_str()
                    .unwrap()
                    .starts_with("自动规划无解：无可行解"),
                "expected infeasibility, not an internal error: {state}"
            );
        }
    }
}

#[test]
fn explicit_import_locations_include_subtypes_and_exclude_aquilo() {
    let targets = import_targets();
    assert_eq!(targets.iter().filter(|t| t.asset == BASE).count(), 45);
    assert_eq!(targets.iter().filter(|t| t.asset == MODDED).count(), 1);
    assert!(targets.iter().all(|t| t.planet != "aquilo"));
    assert!(targets.iter().all(|t| t.item != "iron-plate")); // absent raw field, defaulted by ItemComponent
    for (item, kind, planet) in [
        ("yumako", "capsule", "gleba"),
        ("tesla-ammo", "ammo", "fulgora"),
        ("teslagun", "gun", "fulgora"),
        ("foundry", "item", "vulcanus"),
    ] {
        assert!(
            targets
                .iter()
                .any(|t| t.asset == BASE && t.item == item && t.kind == kind && t.planet == planet)
        );
    }
}

#[test]
fn beacon_groups_use_total_modules_and_share_building_count() {
    let prototype = store(BASE);
    let game = metatorio_core::GameState {
        qualities: prototype.quality_order().to_vec(),
        max_quality: 4,
        ..Default::default()
    };
    let ctx = metatorio_core::Context::new(prototype, &game);
    let config = |count, share, modules| ModuleConfig {
        modules: Vec::new(),
        beacons: vec![BeaconConfig {
            beacon: IdWithQuality::new("beacon", "normal"),
            count,
            share,
            modules: vec![(IdWithQuality::new("speed-module-3", "normal"), modules)],
        }],
    };
    let full = config(8, 8.0, 16);
    let sparse = config(8, 8.0, 2);
    let full_speed = full.get_effect(&ctx).speed;
    assert!(full_speed > 0.0);
    assert!((full_speed - sparse.get_effect(&ctx).speed * 8.0).abs() < 1e-9);
    assert!((full.get_consumption(&ctx) - config(1, 1.0, 2).get_consumption(&ctx)).abs() < 1e-9);
    for (count, share) in [(6, 6.0), (8, 8.0), (12, 8.0)] {
        let args = beacon_layout(count, share, "normal", "normal");
        assert_eq!(args[0]["modules"][0]["count"], count * 2);
        assert!(args[0]["share"].as_f64().unwrap() >= 1.0);
    }
}
