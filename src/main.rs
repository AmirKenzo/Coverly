#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod config;
mod folder;
mod icon;
mod name;
mod net;
mod nfo;
mod search_cache;
mod sources;
mod templates;
mod ui;
mod update;
mod videos;
mod worker;

use eframe::egui;
use std::{io::Write, path::PathBuf};

fn main() -> Result<(), eframe::Error> {
    install_crash_log();
    update::cleanup();

    let start = std::env::args_os().nth(1).map(PathBuf::from).filter(|p| p.is_dir());

    let icon = templates::app_icon(64);
    let viewport = egui::ViewportBuilder::default()
        .with_title(config::APP_NAME)
        .with_inner_size([1400.0, 880.0])
        .with_min_inner_size([900.0, 600.0])
        .with_decorations(false)
        .with_drag_and_drop(true)
        .with_icon(egui::IconData {
            width: icon.width(),
            height: icon.height(),
            rgba: icon.into_raw(),
        });

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        config::APP_NAME,
        options,
        Box::new(move |cc| Ok(Box::new(app::App::new(cc, start)))),
    )
}

fn install_crash_log() {
    std::panic::set_hook(Box::new(|info| {
        let dir = config::Config::dir();
        let _ = std::fs::create_dir_all(&dir);
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join("crash.log"))
        {
            let when = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let _ = writeln!(f, "[{when}] {info}");
            let _ = writeln!(f, "{}", std::backtrace::Backtrace::force_capture());
        }
    }));

    let _ = rayon::ThreadPoolBuilder::new().panic_handler(|_| {}).build_global();
}
