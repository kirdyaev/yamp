#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod keychain;
mod sidecar;

use keychain::TokenStatus;
use sidecar::SidecarManager;
use std::io::Read;
use std::sync::Mutex;
use std::time::Duration;
use tauri::http::{header, Method, Response, StatusCode};
use tauri::{Emitter, Manager, RunEvent, WebviewUrl, WebviewWindowBuilder, WindowEvent};

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

fn cors_empty_response(status: StatusCode) -> Response<Vec<u8>> {
    Response::builder()
        .status(status)
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .header(header::ACCESS_CONTROL_ALLOW_METHODS, "GET, HEAD, OPTIONS")
        .header(header::ACCESS_CONTROL_ALLOW_HEADERS, "Range")
        .header(
            header::ACCESS_CONTROL_EXPOSE_HEADERS,
            "Content-Length, Content-Range, Accept-Ranges",
        )
        .header(header::CONTENT_LENGTH, "0")
        .body(Vec::new())
        .unwrap_or_else(|_| Response::new(Vec::new()))
}

fn sidecar_api_get(state: &tauri::State<'_, AppState>, path: &str) -> Result<serde_json::Value, String> {
    let (port, secret) = {
        let mut mgr = state
            .sidecar
            .lock()
            .map_err(|_| "Sidecar lock poisoned".to_string())?;
        mgr.api_endpoint()?
    };
    sidecar::get_json_from(port, &secret, path)
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
    let (port, secret) = {
        let mut mgr = state
            .sidecar
            .lock()
            .map_err(|_| "Sidecar lock poisoned".to_string())?;
        mgr.api_endpoint()?
    };
    sidecar::update_token_at(port, &secret, Some(trimmed))?;
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
    if let Ok(mut mgr) = state.sidecar.lock() {
        if let Ok((port, secret)) = mgr.api_endpoint() {
            drop(mgr);
            let _ = sidecar::update_token_at(port, &secret, None);
        }
    }
    let status = keychain::token_status()?;
    let _ = app.emit("token-status-changed", &status);
    Ok(status)
}

#[tauri::command]
fn open_library_window(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("library") {
        let _ = win.unminimize();
        let _ = win.show();
        let _ = win.set_focus();
        return Ok(());
    }

    let mut builder =
        WebviewWindowBuilder::new(&app, "library", WebviewUrl::App("library.html".into()))
            .title("Yamamp — Library")
            .inner_size(460.0, 544.0)
            .min_inner_size(380.0, 360.0)
            .resizable(true);

    if let Some(main_win) = app.get_webview_window("main") {
        if let (Ok(pos), Ok(scale)) = (main_win.outer_position(), main_win.scale_factor()) {
            let logical_x = (pos.x as f64) / scale;
            let logical_y = (pos.y as f64) / scale;
            builder = builder.position(logical_x + 283.0, logical_y);
        } else {
            builder = builder.center();
        }
    } else {
        builder = builder.center();
    }

    builder
        .build()
        .map_err(|e| format!("Failed to open Library window: {e}"))?;
    Ok(())
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
        .inner_size(440.0, 350.0)
        .resizable(false)
        .center()
        .build()
        .map_err(|e| format!("Failed to open Settings window: {e}"))?;
    Ok(())
}

#[tauri::command]
fn quit_app(app: tauri::AppHandle) {
    app.exit(0);
}

#[tauri::command]
fn minimize_app(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("main") {
        win.minimize()
            .map_err(|e| format!("Failed to minimize window: {e}"))?;
    }
    Ok(())
}

#[tauri::command]
fn play_tracks_in_player(
    app: tauri::AppHandle,
    tracks: serde_json::Value,
    start_index: usize,
) -> Result<(), String> {
    app.emit_to(
        "main",
        "yamamp:play-tracks",
        serde_json::json!({
            "tracks": tracks,
            "startIndex": start_index,
        }),
    )
    .map_err(|e| format!("Failed to send tracks to player: {e}"))
}

#[tauri::command]
fn queue_track_in_player(app: tauri::AppHandle, track: serde_json::Value) -> Result<(), String> {
    app.emit_to("main", "yamamp:queue-track", track)
        .map_err(|e| format!("Failed to queue track in player: {e}"))
}

#[tauri::command]
fn append_tracks_in_player(
    app: tauri::AppHandle,
    tracks: serde_json::Value,
) -> Result<(), String> {
    app.emit_to("main", "yamamp:append-tracks", tracks)
        .map_err(|e| format!("Failed to append tracks in player: {e}"))
}

#[tauri::command]
fn search_tracks(
    state: tauri::State<'_, AppState>,
    query: String,
    page: Option<u32>,
) -> Result<serde_json::Value, String> {
    let encoded_q = percent_encode(query.trim());
    let path = format!("/api/search?q={encoded_q}&page={}", page.unwrap_or(0));
    sidecar_api_get(&state, &path)
}

#[tauri::command]
fn get_playlists(state: tauri::State<'_, AppState>) -> Result<serde_json::Value, String> {
    sidecar_api_get(&state, "/api/playlists")
}

#[tauri::command]
fn get_playlist_tracks(
    state: tauri::State<'_, AppState>,
    owner_id: String,
    kind: i64,
) -> Result<serde_json::Value, String> {
    let encoded_owner = percent_encode(owner_id.trim());
    let path = format!("/api/playlists/{encoded_owner}/{kind}");
    sidecar_api_get(&state, &path)
}

#[tauri::command]
fn get_liked_tracks(state: tauri::State<'_, AppState>) -> Result<serde_json::Value, String> {
    sidecar_api_get(&state, "/api/likes")
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
    sidecar_api_get(&state, &path)
}

fn main() {
    let app = tauri::Builder::default()
        .manage(AppState {
            sidecar: Mutex::new(SidecarManager::new()),
        })
        .register_asynchronous_uri_scheme_protocol("yamamp", |ctx, request, responder| {
            let method = request.method().clone();
            if method == Method::OPTIONS {
                responder.respond(cors_empty_response(StatusCode::NO_CONTENT));
                return;
            }
            if method != Method::GET && method != Method::HEAD {
                responder.respond(cors_empty_response(StatusCode::METHOD_NOT_ALLOWED));
                return;
            }

            let path = request.uri().path().to_string();
            let Some(raw_track_id) = path.strip_prefix("/stream/").map(str::to_string) else {
                responder.respond(cors_empty_response(StatusCode::NOT_FOUND));
                return;
            };
            if raw_track_id.is_empty() {
                responder.respond(cors_empty_response(StatusCode::BAD_REQUEST));
                return;
            }

            let range_header = request
                .headers()
                .get(header::RANGE)
                .and_then(|v| v.to_str().ok())
                .map(str::to_string);
            let is_head = method == Method::HEAD;
            let app_handle = ctx.app_handle().clone();

            std::thread::spawn(move || {
                let endpoint = {
                    let state = app_handle.state::<AppState>();
                    let result = match state.sidecar.lock() {
                        Ok(mut mgr) => mgr.stream_endpoint(),
                        Err(_) => Err("Sidecar lock poisoned".to_string()),
                    };
                    result
                };

                let (port, stream_nonce) = match endpoint {
                    Ok(v) => v,
                    Err(_) => {
                        eprintln!("[yamamp-proto] sidecar_unavailable");
                        responder.respond(cors_empty_response(StatusCode::BAD_GATEWAY));
                        return;
                    }
                };

                let url = format!("http://127.0.0.1:{port}/s/{stream_nonce}/stream/{raw_track_id}");
                let agent = ureq::AgentBuilder::new()
                    .timeout(Duration::from_secs(30))
                    .build();

                let mut req = if is_head {
                    agent.head(&url)
                } else {
                    agent.get(&url)
                };
                if let Some(ref rng) = range_header {
                    req = req.set("Range", rng);
                }

                match req.call() {
                    Ok(resp) => {
                        let status =
                            StatusCode::from_u16(resp.status()).unwrap_or(StatusCode::OK);
                        let raw_ct = resp
                            .header("Content-Type")
                            .unwrap_or("audio/mpeg")
                            .split(';')
                            .next()
                            .unwrap_or("audio/mpeg")
                            .trim()
                            .to_ascii_lowercase();
                        let content_type = if raw_ct.starts_with("audio/") {
                            raw_ct
                        } else {
                            "audio/mpeg".to_string()
                        };
                        let accept_ranges = resp
                            .header("Accept-Ranges")
                            .unwrap_or("bytes")
                            .to_string();
                        let content_range = resp.header("Content-Range").map(str::to_string);
                        let upstream_len = resp.header("Content-Length").map(str::to_string);

                        let mut body = Vec::new();
                        if !is_head {
                            const MAX_BODY: u64 = 32 * 1024 * 1024;
                            if resp.into_reader().take(MAX_BODY).read_to_end(&mut body).is_err() {
                                eprintln!("[yamamp-proto] body_read_error");
                                responder.respond(cors_empty_response(StatusCode::BAD_GATEWAY));
                                return;
                            }
                        }

                        let mut builder = Response::builder()
                            .status(status)
                            .header(header::CONTENT_TYPE, content_type)
                            .header(header::ACCEPT_RANGES, accept_ranges)
                            .header(header::CACHE_CONTROL, "no-store")
                            .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
                            .header(header::ACCESS_CONTROL_ALLOW_METHODS, "GET, HEAD, OPTIONS")
                            .header(header::ACCESS_CONTROL_ALLOW_HEADERS, "Range")
                            .header(
                                header::ACCESS_CONTROL_EXPOSE_HEADERS,
                                "Content-Length, Content-Range, Accept-Ranges",
                            );

                        if let Some(cr) = content_range {
                            builder = builder.header(header::CONTENT_RANGE, cr);
                        }
                        if is_head {
                            if let Some(cl) = upstream_len {
                                builder = builder.header(header::CONTENT_LENGTH, cl);
                            }
                        } else {
                            builder = builder.header(header::CONTENT_LENGTH, body.len().to_string());
                        }

                        match builder.body(body) {
                            Ok(r) => responder.respond(r),
                            Err(_) => {
                                responder.respond(cors_empty_response(StatusCode::INTERNAL_SERVER_ERROR))
                            }
                        }
                    }
                    Err(ureq::Error::Status(code, resp)) => {
                        eprintln!("[yamamp-proto] upstream_status={code}");
                        let status =
                            StatusCode::from_u16(code).unwrap_or(StatusCode::BAD_GATEWAY);
                        let mut builder = Response::builder()
                            .status(status)
                            .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
                            .header(header::ACCESS_CONTROL_ALLOW_METHODS, "GET, HEAD, OPTIONS")
                            .header(header::ACCESS_CONTROL_ALLOW_HEADERS, "Range")
                            .header(
                                header::ACCESS_CONTROL_EXPOSE_HEADERS,
                                "Content-Length, Content-Range, Accept-Ranges",
                            )
                            .header(header::CONTENT_LENGTH, "0");
                        if let Some(cr) = resp.header("Content-Range") {
                            builder = builder.header(header::CONTENT_RANGE, cr);
                        }
                        let response = builder
                            .body(Vec::new())
                            .unwrap_or_else(|_| cors_empty_response(status));
                        responder.respond(response);
                    }
                    Err(ureq::Error::Transport(_)) => {
                        eprintln!("[yamamp-proto] transport_error");
                        responder.respond(cors_empty_response(StatusCode::BAD_GATEWAY));
                    }
                }
            });
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
        .on_window_event(|window, event| {
            if window.label() == "main" && matches!(event, WindowEvent::Destroyed) {
                window.app_handle().exit(0);
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_token_status,
            save_token,
            clear_token,
            open_library_window,
            open_settings_window,
            quit_app,
            minimize_app,
            play_tracks_in_player,
            queue_track_in_player,
            append_tracks_in_player,
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
