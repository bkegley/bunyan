use tauri::State;

use crate::db;
use crate::models::Setting;
use crate::state::AppState;

#[tauri::command]
#[specta::specta]
pub fn get_setting(state: State<AppState>, key: String) -> Result<Setting, String> {
    let conn = state.db.lock().unwrap();
    db::settings::get(&conn, &key).map_err(|e| e.into())
}

#[tauri::command]
#[specta::specta]
pub fn set_setting(
    state: State<AppState>,
    key: String,
    value: String,
) -> Result<Setting, String> {
    let conn = state.db.lock().unwrap();
    db::settings::set(&conn, &key, &value).map_err(|e| e.into())
}

#[tauri::command]
#[specta::specta]
pub fn get_all_settings(state: State<AppState>) -> Result<Vec<Setting>, String> {
    let conn = state.db.lock().unwrap();
    db::settings::get_all(&conn).map_err(|e| e.into())
}
