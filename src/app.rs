//! The eframe glue: runs the core's commands, feeds results and input back as
//! events, and keeps the window state. The first decode starts before the
//! window exists, so decoding and GL context creation overlap.

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::Instant;

use crate::core::{Command, Core, Event, Screen};
use crate::geometry::{self, Geometry};
use crate::{decode, folder, startup, ui};

pub const APP_ID: &str = "io.github.paxel.pixpixpix";

/// Set to print the time from launch to the first frame showing an image.
const TIMING_ENV: &str = "PIXPIXPIX_TIMING";

/// Starts the viewer for the first command-line argument.
pub fn run() -> ExitCode {
    let launched = Instant::now();
    let arg = std::env::args_os().nth(1).map(PathBuf::from);
    let cwd = std::env::current_dir().unwrap_or_default();
    let start = startup::resolve(arg.as_deref(), &cwd);

    let ctx = egui::Context::default();
    let (events, inbox) = channel();
    let (core, commands) = Core::start(start);
    let worker = Worker {
        ctx: ctx.clone(),
        events,
    };
    for command in &commands {
        worker.spawn(command);
    }

    let geometry_path = geometry::default_path();
    let saved = geometry_path.as_deref().and_then(geometry::load);
    let mut viewport = egui::ViewportBuilder::default()
        .with_app_id(APP_ID)
        .with_title(title(core.view()))
        .with_inner_size(saved.map_or([1200.0, 800.0], |g| [g.width, g.height]));
    if let Some(Geometry {
        x: Some(x),
        y: Some(y),
        ..
    }) = saved
    {
        viewport = viewport.with_position([x, y]);
    }
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    let app = App {
        core,
        worker,
        inbox,
        textures: ui::Textures::default(),
        launched,
        first_frame_drawn: false,
        geometry: saved,
        geometry_path,
    };
    let result = eframe::run_native_ext(
        "pixpixpix",
        options,
        Some(ctx),
        Box::new(|_cc| Ok(Box::new(app))),
    );
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("pixpixpix: {e}");
            ExitCode::FAILURE
        }
    }
}

fn title(view: &crate::core::View) -> String {
    let path = match &view.screen {
        Screen::Loading { path } | Screen::Image { path, .. } => Some(path),
        Screen::Error { path, .. } => path.as_ref(),
    };
    match path {
        Some(p) => format!("{} — pixpixpix", folder::file_name(p)),
        None => "pixpixpix".to_owned(),
    }
}

/// Runs background work for commands and reports back as events.
struct Worker {
    ctx: egui::Context,
    events: Sender<Event>,
}

impl Worker {
    /// Starts the background part of `command`; commands for the window itself
    /// are handled by the app.
    fn spawn(&self, command: &Command) {
        if let Command::Decode(path) = command {
            let path = path.clone();
            let events = self.events.clone();
            let ctx = self.ctx.clone();
            std::thread::spawn(move || {
                let result = decode::decode_file(&path);
                // The app may already be gone; nothing to report to then.
                let _ = events.send(Event::Decoded { path, result });
                ctx.request_repaint();
            });
        }
    }
}

struct App {
    core: Core,
    worker: Worker,
    inbox: Receiver<Event>,
    textures: ui::Textures,
    launched: Instant,
    first_frame_drawn: bool,
    geometry: Option<Geometry>,
    geometry_path: Option<PathBuf>,
}

impl App {
    fn dispatch(&mut self, ctx: &egui::Context, event: Event) {
        let title_before = title(self.core.view());
        for command in self.core.handle(event) {
            match command {
                Command::Quit => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
                Command::SetFullscreen(on) => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(on));
                }
                Command::Decode(_) => self.worker.spawn(&command),
            }
        }
        let title_after = title(self.core.view());
        if title_after != title_before {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title_after));
        }
    }

    fn remember_geometry(&mut self, ctx: &egui::Context) {
        if self.core.view().fullscreen {
            return;
        }
        let (inner, outer) = ctx.input(|i| (i.viewport().inner_rect, i.viewport().outer_rect));
        let size = inner.map_or_else(|| ctx.content_rect().size(), |r| r.size());
        let position = outer.map(|r| r.min);
        self.geometry = Some(Geometry {
            width: size.x,
            height: size.y,
            x: position.map(|p| p.x),
            y: position.map(|p| p.y),
        });
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if self.first_frame_drawn && !self.core.first_frame_shown() {
            if std::env::var_os(TIMING_ENV).is_some() {
                eprintln!(
                    "pixpixpix: first frame after {} ms",
                    self.launched.elapsed().as_millis()
                );
            }
            self.dispatch(&ctx, Event::FirstFrameShown);
        }
        while let Ok(event) = self.inbox.try_recv() {
            self.dispatch(&ctx, event);
        }
        for event in ui::input_events(&ctx) {
            self.dispatch(&ctx, event);
        }
        ui::show(ui, self.core.view(), &mut self.textures);
        if !self.first_frame_drawn && !matches!(self.core.view().screen, Screen::Loading { .. }) {
            self.first_frame_drawn = true;
            ctx.request_repaint();
        }
        self.remember_geometry(&ctx);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if let (Some(path), Some(geometry)) = (&self.geometry_path, &self.geometry) {
            // Losing the window size is not worth an error at exit.
            let _ = geometry::save(path, geometry);
        }
    }
}
