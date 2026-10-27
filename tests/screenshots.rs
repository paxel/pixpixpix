//! The screenshots of the documentation site: the app drawn headless with
//! generated sample pictures, its triangles rasterized on the CPU, so the
//! pictures come out the same on every machine and need no GPU.
//!
//! `cargo test --test screenshots -- --ignored` writes them into
//! `site/assets/screenshots/`.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use egui::epaint::{ClippedPrimitive, ColorImage, ImageData, Primitive};
use egui::{Color32, Pos2, Rect, TextureId, Vec2};
use pixpixpix::core::{BinItem, Command, Core, DefaultRow, Defaults, Event, Key, KeyPress};
use pixpixpix::decode::DecodedImage;
use pixpixpix::settings::Settings;
use pixpixpix::ui;

/// The window the screenshots show, in points; one point is one pixel.
const WINDOW: Vec2 = Vec2::new(1200.0, 760.0);

const ROOT: &str = "/home/you/Pictures/Holiday 2026";

/// The sample folders and their files.
fn folders() -> BTreeMap<PathBuf, Vec<PathBuf>> {
    let folder = |name: &str, count: usize| {
        let dir = Path::new(ROOT).join(name);
        let files = (1..=count)
            .map(|i| dir.join(format!("IMG_{:04}.jpg", 4100 + i)))
            .collect();
        (dir, files)
    };
    let mut folders = BTreeMap::from([folder("Coast", 9), folder("Mountains", 7)]);
    let root = PathBuf::from(ROOT);
    let files = (1..=14)
        .map(|i| root.join(format!("IMG_{:04}.jpg", 4000 + i)))
        .collect();
    folders.insert(root, files);
    folders
}

/// A small landscape for `path`: a sky, a sun and hills in colours of its own.
fn picture(path: &Path, width: u32, height: u32) -> DecodedImage {
    let seed = path
        .to_string_lossy()
        .bytes()
        .fold(7u32, |h, b| h.wrapping_mul(31).wrapping_add(u32::from(b)));
    let hue = (seed.wrapping_mul(2_654_435_761) >> 7) as f32 % 360.0;
    let sky_top = hsv(hue, 0.55, 0.55);
    let sky_low = hsv(hue + 40.0, 0.35, 0.95);
    let sun = hsv(hue + 70.0, 0.25, 1.0);
    let far = hsv(hue + 160.0, 0.35, 0.55);
    let near = hsv(hue + 190.0, 0.45, 0.32);
    let phase = (seed % 100) as f32 / 16.0;
    let (w, h) = (width as f32, height as f32);
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            let (u, v) = (x as f32 / w, y as f32 / h);
            let far_line = 0.55 + 0.08 * (u * 5.0 + phase).sin();
            let near_line = 0.72 + 0.06 * (u * 9.0 - phase).cos();
            let (sx, sy) = (u - 0.7, (v - 0.3) * h / w);
            let color = if v > near_line {
                near
            } else if v > far_line {
                far
            } else if (sx * sx + sy * sy).sqrt() < 0.07 {
                sun
            } else {
                mix(sky_top, sky_low, v / far_line)
            };
            rgba.extend_from_slice(&[color[0], color[1], color[2], 255]);
        }
    }
    DecodedImage {
        width,
        height,
        rgba: Arc::from(rgba),
    }
}

fn hsv(hue: f32, s: f32, v: f32) -> [u8; 3] {
    let h = (hue % 360.0) / 60.0;
    let c = v * s;
    let x = c * (1.0 - (h % 2.0 - 1.0).abs());
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    [r, g, b].map(|ch| ((ch + m) * 255.0).round() as u8)
}

fn mix(a: [u8; 3], b: [u8; 3], t: f32) -> [u8; 3] {
    let t = t.clamp(0.0, 1.0);
    [0, 1, 2].map(|i| (f32::from(a[i]) * (1.0 - t) + f32::from(b[i]) * t).round() as u8)
}

/// The app with its answers faked: files, thumbnails, the trash and the
/// default apps come from the sample folders.
struct Shot {
    ctx: egui::Context,
    core: Core,
    textures: ui::Textures,
    gpu: HashMap<TextureId, ColorImage>,
    primitives: Vec<ClippedPrimitive>,
    time: f64,
}

impl Shot {
    fn start(path: &Path) -> Self {
        let (core, commands) = Core::start(Ok(path.to_path_buf()));
        let mut shot = Shot {
            ctx: egui::Context::default(),
            core,
            textures: ui::Textures::default(),
            gpu: HashMap::new(),
            primitives: Vec::new(),
            time: 0.0,
        };
        for command in commands {
            shot.answer(command);
        }
        shot.frames(3);
        shot.handle(Event::FirstFrameShown);
        shot.frames(12);
        shot
    }

    fn handle(&mut self, event: Event) {
        for command in self.core.handle(event) {
            self.answer(command);
        }
    }

    fn press(&mut self, key: Key) {
        self.handle(Event::Key(KeyPress::plain(key)));
        self.frames(12);
    }

    fn answer(&mut self, command: Command) {
        let folders = folders();
        match command {
            Command::Decode(path) => {
                let image = picture(&path, 1500, 1000);
                self.handle(Event::Decoded {
                    path,
                    result: Ok(image),
                });
            }
            Command::ListFolder(dir) => {
                let files = folders.get(&dir).cloned().unwrap_or_default();
                self.handle(Event::FolderListed {
                    dir,
                    result: Ok(files),
                });
            }
            Command::ListDirs(dir) => {
                let dirs = folders
                    .keys()
                    .filter(|d| d.parent() == Some(dir.as_path()))
                    .cloned()
                    .collect();
                self.handle(Event::DirsListed {
                    dir,
                    result: Ok(dirs),
                });
            }
            Command::LoadSettings => self.handle(Event::SettingsLoaded(Ok(Settings::default()))),
            Command::ScanOverview { scan, root, .. } => {
                for (dir, files) in folders.iter().filter(|(d, _)| d.starts_with(&root)) {
                    self.handle(Event::OverviewSection {
                        scan,
                        dir: dir.clone(),
                        files: files.clone(),
                    });
                }
                self.handle(Event::OverviewScanned(scan));
            }
            Command::DecodeThumbnail { path, size } => {
                let image = picture(&path, size, size * 2 / 3);
                self.handle(Event::ThumbnailDecoded {
                    path,
                    size,
                    result: Ok(image),
                });
            }
            Command::ListBin => self.handle(Event::BinListed(Ok(bin()))),
            Command::ReadDefaults { .. } => self.handle(Event::DefaultsRead(defaults())),
            _ => {}
        }
    }

    /// Runs `n` frames of the app at 60 frames a second.
    fn frames(&mut self, n: usize) {
        for _ in 0..n {
            self.time += 1.0 / 60.0;
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, WINDOW)),
                time: Some(self.time),
                ..Default::default()
            };
            let mut events = Vec::new();
            let output = self.ctx.run_ui(input, |ui| {
                events = ui::show(ui, self.core.view(), &mut self.textures);
            });
            for (id, delta) in output.textures_delta.set {
                let ImageData::Color(image) = delta.image;
                match (delta.pos, self.gpu.get_mut(&id)) {
                    (Some([x0, y0]), Some(texture)) => {
                        for y in 0..image.size[1] {
                            for x in 0..image.size[0] {
                                let to = (y0 + y) * texture.size[0] + x0 + x;
                                texture.pixels[to] = image.pixels[y * image.size[0] + x];
                            }
                        }
                    }
                    _ => {
                        self.gpu.insert(id, (*image).clone());
                    }
                }
            }
            for id in output.textures_delta.free {
                self.gpu.remove(&id);
            }
            self.primitives = self.ctx.tessellate(output.shapes, output.pixels_per_point);
            for event in events {
                self.handle(event);
            }
        }
    }

    /// The last frame, rasterized.
    fn render(&self) -> image::RgbaImage {
        let (width, height) = (WINDOW.x as u32, WINDOW.y as u32);
        let mut canvas = vec![[0.0f32; 4]; (width * height) as usize];
        for primitive in &self.primitives {
            let Primitive::Mesh(mesh) = &primitive.primitive else {
                continue;
            };
            let Some(texture) = self.gpu.get(&mesh.texture_id) else {
                continue;
            };
            let clip = primitive
                .clip_rect
                .intersect(Rect::from_min_size(Pos2::ZERO, WINDOW));
            for triangle in mesh.indices.as_chunks::<3>().0 {
                let [a, b, c] = triangle.map(|i| mesh.vertices[i as usize]);
                fill_triangle(&mut canvas, width, clip, texture, [a, b, c]);
            }
        }
        let byte = |c: f32| (c * 255.0).round().clamp(0.0, 255.0) as u8;
        image::RgbaImage::from_fn(width, height, |x, y| {
            let [r, g, b, _] = canvas[(y * width + x) as usize];
            image::Rgba([byte(r), byte(g), byte(b), 255])
        })
    }
}

/// Twice the signed area of the triangle `a`, `b`, `p`.
fn edge(a: Pos2, b: Pos2, p: Pos2) -> f32 {
    (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x)
}

/// Draws one triangle, its colours and texture interpolated, blended over
/// the canvas with premultiplied alpha as egui's painters do.
fn fill_triangle(
    canvas: &mut [[f32; 4]],
    width: u32,
    clip: Rect,
    texture: &ColorImage,
    [a, b, c]: [egui::epaint::Vertex; 3],
) {
    let area = edge(a.pos, b.pos, c.pos);
    if area.abs() < 1e-6 {
        return;
    }
    let min_x = a
        .pos
        .x
        .min(b.pos.x)
        .min(c.pos.x)
        .max(clip.min.x)
        .floor()
        .max(0.0) as u32;
    let max_x = a
        .pos
        .x
        .max(b.pos.x)
        .max(c.pos.x)
        .min(clip.max.x)
        .ceil()
        .max(0.0) as u32;
    let min_y = a
        .pos
        .y
        .min(b.pos.y)
        .min(c.pos.y)
        .max(clip.min.y)
        .floor()
        .max(0.0) as u32;
    let max_y = a
        .pos
        .y
        .max(b.pos.y)
        .max(c.pos.y)
        .min(clip.max.y)
        .ceil()
        .max(0.0) as u32;
    let rgba = |color: Color32| color.to_array().map(|c| f32::from(c) / 255.0);
    let [ca, cb, cc] = [a.color, b.color, c.color].map(rgba);
    for y in min_y..max_y {
        for x in min_x..max_x {
            let p = Pos2::new(x as f32 + 0.5, y as f32 + 0.5);
            if !clip.contains(p) {
                continue;
            }
            let wa = edge(b.pos, c.pos, p) / area;
            let wb = edge(c.pos, a.pos, p) / area;
            let wc = edge(a.pos, b.pos, p) / area;
            if wa < 0.0 || wb < 0.0 || wc < 0.0 {
                continue;
            }
            let uv = Pos2::new(
                wa * a.uv.x + wb * b.uv.x + wc * c.uv.x,
                wa * a.uv.y + wb * b.uv.y + wc * c.uv.y,
            );
            let texel = sample(texture, uv);
            let src: [f32; 4] =
                std::array::from_fn(|i| (wa * ca[i] + wb * cb[i] + wc * cc[i]) * texel[i]);
            let dst = &mut canvas[(y * width + x) as usize];
            for i in 0..4 {
                dst[i] = src[i] + dst[i] * (1.0 - src[3]);
            }
        }
    }
}

/// The texture at `uv`, bilinear and clamped to its edges, premultiplied.
fn sample(texture: &ColorImage, uv: Pos2) -> [f32; 4] {
    let [w, h] = texture.size;
    let x = (uv.x * w as f32 - 0.5).clamp(0.0, (w - 1) as f32);
    let y = (uv.y * h as f32 - 0.5).clamp(0.0, (h - 1) as f32);
    let (x0, y0) = (x.floor() as usize, y.floor() as usize);
    let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
    let (fx, fy) = (x - x0 as f32, y - y0 as f32);
    let at = |x: usize, y: usize| {
        texture.pixels[y * w + x]
            .to_array()
            .map(|c| f32::from(c) / 255.0)
    };
    let (p00, p10, p01, p11) = (at(x0, y0), at(x1, y0), at(x0, y1), at(x1, y1));
    std::array::from_fn(|i| {
        let top = p00[i] * (1.0 - fx) + p10[i] * fx;
        let bottom = p01[i] * (1.0 - fx) + p11[i] * fx;
        top * (1.0 - fy) + bottom * fy
    })
}

/// Files trashed on two days, the photos of the sample folders.
fn bin() -> Vec<BinItem> {
    let day = 86_400;
    let monday = 1_793_000_000;
    folders()
        .into_values()
        .flatten()
        .step_by(3)
        .enumerate()
        .map(|(i, original)| {
            let file = PathBuf::from("/home/you/.local/share/Trash/files")
                .join(original.file_name().unwrap_or_default());
            BinItem {
                id: format!("{}.trashinfo", file.display()).into(),
                file,
                original: Some(original),
                deleted: monday + if i < 6 { day } else { 0 } + i as i64 * 60,
            }
        })
        .collect()
}

/// pixpixpix the default for JPEG and PNG, the others with an image viewer
/// and a video player.
fn defaults() -> Defaults {
    let ours = "io.github.paxel.pixpixpix.desktop".to_owned();
    let rows = pixpixpix::associations::TYPES
        .iter()
        .map(|t| {
            let current = match (t.name, t.video) {
                ("JPEG" | "PNG", _) => ours.clone(),
                (_, false) => "org.gnome.Loupe.desktop".to_owned(),
                (_, true) => "org.videolan.VLC.desktop".to_owned(),
            };
            DefaultRow {
                name: t.name.to_owned(),
                video: t.video,
                ours: current == ours,
                current: Some(current),
            }
        })
        .collect();
    let names = BTreeMap::from([
        (ours.clone(), "pixpixpix".to_owned()),
        (
            "org.gnome.Loupe.desktop".to_owned(),
            "Image Viewer".to_owned(),
        ),
        (
            "org.videolan.VLC.desktop".to_owned(),
            "VLC media player".to_owned(),
        ),
    ]);
    Defaults {
        rows,
        names,
        ours,
        read_only: false,
        error: None,
    }
}

fn first_file() -> PathBuf {
    Path::new(ROOT).join("IMG_4005.jpg")
}

/// The share of the canvas that is not the window's background.
fn drawn(image: &image::RgbaImage) -> f64 {
    let background = image.get_pixel(0, image.height() - 1).0;
    let differ = image.pixels().filter(|p| p.0 != background).count();
    differ as f64 / f64::from(image.width() * image.height())
}

#[test]
fn the_image_view_renders_the_picture_and_the_key_line() {
    let shot = Shot::start(&first_file());
    let image = shot.render();
    // The middle of the window shows the sample picture, not the background.
    let middle = image.get_pixel(image.width() / 2, image.height() / 3).0;
    assert_ne!(middle, [0x12, 0x12, 0x14, 255]);
    assert!(drawn(&image) > 0.3, "{}", drawn(&image));
}

#[test]
#[ignore = "writes the site's screenshots; run with --ignored"]
fn write_the_site_screenshots() {
    let out = Path::new(env!("CARGO_MANIFEST_DIR")).join("site/assets/screenshots");
    std::fs::create_dir_all(&out).unwrap();
    let save = |shot: &Shot, name: &str| {
        shot.render().save(out.join(format!("{name}.png"))).unwrap();
    };
    let mut shot = Shot::start(&first_file());
    save(&shot, "image-view");
    shot.press(Key::N);
    save(&shot, "dir-column");
    let mut shot = Shot::start(&first_file());
    shot.press(Key::G);
    save(&shot, "overview");
    let mut shot = Shot::start(&first_file());
    shot.press(Key::T);
    save(&shot, "trash");
    let mut shot = Shot::start(&first_file());
    shot.press(Key::S);
    save(&shot, "settings");
}
