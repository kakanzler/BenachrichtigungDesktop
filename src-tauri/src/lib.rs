mod activate;
mod autostart;
mod config;
mod counts;
mod model;
mod placement;

use chrono::Local;
use config::Paths;
use counts::{english, forscher, Reading, BUSY_TIMEOUT};
use model::{AppId, Model, View};
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::menu::{CheckMenuItem, Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, WindowEvent};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};

const POLL_INTERVAL: Duration = Duration::from_secs(30);
const MAX_LOG_BYTES: u64 = 1024 * 1024;
const TRAY_ID: &str = "main";

struct Shared {
    model: Mutex<Model>,
    paths: Paths,
    last_launch: Mutex<HashMap<AppId, Instant>>,
    zukunft_last_launch: Mutex<Option<Instant>>,
    refresh: Sender<()>,
    log_file: PathBuf,
}

impl Shared {
    fn log(&self, message: &str) {
        let line = format!("{} {message}\n", Local::now().format("%Y-%m-%d %H:%M:%S"));
        if let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log_file)
        {
            let _ = file.write_all(line.as_bytes());
        }
    }

    fn view(&self) -> View {
        self.model.lock().unwrap().view()
    }

    fn exe(&self, app: AppId) -> &PathBuf {
        match app {
            AppId::English => &self.paths.english_exe,
            AppId::Forscher => &self.paths.forscher_exe,
        }
    }

    fn read(&self, app: AppId) -> Reading {
        let today = Local::now().date_naive();
        match app {
            AppId::English => counts::read(
                &self.paths.english_db,
                today,
                BUSY_TIMEOUT,
                english::count_due,
            ),
            AppId::Forscher => counts::read(
                &self.paths.forscher_db,
                today,
                BUSY_TIMEOUT,
                forscher::count_due,
            ),
        }
    }
}

/// Pushes the current view to the card, the window and the tray tooltip.
fn sync(app: &AppHandle) {
    let shared = app.state::<Shared>();
    let view = shared.view();
    let _ = app.emit("view-updated", &view);
    if let Some(window) = app.get_webview_window("main") {
        let shown = if view.visible {
            placement::show(&window, view.rows.len())
        } else {
            placement::hide(&window).map_err(|e| e.to_string())
        };
        if let Err(e) = shown {
            shared.log(&format!("window: {e}"));
        }
    }
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let tooltip = if view.rows.is_empty() {
            "今日の復習: なし".to_string()
        } else {
            let parts: Vec<String> = view
                .rows
                .iter()
                .map(|r| format!("{} {}", r.label, r.count))
                .collect();
            format!("今日の復習: {}", parts.join(" / "))
        };
        let _ = tray.set_tooltip(Some(tooltip));
    }
}

fn reveal(app: &AppHandle) {
    app.state::<Shared>().model.lock().unwrap().reveal();
    sync(app);
}

fn spawn_poller(app: AppHandle, wake: mpsc::Receiver<()>) {
    std::thread::spawn(move || {
        // Logged only when it changes, so a lasting failure leaves one line
        // rather than one every 30 seconds.
        let mut last_failures: HashMap<AppId, String> = HashMap::new();
        loop {
            let shared = app.state::<Shared>();
            for id in AppId::ALL {
                let reading = shared.read(id);
                if let Reading::Failed(reason) = &reading {
                    if last_failures.get(&id) != Some(reason) {
                        shared.log(&format!("{}: {reason}", id.label()));
                        last_failures.insert(id, reason.clone());
                    }
                } else if !matches!(reading, Reading::Busy) {
                    last_failures.remove(&id);
                }
                shared.model.lock().unwrap().apply(id, &reading);
            }
            sync(&app);
            if let Err(RecvTimeoutError::Disconnected) = wake.recv_timeout(POLL_INTERVAL) {
                return;
            }
        }
    });
}

#[tauri::command]
fn get_view(shared: tauri::State<'_, Shared>) -> View {
    shared.view()
}

#[tauri::command]
fn dismiss(app: AppHandle) {
    app.state::<Shared>().model.lock().unwrap().dismiss();
    sync(&app);
}

#[tauri::command(async)]
fn open_app(app: AppHandle, target: AppId) {
    let shared = app.state::<Shared>();
    let exe = shared.exe(target);
    let last = shared.last_launch.lock().unwrap().get(&target).copied();
    let note = match activate::open(exe, last) {
        Ok(launched) => {
            if launched {
                shared
                    .last_launch
                    .lock()
                    .unwrap()
                    .insert(target, Instant::now());
            }
            None
        }
        Err(activate::OpenError::NotFound) => {
            shared.log(&format!("{}: not found: {}", target.label(), exe.display()));
            Some("未検出")
        }
        Err(activate::OpenError::Failed(reason)) => {
            shared.log(&format!("{}: launch failed: {reason}", target.label()));
            Some("起動失敗")
        }
    };
    shared
        .model
        .lock()
        .unwrap()
        .set_note(target, note.map(String::from));
    sync(&app);
}

/// Zukunft has no counts, so it is not a row: it opens from the header button
/// and from the tray, which still works while the card is hidden.
/// Returns the failure text for the button's tooltip.
#[tauri::command(async)]
fn open_zukunft(shared: tauri::State<'_, Shared>) -> Result<(), String> {
    launch_zukunft(&shared)
}

fn launch_zukunft(shared: &Shared) -> Result<(), String> {
    let exe = &shared.paths.zukunft_exe;
    let last = *shared.zukunft_last_launch.lock().unwrap();
    match activate::open(exe, last) {
        Ok(launched) => {
            if launched {
                *shared.zukunft_last_launch.lock().unwrap() = Some(Instant::now());
            }
            Ok(())
        }
        Err(activate::OpenError::NotFound) => {
            shared.log(&format!("Zukunft: not found: {}", exe.display()));
            Err("Zukunft: 未検出".into())
        }
        Err(activate::OpenError::Failed(reason)) => {
            shared.log(&format!("Zukunft: launch failed: {reason}"));
            Err("Zukunft: 起動失敗".into())
        }
    }
}

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "再表示", true, None::<&str>)?;
    let refresh = MenuItem::with_id(app, "refresh", "今すぐ更新", true, None::<&str>)?;
    let autostart = CheckMenuItem::with_id(
        app,
        "autostart",
        "PC 起動時に自動で開始",
        !cfg!(debug_assertions),
        app.autolaunch().is_enabled().unwrap_or(false),
        None::<&str>,
    )?;
    let zukunft = MenuItem::with_id(app, "zukunft", "Zukunft を開く", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "終了", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &refresh, &zukunft, &autostart, &quit])?;

    let mut tray = TrayIconBuilder::with_id(TRAY_ID)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(move |app, event| match event.id.as_ref() {
            "show" => reveal(app),
            "refresh" => {
                let _ = app.state::<Shared>().refresh.send(());
            }
            "autostart" => {
                let want = autostart.is_checked().unwrap_or(false);
                if let Err(e) = autostart::set_enabled(app, want) {
                    app.state::<Shared>().log(&format!("autostart: {e}"));
                    let _ = autostart.set_checked(!want);
                }
            }
            "zukunft" => {
                // Off the event loop, like the async command the button uses.
                let app = app.clone();
                std::thread::spawn(move || {
                    let _ = launch_zukunft(&app.state::<Shared>());
                });
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                reveal(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}

pub fn run() {
    tauri::Builder::default()
        // Must come first: a second launch only reveals the running card.
        .plugin(tauri_plugin_single_instance::init(|app, _, _| reveal(app)))
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            None,
        ))
        .invoke_handler(tauri::generate_handler![
            get_view,
            dismiss,
            open_app,
            open_zukunft
        ])
        .on_window_event(|window, event| {
            // Alt+F4 hides the card like ×; only the tray's 終了 ends the app.
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                dismiss(window.app_handle().clone());
            }
        })
        .setup(|app| {
            let handle = app.handle().clone();
            let data_dir = app.path().app_data_dir()?;
            fs::create_dir_all(&data_dir)?;

            let log_file = data_dir.join("app.log");
            if fs::metadata(&log_file).is_ok_and(|m| m.len() > MAX_LOG_BYTES) {
                let _ = fs::remove_file(&log_file);
            }

            let roaming = app.path().config_dir()?;
            // A debug-only override lets the app be exercised against fixtures
            // without opening the real apps, whose startup settles real data.
            let config_file = std::env::var_os("BENACHRICHTIGUNG_CONFIG")
                .filter(|_| cfg!(debug_assertions))
                .map_or_else(|| data_dir.join("config.json"), PathBuf::from);
            let (paths, problem) = config::load(&config_file, &roaming);

            let (refresh, wake) = mpsc::channel();
            app.manage(Shared {
                model: Mutex::new(Model::default()),
                paths,
                last_launch: Mutex::new(HashMap::new()),
                zukunft_last_launch: Mutex::new(None),
                refresh,
                log_file,
            });
            if let Some(problem) = problem {
                handle.state::<Shared>().log(&format!("config: {problem}"));
            }

            if let Err(e) = autostart::init(&handle, &data_dir) {
                handle.state::<Shared>().log(&format!("autostart: {e}"));
            }
            build_tray(&handle)?;
            spawn_poller(handle, wake);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running the application");
}
