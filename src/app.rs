//! The eframe glue: runs the core's commands, feeds results and input back as
//! events, and keeps the window state. The first decode starts before the
//! window exists, so decoding and GL context creation overlap.

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::Instant;

use crate::clipboard::Clipboard;
use crate::core::{Command, Core, Event, Screen};
use crate::decode::DecodedImage;
use crate::geometry::{self, Geometry};
use crate::rotate::Rotation;
use crate::{decode, external, folder, rotate, settings, startup, trash, ui, watch};

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
    let mut worker = Worker {
        ctx: ctx.clone(),
        events,
        decodes: HashMap::new(),
        watcher: None,
        search: None,
        frames: None,
        inner: None,
        settings_path: settings::default_path(),
        copier: None,
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
        Screen::Listing { dir }
        | Screen::NoImages { dir, .. }
        | Screen::Searching { dir }
        | Screen::NothingBelow { dir } => Some(dir),
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
    /// Cancellation tokens of the decodes still running.
    decodes: HashMap<PathBuf, Arc<AtomicBool>>,
    /// Watches the current folder while it is kept.
    watcher: Option<notify::RecommendedWatcher>,
    /// Cancellation token of the running search.
    search: Option<Arc<AtomicBool>>,
    /// Cancellation token of the running frames decode.
    frames: Option<Arc<AtomicBool>>,
    /// Cancellation token of the running decode of an inner image.
    inner: Option<Arc<AtomicBool>>,
    /// Where the settings file lives; absent without a config directory.
    settings_path: Option<PathBuf>,
    /// Hands copies to the clipboard thread, started on the first copy.
    copier: Option<Sender<CopyJob>>,
}

/// One `c`: the file and its pixels as shown.
type CopyJob = (PathBuf, DecodedImage, Rotation);

/// Why there is no settings file to read or write.
const NO_CONFIG_DIR: &str = "No configuration directory found";

impl Worker {
    /// Starts the background part of `command`; commands for the window itself
    /// are handled by the app.
    fn spawn(&mut self, command: &Command) {
        match command {
            Command::Decode(path) => {
                let cancel = Arc::new(AtomicBool::new(false));
                self.decodes.insert(path.clone(), Arc::clone(&cancel));
                let path = path.clone();
                self.run_all(move || {
                    let result = decode::decode_first_frame(&path, &cancel)?;
                    let animated = matches!(&result, Ok(first) if first.animated);
                    let images = result.as_ref().map_or(1, |first| first.images);
                    let result = result.map(|first| first.image);
                    let mut events = vec![Event::Decoded {
                        path: path.clone(),
                        result,
                    }];
                    if animated {
                        events.push(Event::Animated(path.clone()));
                    }
                    if images > 1 {
                        events.push(Event::Images {
                            path,
                            count: images,
                        });
                    }
                    Some(events)
                });
            }
            Command::DecodeFrames(path) => {
                let cancel = Arc::new(AtomicBool::new(false));
                if let Some(old) = self.frames.replace(Arc::clone(&cancel)) {
                    old.store(true, Ordering::Relaxed);
                }
                let path = path.clone();
                self.run(move || {
                    let result = decode::decode_frames(&path, &cancel)?;
                    Some(Event::FramesDecoded { path, result })
                });
            }
            Command::CancelFrames => {
                if let Some(cancel) = self.frames.take() {
                    cancel.store(true, Ordering::Relaxed);
                }
            }
            Command::DecodeInner { path, index } => {
                let cancel = Arc::new(AtomicBool::new(false));
                if let Some(old) = self.inner.replace(Arc::clone(&cancel)) {
                    old.store(true, Ordering::Relaxed);
                }
                let (path, index) = (path.clone(), *index);
                self.run(move || {
                    let result = decode::decode_inner(&path, index, &cancel)?;
                    Some(Event::InnerDecoded {
                        path,
                        index,
                        result,
                    })
                });
            }
            Command::CancelInner => {
                if let Some(cancel) = self.inner.take() {
                    cancel.store(true, Ordering::Relaxed);
                }
            }
            Command::CancelDecode(path) => {
                if let Some(cancel) = self.decodes.remove(path) {
                    cancel.store(true, Ordering::Relaxed);
                }
            }
            Command::ListFolder(dir) => {
                let dir = dir.clone();
                self.run(move || {
                    let result = folder::list_media(&dir, true).map_err(|e| e.to_string());
                    Some(Event::FolderListed { dir, result })
                });
            }
            Command::ListDirs(dir) => {
                let dir = dir.clone();
                self.run(move || {
                    let result = folder::list_dirs(&dir, true).map_err(|e| e.to_string());
                    Some(Event::DirsListed { dir, result })
                });
            }
            Command::Search { dir, dots } => {
                let cancel = Arc::new(AtomicBool::new(false));
                if let Some(old) = self.search.replace(Arc::clone(&cancel)) {
                    old.store(true, Ordering::Relaxed);
                }
                let (dir, dots) = (dir.clone(), *dots);
                self.run(move || {
                    let found = folder::search_media(&dir, dots, &cancel)?;
                    Some(Event::Searched { dir, dots, found })
                });
            }
            Command::CancelSearch => {
                if let Some(cancel) = self.search.take() {
                    cancel.store(true, Ordering::Relaxed);
                }
            }
            Command::WatchFolder(dir) => {
                // Dropping the old watcher stops it.
                self.watcher = None;
                let events = self.events.clone();
                let ctx = self.ctx.clone();
                let watched = dir.clone();
                let watcher = watch::watch(dir, move |result| {
                    let dir = watched.clone();
                    // The app may already be gone; nothing to report to then.
                    let _ = events.send(Event::FolderListed { dir, result });
                    ctx.request_repaint();
                });
                match watcher {
                    Ok(watcher) => self.watcher = Some(watcher),
                    // Browsing works without it, only changes go unnoticed.
                    Err(e) => eprintln!("pixpixpix: cannot watch {}: {e}", dir.display()),
                }
            }
            Command::LoadSettings => {
                let path = self.settings_path.clone();
                self.run(move || {
                    let result = match path {
                        Some(path) => settings::load(&path),
                        None => Err(NO_CONFIG_DIR.to_owned()),
                    };
                    Some(Event::SettingsLoaded(result))
                });
            }
            // Written right away, so writes cannot overtake each other; the
            // file is a few bytes.
            Command::SaveSettings(values) => {
                let result = match &self.settings_path {
                    Some(path) => settings::save(path, values),
                    None => Err(NO_CONFIG_DIR.to_owned()),
                };
                // The app may already be gone; nothing to report to then.
                let _ = self.events.send(Event::SettingsSaved(result));
                self.ctx.request_repaint();
            }
            Command::OpenExternally { path, app } => {
                let events = self.events.clone();
                let ctx = self.ctx.clone();
                let exited = move |result: Result<(), String>| {
                    if let Err(message) = result {
                        // The app may already be gone; nothing to report to then.
                        let _ = events.send(Event::OpenFailed(message));
                        ctx.request_repaint();
                    }
                };
                if let Err(message) =
                    external::launch(external::command(path, app.as_deref()), exited)
                {
                    // The app may already be gone; nothing to report to then.
                    let _ = self.events.send(Event::OpenFailed(message));
                    self.ctx.request_repaint();
                }
            }
            Command::WriteRotation { path, rotation } => {
                let (path, rotation) = (path.clone(), *rotation);
                self.run(move || {
                    let result = rotate::write(&path, rotation);
                    Some(Event::RotationWritten { path, result })
                });
            }
            Command::Trash(path) => {
                let path = path.clone();
                self.run(move || {
                    let result = trash::move_to_trash(&path);
                    Some(Event::Trashed { path, result })
                });
            }
            Command::Copy {
                path,
                image,
                rotation,
            } => {
                let job = (path.clone(), image.clone(), *rotation);
                if self.copier().send(job).is_err() {
                    // The app may already be gone; nothing to report to then.
                    let _ = self.events.send(Event::Copied(Err(
                        "Cannot copy: the clipboard stopped".to_owned(),
                    )));
                }
            }
            Command::SetFullscreen(_) | Command::Quit => {}
        }
    }

    /// The clipboard thread, started on the first copy. One thread copies in
    /// the order of the keys and keeps the clipboard served while the app
    /// runs.
    fn copier(&mut self) -> &Sender<CopyJob> {
        self.copier.get_or_insert_with(|| {
            let (jobs, inbox) = channel::<CopyJob>();
            let events = self.events.clone();
            let ctx = self.ctx.clone();
            std::thread::spawn(move || {
                let mut clipboard = Clipboard::default();
                for (path, image, rotation) in inbox {
                    let result = clipboard.copy(&path, &image, rotation);
                    // The app may already be gone; nothing to report to then.
                    let _ = events.send(Event::Copied(result));
                    ctx.request_repaint();
                }
            });
            jobs
        })
    }

    /// Forgets the token of a decode that reported back.
    fn finished(&mut self, event: &Event) {
        if let Event::Decoded { path, .. } = event {
            self.decodes.remove(path);
        }
    }

    /// Runs `job` on its own thread and sends its event back, if it has one.
    fn run(&self, job: impl FnOnce() -> Option<Event> + Send + 'static) {
        self.run_all(move || job().map(|event| vec![event]));
    }

    /// Runs `job` on its own thread and sends its events back in order, if
    /// it has any.
    fn run_all(&self, job: impl FnOnce() -> Option<Vec<Event>> + Send + 'static) {
        let events = self.events.clone();
        let ctx = self.ctx.clone();
        std::thread::spawn(move || {
            let Some(sent) = job() else {
                return;
            };
            for event in sent {
                // The app may already be gone; nothing to report to then.
                let _ = events.send(event);
            }
            ctx.request_repaint();
        });
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
        self.worker.finished(&event);
        for command in self.core.handle(event) {
            match command {
                Command::Quit => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
                Command::SetFullscreen(on) => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(on));
                }
                Command::Decode(_)
                | Command::CancelDecode(_)
                | Command::ListFolder(_)
                | Command::WatchFolder(_)
                | Command::ListDirs(_)
                | Command::Search { .. }
                | Command::CancelSearch
                | Command::LoadSettings
                | Command::SaveSettings(_)
                | Command::OpenExternally { .. }
                | Command::WriteRotation { .. }
                | Command::Trash(_)
                | Command::Copy { .. }
                | Command::DecodeFrames(_)
                | Command::CancelFrames
                | Command::DecodeInner { .. }
                | Command::CancelInner => {
                    self.worker.spawn(&command);
                }
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
        // Not from `ui::show`: its events ask for another frame at once.
        if self.core.view().next_frame_in.is_some() {
            self.dispatch(&ctx, Event::Tick(Instant::now()));
        }
        for event in ui::input_events(&ctx) {
            self.dispatch(&ctx, event);
        }
        let clicks = ui::show(ui, self.core.view(), &mut self.textures);
        if !clicks.is_empty() {
            ctx.request_repaint();
        }
        for event in clicks {
            self.dispatch(&ctx, event);
        }
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
