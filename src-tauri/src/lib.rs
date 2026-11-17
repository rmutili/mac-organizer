use tauri::Manager;

mod claude;
mod commands;
mod ops;
mod scanner;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(commands::AppState::default())
        .invoke_handler(tauri::generate_handler![
            commands::scan::scan_downloads,
            commands::classify::set_api_key,
            commands::classify::claude_backends,
            commands::classify::clear_api_key,
            commands::classify::classify_downloads,
            commands::classify::cancel_classify,
            commands::ops::apply_plan,
            commands::ops::last_apply,
            commands::ops::undo_last_apply,
            commands::ops::thumbnail,
            commands::ops::quick_look,
            commands::ops::reveal_in_finder,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            // Don't leave `claude -p` batches running after the window closes.
            if let tauri::RunEvent::Exit = event {
                app.state::<commands::AppState>().processes.kill_all();
            }
        });
}
