//! One registry drives runtime commands and generated TypeScript contracts.
use super::*;
use serde::Serialize;
use specta::Type;
use tauri_specta::{Builder, ErrorHandlingMode};

macro_rules! event_contracts {
    ($($name:ident($payload:ty) => $wire:literal),* $(,)?) => {
        $(
            #[derive(Debug, Clone, Serialize, Type, tauri_specta::Event)]
            #[serde(transparent)]
            #[tauri_specta(event_name = $wire)]
            pub struct $name(pub $payload);
        )*
        fn register_events(builder: Builder<tauri::Wry>) -> Builder<tauri::Wry> {
            builder.events(tauri_specta::collect_events![$($name),*])
        }
    };
}

event_contracts! {
    SolveResultEvent(metatorio_runtime::SolveResult) => "solve-result",
    SolveErrorEvent(metatorio_shell::events::SolveErrorPayload) => "solve-error",
    UniverseSolveResultEvent(metatorio_runtime::UniverseSolveResult) => "universe-solve-result",
    UniverseSolveErrorEvent(metatorio_shell::events::UniverseSolveError) => "universe-solve-error",
    AutoPlanProgressNotification(metatorio_shell::events::AutoPlanProgressEvent) => "auto-plan-progress",
    ContextsChangedEvent(ContextList) => "contexts-changed",
    ContextErrorEvent(String) => "context-error",
    DocumentChangedEvent(Option<u64>) => "document-changed",
    CommandErrorEvent(Vec<metatorio_shell::events::EventError>) => "command-error",
    ProjectOpenRequestedEvent(()) => "project-open-requested",
}

pub(super) fn builder() -> Builder<tauri::Wry> {
    register_events(
        Builder::new()
            .commands(tauri_specta::collect_commands![
                load_bundled_dump,
                load_game_context,
                load_dump,
                list_contexts,
                pick_game_executable,
                pick_dump_file,
                pick_mod_dir,
                icon,
                catalog_index,
                prototype_detail,
                suggest,
                implicit_sources,
                mechanic_flow,
                solar_balance,
                allowed_modules,
                dispatch,
                solve_universe,
                auto_plan_universe,
                universe_preview,
                get_document,
                history_state,
                accessibility,
                milestones_ordered,
                productivity,
                pick_project_file,
                pick_project_save_path,
                begin_network_png,
                write_network_png_tile,
                finish_network_png,
                abort_network_png,
                project_save_path,
                open_project_file,
                take_project_open_requests,
            ])
            // Preserve Tauri's existing promise rejection and JSON number wire contract.
            .error_handling(ErrorHandlingMode::Throw)
            .dangerously_cast_bigints_to_number(),
    )
}

#[cfg(test)]
mod tests {
    fn bindings_path() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/lib/runtime/bindings.ts")
    }

    #[test]
    #[ignore = "explicitly regenerate checked-in frontend bindings"]
    fn export_frontend_bindings() {
        super::builder()
            .export(specta_typescript::Typescript::default(), bindings_path())
            .expect("export IPC bindings");
    }

    #[test]
    fn bindings_are_current() {
        let path =
            std::env::temp_dir().join(format!("metatorio-bindings-{}.ts", std::process::id()));
        super::builder()
            .export(specta_typescript::Typescript::default(), &path)
            .expect("render IPC bindings");
        let generated = std::fs::read_to_string(&path).expect("read generated bindings");
        std::fs::remove_file(path).expect("remove temporary bindings");
        let checked_in =
            std::fs::read_to_string(bindings_path()).expect("read checked-in bindings");
        assert_eq!(
            checked_in.replace("\r\n", "\n"),
            generated.replace("\r\n", "\n"),
            "IPC bindings changed; run pnpm bindings in metatorio-app and commit the result"
        );
    }
}
