//! Grandie Chess application core.

pub mod ai;
pub mod analysis;
pub mod coach;
pub mod db;
pub mod engine;
pub mod error;
pub mod game;
pub mod masters;
pub mod net;
pub mod paths;
pub mod secrets;
pub mod settings;
pub mod training;
pub mod voice;

mod commands;
mod events;
mod menu;
mod state;

use tauri::{Manager, RunEvent};
use tauri_specta::{collect_commands, collect_events};

/// Commands and events shared with the UI. Exported to `src/bindings.ts` on
/// debug builds and by the `bindings` test.
pub fn specta_builder() -> tauri_specta::Builder<tauri::Wry> {
    tauri_specta::Builder::<tauri::Wry>::new()
        .commands(collect_commands![
            commands::app::app_info,
            commands::ai::ai_key_status,
            commands::ai::ai_key_set,
            commands::ai::ai_key_info,
            commands::ai::ai_models,
            commands::ai::ai_voices,
            commands::ai::ai_usage,
            commands::coach::coach_list,
            commands::coach::coach_explain_move,
            commands::coach::coach_check_move,
            commands::coach::coach_explain_line,
            commands::coach::coach_walk_game,
            commands::coach::coach_ask,
            commands::coach::coach_cancel,
            commands::masters::masters_players,
            commands::masters::masters_legends,
            commands::masters::masters_download,
            commands::masters::masters_import_files,
            commands::masters::masters_remove,
            commands::masters::masters_profile,
            commands::masters::masters_analyse_style,
            commands::masters::masters_style_report,
            commands::masters::masters_walkthrough,
            commands::masters::masters_guess,
            commands::voice::voice_start,
            commands::voice::voice_stop,
            commands::voice::voice_cancel,
            commands::voice::voice_interpret,
            commands::voice::voice_speak,
            commands::app::chess_position_info,
            commands::app::chess_apply_move,
            commands::app::chess_apply_san_line,
            commands::app::settings_get,
            commands::app::settings_set,
            commands::engines::networks_list,
            commands::engines::network_download,
            commands::engines::network_delete,
            commands::engines::engine_detect_backend,
            commands::engines::engines_list,
            commands::engines::engine_add,
            commands::engines::engine_update,
            commands::engines::engine_remove,
            commands::engines::engine_install_stockfish,
            commands::engines::analysis_start,
            commands::engines::analysis_stop,
            commands::engines::analysis_backend,
            commands::play::levels_list,
            commands::play::play_missing_networks,
            commands::play::play_start,
            commands::play::play_snapshot,
            commands::play::play_move,
            commands::play::play_takeback,
            commands::play::play_resign,
            commands::play::play_offer_draw,
            commands::play::play_hint,
            commands::play::play_pgn,
            commands::play::play_close,
            commands::files::pgn_parse,
            commands::files::pgn_read_file,
            commands::files::pgn_write,
            commands::files::file_read_text,
            commands::files::file_write_text,
            commands::review::review_start,
            commands::review::review_cancel,
            commands::review::review_check_move,
            commands::db::db_query,
            commands::db::db_game,
            commands::db::db_delete,
            commands::db::db_stats,
            commands::db::db_find,
            commands::db::db_save_review,
            commands::db::db_import_files,
            commands::db::db_import_lichess,
            commands::db::db_import_chesscom,
            commands::db::db_import_grandie,
            commands::db::opening_name,
            commands::db::explorer_db,
            commands::db::explorer_lichess,
            commands::db::lichess_token_status,
            commands::db::lichess_token_set,
            commands::training::repertoire_moves,
            commands::training::repertoire_add_line,
            commands::training::repertoire_remove_move,
            commands::training::repertoire_set_comment,
            commands::training::repertoire_import_pgn,
            commands::training::repertoire_export_pgn,
            commands::training::repertoire_stats,
            commands::training::repertoire_due,
            commands::training::repertoire_grade,
            commands::training::puzzles_info,
            commands::training::puzzles_download,
            commands::training::puzzles_themes,
            commands::training::puzzle_next,
            commands::training::puzzle_check,
            commands::training::puzzle_finish,
            commands::training::progress_summary,
            commands::training::mistakes_add,
            commands::training::mistakes_due,
            commands::training::mistakes_stats,
            commands::training::mistakes_grade,
            commands::training::mistakes_remove,
        ])
        .events(collect_events![
            events::PlayUpdate,
            events::PlayClock,
            events::DownloadProgress,
            events::MenuAction,
            events::AnalysisEvent,
            events::ReviewProgress,
            events::ImportProgress,
            events::SettingsChanged,
            events::CoachEvent,
            events::VoiceLevel,
        ])
}

pub fn export_bindings(path: &str) {
    specta_builder()
        .export(specta_typescript::Typescript::default().header("// Generated by tauri-specta. Do not edit.\n// @ts-nocheck"), path)
        .expect("failed to export TypeScript bindings");
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let builder = specta_builder();
    #[cfg(debug_assertions)]
    export_bindings("../src/bindings.ts");

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .invoke_handler(builder.invoke_handler())
        .setup(move |app| {
            builder.mount_events(app);
            let data_dir = app.path().app_data_dir()?;
            let state = state::AppState::new(data_dir).map_err(|e| e.to_string())?;
            log::info!("data folder: {}", state.paths.data_dir.display());
            app.manage(state);
            menu::install(app)?;
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building Grandie Chess");

    app.run(|handle, event| {
        if let RunEvent::Exit = event {
            if let Some(state) = handle.try_state::<state::AppState>() {
                state.shutdown();
            }
        }
    });
}
