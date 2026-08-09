//! Metatorio 桌面壳（Tauri）。
//!
//! IPC 边界：前端只通过 command 与 Rust 交互——
//! - `dispatch(AppMessage)`：reducer（metatorio-ui 平移）→ 返回渲染投影
//! - 后续 Phase：search_prototypes / solve / undo / persist

mod message;
mod state;

use std::sync::Mutex;

use message::{AppMessage, MechanicKind};
use serde::Serialize;
use state::AppState;

/// 渲染投影：前端只消费这个结构（不持有 document 本体）。
#[derive(Debug, Clone, Serialize)]
pub struct MechanicView {
    pub id: u64,
    /// 机制类型的 kebab-case 名（"recipe" / "mining" / ...）。
    pub kind: String,
    /// 人类可读摘要（Rust 侧生成，前端不复制语义）。
    pub summary: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct AppViewState {
    pub factory_name: String,
    pub mechanics: Vec<MechanicView>,
    pub selected: Option<u64>,
}

impl From<&AppState> for AppViewState {
    fn from(app: &AppState) -> Self {
        AppViewState {
            factory_name: app.factory.name.clone(),
            mechanics: app
                .factory
                .mechanics
                .iter()
                .map(|entry| MechanicView {
                    id: entry.id,
                    kind: kind_name(entry.kind()).to_string(),
                    summary: summarize(entry),
                })
                .collect(),
            selected: app.ui.selected,
        }
    }
}

fn kind_name(kind: MechanicKind) -> &'static str {
    match kind {
        MechanicKind::Recipe => "recipe",
        MechanicKind::Mining => "mining",
        MechanicKind::Spoil => "spoil",
        MechanicKind::Plant => "plant",
        MechanicKind::ItemFuel => "item-fuel",
        MechanicKind::ItemLaunch => "item-launch",
        MechanicKind::Generator => "generator",
        MechanicKind::Boiler => "boiler",
        MechanicKind::Reactor => "reactor",
        MechanicKind::Unsupported => "unsupported",
    }
}

/// 机制摘要（第一版：kind 名；后续 Phase 填 recipe/machine 等字段）。
fn summarize(entry: &state::MechanicEntry) -> String {
    use metatorio_core::Mechanic;
    match &entry.mechanic {
        Mechanic::Recipe(r) => format!("recipe {}", r.recipe.id),
        Mechanic::Mining(m) => format!("mine {}", m.resource),
        other => format!("{:?}", other).split('(').next().unwrap_or("?").to_string(),
    }
}

#[tauri::command]
fn dispatch(app: tauri::State<'_, Mutex<AppState>>, message: AppMessage) -> AppViewState {
    let mut app = app.inner().lock().expect("state poisoned");
    app.update(message);
    AppViewState::from(&*app)
}

#[tauri::command]
fn get_view(app: tauri::State<'_, Mutex<AppState>>) -> AppViewState {
    AppViewState::from(&*app.inner().lock().expect("state poisoned"))
}

#[tauri::command]
fn hello(name: String) -> String {
    format!("Hello, {name}! (from Rust)")
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
  tauri::Builder::default()
    .manage(Mutex::new(AppState::default()))
    .invoke_handler(tauri::generate_handler![hello, dispatch, get_view])
    .setup(|app| {
      if cfg!(debug_assertions) {
        app.handle().plugin(
          tauri_plugin_log::Builder::default()
            .level(log::LevelFilter::Info)
            .build(),
        )?;
      }
      Ok(())
    })
    .run(tauri::generate_context!())
    .expect("error while running tauri application");
}
