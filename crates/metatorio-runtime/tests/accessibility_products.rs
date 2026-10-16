use metatorio_core::Accessible;
use metatorio_data::store::PrototypeStore;
use metatorio_runtime::*;
use serde_json::json;

#[test]
fn cached_accessibility_tracks_product_dependencies_after_milestone_changes_and_undo() {
    let dump = json!({
        "item": {"fuel": {"fuel_value": "1MJ", "fuel_categories": ["chemical"], "burnt_result": "ash"}, "ash": {}, "pack": {}},
        "fluid": {"water": {}},
        "tile": {"lake": {"fluid": "water"}},
        "planet": {"nauvis": {"map_gen_settings": {"autoplace_settings": {"tile": {"settings": {"lake": {}}}}}}},
        "recipe": {
            "fuel": {"enabled": true, "results": [{"type": "item", "name": "fuel", "amount": 1}]},
            "pack": {"enabled": true, "ingredients": [{"type": "item", "name": "ash", "amount": 1}, {"type": "fluid", "name": "water", "amount": 1}], "results": [{"type": "item", "name": "pack", "amount": 1}]}
        },
        "technology": {"advanced": {"unit": {"count": 1, "time": 1, "ingredients": [["pack", 1]]}}}
    });
    let mut runtime = Runtime::new();
    runtime.install_context("test".into(), PrototypeStore::load(&dump).unwrap());
    runtime.set_active_context(Some("test".into()));
    runtime
        .dispatch(AppMessage::Application(ApplicationAction::NewProject {
            name: "test".into(),
        }))
        .unwrap();
    let project = runtime.state.document.projects[0].id;
    let check = |runtime: &Runtime, ash, water, tech| {
        // 第二次读取经过结果缓存，记录的集合也必须一致。
        for _ in 0..2 {
            let result = runtime.project_accessibility(project).unwrap();
            assert_eq!(result.is_item_accessible("ash"), ash);
            assert_eq!(
                result.is_accessible(&Accessible::Fluid("water".into())),
                water
            );
            assert_eq!(
                result.is_accessible(&Accessible::Tech("advanced".into())),
                tech
            );
        }
    };
    check(&runtime, true, true, true);
    runtime
        .dispatch(AppMessage::Project {
            project,
            action: ProjectAction::AddMilestone {
                node: Accessible::Item("fuel".into()),
                unlocked: false,
            },
        })
        .unwrap();
    check(&runtime, false, true, false);
    runtime
        .dispatch(AppMessage::History(HistoryAction::Undo))
        .unwrap();
    check(&runtime, true, true, true);
    runtime
        .dispatch(AppMessage::History(HistoryAction::Redo))
        .unwrap();
    check(&runtime, false, true, false);
    runtime
        .dispatch(AppMessage::Project {
            project,
            action: ProjectAction::RemoveMilestone {
                node: Accessible::Item("fuel".into()),
            },
        })
        .unwrap();
    runtime
        .dispatch(AppMessage::Project {
            project,
            action: ProjectAction::AddMilestone {
                node: Accessible::Planet("nauvis".into()),
                unlocked: false,
            },
        })
        .unwrap();
    check(&runtime, true, false, false);
    runtime
        .dispatch(AppMessage::History(HistoryAction::Undo))
        .unwrap();
    check(&runtime, true, true, true);
}
