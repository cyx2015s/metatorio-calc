//! `metatorio_shell::app::allowed_modules` 集成测试：通过 `load_dump` 载入合成原型仓库，
//! 直接覆盖 CraftingMachine / Beacon 及配方禁用效果过滤。
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use metatorio_shell::app::{AppState, allowed_modules, load_dump, restore_contexts};
use metatorio_shell::host::Host;
use serde_json::json;

struct TestHost {
    state: AppState,
    dir: PathBuf,
}

impl Host for TestHost {
    fn state(&self) -> &AppState {
        &self.state
    }

    fn app_data_dir(&self) -> PathBuf {
        self.dir.clone()
    }

    fn emit(&self, _event: &str, _payload: serde_json::Value) {}
}

fn synthetic_dump_json() -> serde_json::Value {
    json!({
        "module": {
            "speed-module": {
                "name": "speed-module",
                "type": "module",
                "category": "speed",
                "tier": 1,
                "effect": { "speed": 0.2, "consumption": 0.5 }
            },
            "efficiency-module": {
                "name": "efficiency-module",
                "type": "module",
                "category": "efficiency",
                "tier": 1,
                "effect": { "consumption": -0.3 }
            },
            "productivity-module": {
                "name": "productivity-module",
                "type": "module",
                "category": "productivity",
                "tier": 1,
                "effect": { "productivity": 0.04, "consumption": 0.4, "pollution": 0.05 }
            },
            "quality-module": {
                "name": "quality-module",
                "type": "module",
                "category": "quality",
                "tier": 1,
                "effect": { "quality": 0.01, "speed": -0.05 }
            }
        },
        "recipe": {
            "electronic-circuit": {
                "type": "recipe",
                "name": "electronic-circuit",
                "allow_speed": true,
                "allow_efficiency": true,
                "allow_consumption": true,
                "allow_pollution": true,
                "allow_productivity": false,
                "allow_quality": true
            }
        },
        "assembling-machine": {
            "assembling-machine-1": {
                "name": "assembling-machine-1",
                "type": "assembling-machine",
                "crafting_speed": 0.5,
                "crafting_categories": ["crafting"],
                "module_slots": 2,
                "allowed_effects": ["speed", "consumption"],
                "energy_usage": "75kW",
                "energy_source": { "type": "electric", "usage_priority": "secondary-input" }
            }
        },
        "beacon": {
            "beacon": {
                "name": "beacon",
                "type": "beacon",
                "allowed_effects": ["speed", "consumption"],
                "allowed_module_categories": ["speed", "efficiency"],
                "energy_usage": "480kW",
                "energy_source": { "type": "electric", "usage_priority": "secondary-input" }
            }
        }
    })
}

#[tokio::test]
async fn allowed_modules_filters_by_machine_and_recipe() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "metatorio-allowed-modules-{}-{unique}",
        std::process::id()
    ));
    let app_dir = root.join("app_data");
    std::fs::create_dir_all(&app_dir).unwrap();
    let dump_path = root.join("dump.json");
    std::fs::write(&dump_path, synthetic_dump_json().to_string()).unwrap();

    let host: Arc<dyn Host> = Arc::new(TestHost {
        state: AppState::default(),
        dir: app_dir,
    });
    restore_contexts(&host);
    let ctx = load_dump(&host, dump_path.to_string_lossy().into_owned())
        .await
        .expect("load_dump");

    let allowed = allowed_modules(
        &host,
        ctx.id.clone(),
        "crafting-machine".to_string(),
        "assembling-machine-1".to_string(),
        Some("electronic-circuit".to_string()),
    )
    .await
    .expect("allowed_modules");

    assert_eq!(allowed, vec!["efficiency-module", "speed-module"]);

    let beacon_allowed = allowed_modules(
        &host,
        ctx.id,
        "beacon".to_string(),
        "beacon".to_string(),
        None,
    )
    .await
    .expect("beacon allowed_modules");

    assert_eq!(beacon_allowed, vec!["efficiency-module", "speed-module"]);
    let _ = std::fs::remove_dir_all(&root);
}
