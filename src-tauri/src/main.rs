#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::{DateTime, Local, Utc};
use serde::{Deserialize, Serialize};
use tauri::{
    menu::{IsMenuItem, Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
    AppHandle, Manager, TitleBarStyle, WebviewUrl, WebviewWindowBuilder, WindowEvent,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Break {
    start_time: DateTime<Utc>,
    end_time: Option<DateTime<Utc>>,
    duration: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Session {
    topic: String,
    start_time: DateTime<Utc>,
    duration: u32,
    completed: bool,
    breaks: Vec<Break>,
    total_break_time: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DailyStats {
    date: String,
    sessions: u32,
    total_focus_seconds: u32,
    total_break_seconds: u32,
    break_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AppData {
    sessions: Vec<Session>,
    daily_stats: Vec<DailyStats>,
}

impl Default for AppData {
    fn default() -> Self {
        Self {
            sessions: Vec::new(),
            daily_stats: Vec::new(),
        }
    }
}

// Simple timer state - clean and straightforward
struct AppState {
    data: Mutex<AppData>,
    current_session: Mutex<Option<Session>>,
    current_break: Mutex<Option<Break>>,

    // Timer values (in deciseconds for smooth updates)
    focus_time_remaining: Mutex<u32>, // deciseconds (5400 * 10 = 54000 for 90 min)
    break_time_elapsed: Mutex<u32>,   // deciseconds

    is_on_break: Mutex<bool>,
    is_paused: Mutex<bool>,
    timer_running: Mutex<bool>,

    // Sleep/wake detection
    sleep_start_time: Mutex<Option<DateTime<Utc>>>,
    was_running_before_sleep: Mutex<bool>,

    data_path: PathBuf,
}

impl AppState {
    fn new() -> Self {
        let data_path = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".luma")
            .join("data.json");

        if let Some(parent) = data_path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        let data = if data_path.exists() {
            fs::read_to_string(&data_path)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default()
        } else {
            AppData::default()
        };

        Self {
            data: Mutex::new(data),
            current_session: Mutex::new(None),
            current_break: Mutex::new(None),
            focus_time_remaining: Mutex::new(0),
            break_time_elapsed: Mutex::new(0),
            is_on_break: Mutex::new(false),
            is_paused: Mutex::new(false),
            timer_running: Mutex::new(false),
            sleep_start_time: Mutex::new(None),
            was_running_before_sleep: Mutex::new(false),
            data_path,
        }
    }

    fn save(&self) {
        if let Ok(data) = self.data.lock() {
            if let Ok(json) = serde_json::to_string_pretty(&*data) {
                let _ = fs::write(&self.data_path, json);
            }
        }
    }

    fn get_today_stats(&self) -> (u32, u32, u32) {
        let today = Local::now().format("%Y-%m-%d").to_string();
        let data = self.data.lock().unwrap();

        if let Some(stats) = data.daily_stats.iter().find(|s| s.date == today) {
            (stats.sessions, stats.total_focus_seconds, stats.break_count)
        } else {
            (0, 0, 0)
        }
    }

    fn get_streak(&self) -> u32 {
        let data = self.data.lock().unwrap();
        let mut streak = 0;
        let mut current_date = Local::now().date_naive();

        loop {
            let date_str = current_date.format("%Y-%m-%d").to_string();
            if data
                .daily_stats
                .iter()
                .any(|s| s.date == date_str && s.sessions > 0)
            {
                streak += 1;
                current_date = current_date.pred_opt().unwrap();
            } else {
                break;
            }
        }

        streak
    }

    fn add_session(&self, session: Session) {
        let mut data = self.data.lock().unwrap();
        let date = session
            .start_time
            .with_timezone(&Local)
            .format("%Y-%m-%d")
            .to_string();

        data.sessions.push(session.clone());

        if let Some(stats) = data.daily_stats.iter_mut().find(|s| s.date == date) {
            stats.sessions += 1;
            stats.total_focus_seconds += session.duration;
            stats.total_break_seconds += session.total_break_time;
            stats.break_count += session.breaks.len() as u32;
        } else {
            data.daily_stats.push(DailyStats {
                date,
                sessions: 1,
                total_focus_seconds: session.duration,
                total_break_seconds: session.total_break_time,
                break_count: session.breaks.len() as u32,
            });
        }

        drop(data);
        self.save();
    }
}

#[derive(Debug, Clone, Serialize)]
struct TimerState {
    focus_seconds: u32,
    break_seconds: u32,
    is_on_break: bool,
    is_paused: bool,
}

#[derive(Debug, Clone, Serialize)]
struct MenuStats {
    sessions_today: u32,
    time_today: String,
    streak: u32,
    breaks_today: u32,
}

#[tauri::command]
fn start_focus_session(state: tauri::State<Arc<AppState>>, app: AppHandle) -> Result<(), String> {
    let mut current = state.current_session.lock().map_err(|e| e.to_string())?;

    if current.is_some() {
        return Err("Session already in progress".to_string());
    }

    let session = Session {
        topic: "Study Session".to_string(),
        start_time: Utc::now(),
        duration: 0,
        completed: false,
        breaks: Vec::new(),
        total_break_time: 0,
    };

    *current = Some(session);
    *state.focus_time_remaining.lock().unwrap() = 90 * 60 * 10; // 90 minutes in deciseconds
    *state.break_time_elapsed.lock().unwrap() = 0;
    *state.is_on_break.lock().unwrap() = false;
    *state.is_paused.lock().unwrap() = true; // Start paused for topic input

    // Create window
    if let Some(window) = app.get_webview_window("focus") {
        let _ = window.show();
        let _ = window.set_focus();
    } else {
        WebviewWindowBuilder::new(&app, "focus", WebviewUrl::App("focus.html".into()))
            .title("Focus - 90 min")
            .inner_size(400.0, 550.0)
            .resizable(false)
            .title_bar_style(TitleBarStyle::Overlay)
            .hidden_title(true)
            .build()
            .map_err(|e| format!("Window error: {:?}", e))?;
    }

    // Start timer loop ONLY if not already running
    let mut timer_running = state.timer_running.lock().unwrap();
    if !*timer_running {
        *timer_running = true;
        drop(timer_running); // Release lock before spawning

        let state_clone = state.inner().clone();
        tauri::async_runtime::spawn(async move {
            run_timer(state_clone).await;
        });

        // Start auto-save
        let state_clone = state.inner().clone();
        tauri::async_runtime::spawn(async move {
            auto_save_loop(state_clone).await;
        });
    }

    // update_menu(&app, &state); // DISABLED FOR STABILITY

    Ok(())
}

#[tauri::command]
fn set_session_topic(
    state: tauri::State<Arc<AppState>>,
    _app: AppHandle,
    topic: String,
) -> Result<(), String> {
    let mut current = state.current_session.lock().map_err(|e| e.to_string())?;

    if let Some(ref mut session) = *current {
        session.topic = if topic.is_empty() {
            "Untitled Session".to_string()
        } else {
            topic
        };

        // Also update window title if possible
        // We can't access AppHandle easily here without passing it, but it's fine for now
    }

    // Start the timer now that topic is set
    *state.is_paused.lock().unwrap() = false;

    // We need AppHandle to update menu. For now, since we can't easily get it here without changing signature,
    // let's rely on the next polling or event? No, let's inject AppHandle.
    // Actually, set_session_topic now needs AppHandle.

    // update_menu(&app, &state); // DISABLED FOR STABILITY

    Ok(())
}

#[tauri::command]
fn start_break(state: tauri::State<Arc<AppState>>, _app: AppHandle) -> Result<(), String> {
    let mut is_on_break = state.is_on_break.lock().map_err(|e| e.to_string())?;
    let mut current_break = state.current_break.lock().map_err(|e| e.to_string())?;

    if *is_on_break {
        return Err("Already on break".to_string());
    }

    let break_obj = Break {
        start_time: Utc::now(),
        end_time: None,
        duration: 0,
    };

    *current_break = Some(break_obj);
    *is_on_break = true;
    *state.break_time_elapsed.lock().unwrap() = 0;

    // update_menu(&app, &state); // DISABLED FOR STABILITY

    Ok(())
}

#[tauri::command]
fn end_break(state: tauri::State<Arc<AppState>>, _app: AppHandle) -> Result<(), String> {
    let mut is_on_break = state.is_on_break.lock().map_err(|e| e.to_string())?;
    let mut current_break = state.current_break.lock().map_err(|e| e.to_string())?;
    let mut current_session = state.current_session.lock().map_err(|e| e.to_string())?;

    if !*is_on_break {
        return Err("Not on break".to_string());
    }

    if let Some(mut break_obj) = current_break.take() {
        let break_deciseconds = *state.break_time_elapsed.lock().unwrap();
        let break_seconds = break_deciseconds / 10;
        break_obj.end_time = Some(Utc::now());
        break_obj.duration = break_seconds;

        if let Some(ref mut session) = *current_session {
            session.breaks.push(break_obj);
            session.total_break_time += break_seconds;
        }
    }

    *is_on_break = false;
    *state.break_time_elapsed.lock().unwrap() = 0;
    *state.is_paused.lock().unwrap() = false; // Resume main timer

    // update_menu(&app, &state); // Update menu to "Focus Active"

    Ok(())
}

#[tauri::command]
fn stop_timer(
    state: tauri::State<Arc<AppState>>,
    app: AppHandle,
    completed: bool,
) -> Result<(), String> {
    let mut current = state.current_session.lock().map_err(|e| e.to_string())?;

    if let Some(mut session) = current.take() {
        let remaining_deciseconds = *state.focus_time_remaining.lock().unwrap();
        let elapsed_deciseconds = (90 * 60 * 10) - remaining_deciseconds;
        let elapsed_seconds = elapsed_deciseconds / 10;

        session.duration = elapsed_seconds;
        session.completed = completed; // Use the flag passed from frontend

        state.add_session(session);
    }

    *state.focus_time_remaining.lock().unwrap() = 0;
    *state.break_time_elapsed.lock().unwrap() = 0;
    *state.is_on_break.lock().unwrap() = false;
    *state.current_break.lock().unwrap() = None;

    if let Some(window) = app.get_webview_window("focus") {
        let _ = window.close();
    }

    open_dashboard(&app);

    Ok(())
}

#[tauri::command]
fn get_timer_state(state: tauri::State<Arc<AppState>>) -> Result<TimerState, String> {
    let focus_deciseconds = *state
        .focus_time_remaining
        .lock()
        .map_err(|e| e.to_string())?;
    let break_deciseconds = *state.break_time_elapsed.lock().map_err(|e| e.to_string())?;
    let is_on_break = *state.is_on_break.lock().map_err(|e| e.to_string())?;
    let is_paused = *state.is_paused.lock().map_err(|e| e.to_string())?;

    Ok(TimerState {
        focus_seconds: focus_deciseconds / 10,
        break_seconds: break_deciseconds / 10,
        is_on_break,
        is_paused,
    })
}

#[tauri::command]
fn get_menu_stats(state: tauri::State<Arc<AppState>>) -> Result<MenuStats, String> {
    let (sessions, seconds, breaks_today) = state.get_today_stats();
    let streak = state.get_streak();

    let hours = seconds / 3600;
    let minutes = (seconds % 3600) / 60;
    let time_today = if hours > 0 {
        format!("{}h {}m", hours, minutes)
    } else {
        format!("{}m", minutes)
    };

    Ok(MenuStats {
        sessions_today: sessions,
        time_today,
        streak,
        breaks_today,
    })
}

#[tauri::command]
fn get_dashboard_data(state: tauri::State<Arc<AppState>>) -> Result<AppData, String> {
    let data = state.data.lock().map_err(|e| e.to_string())?;
    Ok(data.clone())
}

// Sleep/wake detection commands
#[derive(Serialize)]
struct WakeInfo {
    away_seconds: u32,
    should_show_dialog: bool,
}

#[tauri::command]
fn handle_system_sleep(state: tauri::State<Arc<AppState>>) {
    let is_paused = *state.is_paused.lock().unwrap();

    // Only track sleep if timer is actually running
    if !is_paused {
        *state.sleep_start_time.lock().unwrap() = Some(Utc::now());
        *state.was_running_before_sleep.lock().unwrap() = true;

        // Pause the timer
        *state.is_paused.lock().unwrap() = true;
    }
}

#[tauri::command]
fn handle_system_wake(state: tauri::State<Arc<AppState>>) -> Result<WakeInfo, String> {
    let sleep_start = state.sleep_start_time.lock().unwrap().take();
    let was_running = *state.was_running_before_sleep.lock().unwrap();

    if let Some(sleep_time) = sleep_start {
        if was_running {
            let away_seconds = (Utc::now() - sleep_time).num_seconds() as u32;

            // Show wake dialog if user was away for more than 30 seconds
            if away_seconds > 30 {
                return Ok(WakeInfo {
                    away_seconds,
                    should_show_dialog: true,
                });
            }
        }
    }

    // If we get here, just resume normally
    *state.is_paused.lock().unwrap() = false;
    *state.was_running_before_sleep.lock().unwrap() = false;

    Ok(WakeInfo {
        away_seconds: 0,
        should_show_dialog: false,
    })
}

#[tauri::command]
fn handle_wake_choice(
    state: tauri::State<Arc<AppState>>,
    choice: String,
    away_seconds: u32,
) -> Result<(), String> {
    match choice.as_str() {
        "break" => {
            // Count the away time as a break
            let mut break_time = state.break_time_elapsed.lock().unwrap();
            *break_time += away_seconds * 10; // Convert to deciseconds

            // Resume timer
            *state.is_paused.lock().unwrap() = false;
            *state.was_running_before_sleep.lock().unwrap() = false;
        }
        "working" => {
            // Just resume the timer (time was paused, so no loss)
            *state.is_paused.lock().unwrap() = false;
            *state.was_running_before_sleep.lock().unwrap() = false;
        }
        "stop" => {
            // User wants to end the session - handled by frontend calling stop_timer
            *state.was_running_before_sleep.lock().unwrap() = false;
        }
        _ => {}
    }

    Ok(())
}

// CLEAN TIMER LOOP - Updates every second
async fn run_timer(state: Arc<AppState>) {
    loop {
        tokio::time::sleep(Duration::from_millis(1000)).await;

        let is_on_break = *state.is_on_break.lock().unwrap();
        let is_paused = *state.is_paused.lock().unwrap();

        if is_paused {
            continue;
        }

        if is_on_break {
            // INCREMENT break timer
            let mut break_time = state.break_time_elapsed.lock().unwrap();
            *break_time += 10; // +10 deciseconds = 1 second
        } else {
            // DECREMENT focus timer
            let mut focus_time = state.focus_time_remaining.lock().unwrap();
            if *focus_time > 0 {
                *focus_time -= 10; // -10 deciseconds = 1 second
            }
        }
    }
}

async fn auto_save_loop(state: Arc<AppState>) {
    loop {
        tokio::time::sleep(Duration::from_secs(30)).await;
        state.save();
    }
}

// Modified build_menu to accept start text

fn open_dashboard(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("dashboard") {
        let _ = window.show();
        let _ = window.set_focus();
        return;
    }

    let _ = WebviewWindowBuilder::new(app, "dashboard", WebviewUrl::App("dashboard.html".into()))
        .title("Luma - Stats")
        .inner_size(500.0, 700.0)
        .resizable(true)
        .title_bar_style(TitleBarStyle::Overlay)
        .hidden_title(true)
        .build();
}

fn main() {
    println!("App starting...");

    tauri::Builder::default()
        .manage(Arc::new(AppState::new()))
        .invoke_handler(tauri::generate_handler![
            start_focus_session,
            set_session_topic,
            start_break,
            end_break,
            stop_timer,
            get_timer_state,
            get_menu_stats,
            get_dashboard_data,
            handle_system_sleep,
            handle_system_wake,
            handle_wake_choice,
        ])
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                // HIDE instead of CLOSE for both windows
                // This keeps the app running in the tray and preserves state
                window.hide().unwrap();
                api.prevent_close();
            }
        })
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec![]),
        ))
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            let _ = app.get_webview_window("focus").map(|w| {
                let _ = w.show();
                let _ = w.set_focus();
            });
            let _ = app.get_webview_window("dashboard").map(|w| {
                let _ = w.show();
                let _ = w.set_focus();
            });
        }))
        .setup(|app| {
            println!("Setup started");
            let state = app.state::<Arc<AppState>>();
            let (_sessions, seconds, _breaks_today) = state.get_today_stats();
            let _streak = state.get_streak();

            let hours = seconds / 3600;
            let minutes = (seconds % 3600) / 60;
            let _time_today = if hours > 0 {
                format!("{}h {}m", hours, minutes)
            } else {
                format!("{}m", minutes)
            };

            // SIMPLIFIED STABLE TRAY MENU
            let quit_i = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let start_i = MenuItem::with_id(app, "start", "Start Focus", true, None::<&str>)?;
            let dashboard_i = MenuItem::with_id(app, "dashboard", "Dashboard", true, None::<&str>)?;
            let separator = PredefinedMenuItem::separator(app)?;

            let items: Vec<&dyn IsMenuItem<tauri::Wry>> =
                vec![&start_i, &dashboard_i, &separator, &quit_i];
            let menu = Menu::with_items(app, &items)?;

            let icon_data = include_bytes!("../icons/tray-32x32.png");
            let icon_img = image::load_from_memory(icon_data)
                .expect("Failed to load tray icon")
                .to_rgba8();
            let (width, height) = icon_img.dimensions();
            let rgba = icon_img.into_raw();
            let icon = tauri::image::Image::new_owned(rgba, width, height);

            TrayIconBuilder::with_id("main")
                .icon(icon)
                .menu(&menu)
                .on_menu_event(move |app, event| match event.id().as_ref() {
                    "start" => {
                        let _ = start_focus_session(app.state(), app.clone());
                    }
                    "dashboard" => {
                        open_dashboard(app);
                    }
                    "quit" => {
                        app.exit(0);
                    }
                    _ => {}
                })
                .build(app)?;

            println!("Tray built successfully");

            // Auto-launch the focus session so the user sees the window immediately
            match start_focus_session(app.state(), app.handle().clone()) {
                Ok(_) => println!("Auto-launch successful"),
                Err(e) => println!("Auto-launch failed: {}", e),
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running luma");
}
