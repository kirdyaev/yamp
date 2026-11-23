#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod keychain;
mod sidecar;

use keychain::TokenStatus;
use sidecar::SidecarManager;
use std::sync::Mutex;
use tauri::{Emitter, Manager, RunEvent, WebviewUrl, WebviewWindowBuilder};

struct AppState {
    sidecar: Mutex<SidecarManager>,
}

fn percent_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for byte in input.as_bytes() {
        let b = *byte;
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

#[tauri::command]
fn get_token_status() -> Result<TokenStatus, String> {
    keychain::token_status()
}

#[tauri::command]
fn save_token(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    token: String,
) -> Result<TokenStatus, String> {
    keychain::save_token(&token)?;
    let trimmed = token.trim().to_string();
    {
        let mut mgr = state
            .sidecar
            .lock()
            .map_err(|_| "Sidecar lock poisoned".to_string())?;
        mgr.update_token(Some(trimmed))?;
    }
    let status = keychain::token_status()?;
    let _ = app.emit("token-status-changed", &status);
    Ok(status)
}

#[tauri::command]
fn clear_token(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<TokenStatus, String> {
    keychain::delete_token()?;
    {
        let mut mgr = state
            .sidecar
            .lock()
            .map_err(|_| "Sidecar lock poisoned".to_string())?;
        let _ = mgr.update_token(None);
    }
    let status = keychain::token_status()?;
    let _ = app.emit("token-status-changed", &status);
    Ok(status)
}

#[tauri::command]
fn open_settings_window(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("settings") {
        let _ = win.unminimize();
        let _ = win.show();
        let _ = win.set_focus();
        return Ok(());
    }

    WebviewWindowBuilder::new(&app, "settings", WebviewUrl::App("settings.html".into()))
        .title("Yamamp — Settings")
        .inner_size(520.0, 420.0)
        .resizable(false)
        .center()
        .build()
        .map_err(|e| format!("Failed to open Settings window: {e}"))?;
    Ok(())
}

#[tauri::command]
fn search_tracks(
    state: tauri::State<'_, AppState>,
    query: String,
    page: Option<u32>,
) -> Result<serde_json::Value, String> {
    let encoded_q = percent_encode(query.trim());
    let path = format!("/api/search?q={encoded_q}&page={}", page.unwrap_or(0));
    let mut mgr = state
        .sidecar
        .lock()
        .map_err(|_| "Sidecar lock poisoned".to_string())?;
    mgr.get_json(&path)
}

#[tauri::command]
fn get_playlists(state: tauri::State<'_, AppState>) -> Result<serde_json::Value, String> {
    let mut mgr = state
        .sidecar
        .lock()
        .map_err(|_| "Sidecar lock poisoned".to_string())?;
    mgr.get_json("/api/playlists")
}

#[tauri::command]
fn get_playlist_tracks(
    state: tauri::State<'_, AppState>,
    owner_id: String,
    kind: i64,
) -> Result<serde_json::Value, String> {
    let encoded_owner = percent_encode(owner_id.trim());
    let path = format!("/api/playlists/{encoded_owner}/{kind}");
    let mut mgr = state
        .sidecar
        .lock()
        .map_err(|_| "Sidecar lock poisoned".to_string())?;
    mgr.get_json(&path)
}

#[tauri::command]
fn get_liked_tracks(state: tauri::State<'_, AppState>) -> Result<serde_json::Value, String> {
    let mut mgr = state
        .sidecar
        .lock()
        .map_err(|_| "Sidecar lock poisoned".to_string())?;
    mgr.get_json("/api/likes")
}

#[tauri::command]
fn get_my_wave(
    state: tauri::State<'_, AppState>,
    queue: Option<String>,
) -> Result<serde_json::Value, String> {
    let path = match queue {
        Some(q) if !q.trim().is_empty() => format!("/api/wave?queue={}", percent_encode(q.trim())),
        _ => "/api/wave".to_string(),
    };
    let mut mgr = state
        .sidecar
        .lock()
        .map_err(|_| "Sidecar lock poisoned".to_string())?;
    mgr.get_json(&path)
}

fn main() {
    let app = tauri::Builder::default()
        .manage(AppState {
            sidecar: Mutex::new(SidecarManager::new()),
        })
        .setup(|app| {
            let initial_token = keychain::load_token().unwrap_or(None);
            let state = app.state::<AppState>();
            if let Ok(mut mgr) = state.sidecar.lock() {
                if let Err(err) = mgr.start(initial_token) {
                    eprintln!("Yamamp sidecar startup warning: {err}");
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_token_status,
            save_token,
            clear_token,
            open_settings_window,
            search_tracks,
            get_playlists,
            get_playlist_tracks,
            get_liked_tracks,
            get_my_wave,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Yamamp");

    app.run(|app_handle, event| {
        if matches!(event, RunEvent::Exit | RunEvent::ExitRequested { .. }) {
            let state = app_handle.state::<AppState>();
            if let Ok(mut mgr) = state.sidecar.lock() {
                mgr.stop();
            };
        }
    });
}
