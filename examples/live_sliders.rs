use eframe::egui;
use image::{ImageBuffer, Rgba, RgbaImage};
use std::fs;
use std::time::{Duration, Instant};
use uuid::Uuid;

use opsis_pipeline_demo::engine::PipelineEngine;
use opsis_pipeline_demo::state::EditState;
use opsis_pipeline_demo::sticker_extractor::{
    extract_fallback_region_grow, ExtractedSticker, YoloDetectionCache,
};

use arboard::{Clipboard, ImageData};
use std::borrow::Cow;
use std::sync::OnceLock;

struct ActiveOverlay {
    bounds_norm: (f32, f32, f32, f32),
    texture: egui::TextureHandle,
}

struct LiveBenchApp {
    proxy_source: RgbaImage,
    yolo_cache: OnceLock<Option<YoloDetectionCache>>,
    state: EditState,
    texture: Option<egui::TextureHandle>,
    last_render_time: Duration,
    needs_recompute: bool,
    temperature_control: f32,
    extraction_status: Option<String>,

    // Sticker extraction state
    is_holding_sticker: bool,
    active_overlay: Option<ActiveOverlay>,
    extracted_sticker: Option<ExtractedSticker>,
    sticker_preview_texture: Option<egui::TextureHandle>,
}

impl LiveBenchApp {
    fn new(image_path: &str) -> Self {
        let proxy_source = if let Ok(img) = image::open(image_path) {
            PipelineEngine::create_preview_buffer(&img.to_rgba8(), 1080)
        } else {
            println!(
                "No valid image found at '{}'. Generating synthetic 1920x1080 gradient test buffer...",
                image_path
            );
            ImageBuffer::from_fn(1920, 1080, |x, y| {
                let r = ((x as f32 / 1920.0) * 255.0) as u8;
                let g = ((y as f32 / 1080.0) * 255.0) as u8;
                let b = (((x + y) as f32 / 3000.0) * 255.0) as u8;
                Rgba([r, g, b, 255])
            })
        };

        Self {
            proxy_source,
            yolo_cache: OnceLock::new(),
            state: EditState::default(),
            texture: None,
            last_render_time: Duration::ZERO,
            needs_recompute: true,
            temperature_control: 0.0,
            extraction_status: None,
            is_holding_sticker: false,
            active_overlay: None,
            extracted_sticker: None,
            sticker_preview_texture: None,
        }
    }

    /// Creates a copy of the sticker with a crisp white silhouette border for the hold overlay
    fn build_outlined_overlay(sticker: &RgbaImage, border_px: i32) -> RgbaImage {
        let (w, h) = sticker.dimensions();
        let pad = border_px.max(1) as u32;
        let out_w = w + pad * 2;
        let out_h = h + pad * 2;
        let mut out = RgbaImage::from_pixel(out_w, out_h, Rgba([0, 0, 0, 0]));

        // Pass 1: Draw white border around any opaque sticker pixel
        for y in 0..h {
            for x in 0..w {
                if sticker.get_pixel(x, y)[3] > 32 {
                    for dy in -border_px..=border_px {
                        for dx in -border_px..=border_px {
                            if dx * dx + dy * dy <= border_px * border_px {
                                let ox = (x + pad) as i32 + dx;
                                let oy = (y + pad) as i32 + dy;
                                if ox >= 0 && ox < out_w as i32 && oy >= 0 && oy < out_h as i32 {
                                    out.put_pixel(ox as u32, oy as u32, Rgba([255, 255, 255, 255]));
                                }
                            }
                        }
                    }
                }
            }
        }

        // Pass 2: Composite original sticker pixels on top
        for y in 0..h {
            for x in 0..w {
                let p = *sticker.get_pixel(x, y);
                if p[3] > 32 {
                    out.put_pixel(x + pad, y + pad, p);
                }
            }
        }

        out
    }
}

impl eframe::App for LiveBenchApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::SidePanel::left("controls_panel")
            .min_width(280.0)
            .max_width(320.0)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.heading("Module 3 Live Bench");
                    ui.separator();

                    // Live Telemetry Readout
                    let ms = self.last_render_time.as_secs_f64() * 1000.0;
                    let fps = if ms > 0.0 { 1000.0 / ms } else { 0.0 };
                    ui.label(format!("Render Time: {:.2} ms ({:.0} FPS)", ms, fps));

                    let budget_ratio = (ms / 16.66) as f32;
                    ui.add(
                        egui::ProgressBar::new(budget_ratio.clamp(0.0, 1.0)).text(format!(
                            "{:.0}% of 16.6ms (60 FPS) budget",
                            budget_ratio * 100.0
                        )),
                    );
                    ui.separator();

                    // Sliders
                    let mut changed = false;

                    changed |= ui
                        .add(
                            egui::Slider::new(&mut self.state.exposure, -3.0..=3.0)
                                .text("Exposure (EV)"),
                        )
                        .changed();

                    changed |= ui
                        .add(
                            egui::Slider::new(&mut self.state.brightness, -0.5..=0.5)
                                .text("Brightness"),
                        )
                        .changed();

                    changed |= ui
                        .add(
                            egui::Slider::new(&mut self.state.contrast, 0.0..=2.0).text("Contrast"),
                        )
                        .changed();

                    changed |= ui
                        .add(
                            egui::Slider::new(&mut self.state.saturation, 0.0..=2.0)
                                .text("Saturation"),
                        )
                        .changed();

                    changed |= ui
                        .add(
                            egui::Slider::new(&mut self.state.vibrance, -1.0..=1.0)
                                .text("Vibrance"),
                        )
                        .changed();

                    changed |= ui
                        .add(
                            egui::Slider::new(&mut self.temperature_control, -1.0..=1.0)
                                .text("Temperature")
                                .step_by(0.01)
                                .fixed_decimals(2),
                        )
                        .changed();
                    if changed {
                        self.state.temperature = if self.temperature_control < 0.0 {
                            6500.0 + self.temperature_control * 4500.0
                        } else {
                            6500.0 + self.temperature_control * 5500.0
                        };
                    }

                    changed |= ui
                        .add(
                            egui::Slider::new(&mut self.state.rotation_degrees, -180.0..=180.0)
                                .text("Rotation (°)"),
                        )
                        .changed();

                    ui.separator();
                    if ui.button("Revert (ED-05)").clicked() {
                        self.state = EditState::default();
                        self.temperature_control = 0.0;
                        self.extraction_status = None;
                        self.active_overlay = None;
                        self.extracted_sticker = None;
                        self.sticker_preview_texture = None;
                        changed = true;
                    }

                    ui.separator();
                    ui.heading("Sticker Extractor");
                    ui.label("Press and hold on the image to preview cutout.");

                    // Persistent Left Preview Box
                    egui::Frame::group(ui.style())
                        .fill(egui::Color32::from_gray(24))
                        .stroke(egui::Stroke::new(1.0, egui::Color32::from_gray(70)))
                        .inner_margin(8.0)
                        .show(ui, |ui| {
                            ui.set_min_width(ui.available_width());
                            ui.set_min_height(150.0);

                            if let Some(preview_tex) = &self.sticker_preview_texture {
                                ui.centered_and_justified(|ui| {
                                    ui.add(
                                        egui::Image::new(preview_tex)
                                            .max_size(egui::vec2(240.0, 160.0))
                                            .shrink_to_fit(),
                                    );
                                });
                            } else {
                                ui.centered_and_justified(|ui| {
                                    ui.weak("No sticker extracted yet");
                                });
                            }
                        });

                    if let Some(sticker) = &self.extracted_sticker {
                        if ui.button("Save PNG to ./stickers").clicked() {
                            if fs::create_dir_all("./stickers").is_ok() {
                                let path = format!("./stickers/sticker-{}.png", Uuid::new_v4());
                                match sticker.image.save(&path) {
                                    Ok(_) => {
                                        self.extraction_status = Some(format!("Saved: {}", path));
                                    }
                                    Err(e) => {
                                        self.extraction_status =
                                            Some(format!("Save failed: {}", e));
                                    }
                                }
                            }
                        }

                        if ui.button("Copy image to Windows clipboard").clicked() {
                            let width = sticker.image.width() as usize;
                            let height = sticker.image.height() as usize;
                            let raw_rgba = sticker.image.as_raw();

                            match Clipboard::new().and_then(|mut cb| {
                                cb.set_image(ImageData {
                                    width,
                                    height,
                                    bytes: Cow::Borrowed(raw_rgba),
                                })
                            }) {
                                Ok(_) => {
                                    self.extraction_status =
                                        Some("Copied sticker to clipboard.".to_string());
                                }
                                Err(e) => {
                                    self.extraction_status =
                                        Some(format!("Clipboard copy failed: {}", e));
                                }
                            }
                        }
                    }

                    if let Some(status) = &self.extraction_status {
                        ui.label(status);
                    }

                    if changed {
                        self.needs_recompute = true;
                    }
                });
            });

        // Re-run Rayon pipeline ONLY when sliders/state change
        if self.needs_recompute {
            let start = Instant::now();
            let rendered = PipelineEngine::render(&self.proxy_source, &self.state);
            self.last_render_time = start.elapsed();

            let size = [rendered.width() as usize, rendered.height() as usize];
            let color_image = egui::ColorImage::from_rgba_unmultiplied(size, rendered.as_raw());

            match &mut self.texture {
                Some(tex) => tex.set(color_image, egui::TextureOptions::LINEAR),
                None => {
                    self.texture = Some(ctx.load_texture(
                        "preview_tex",
                        color_image,
                        egui::TextureOptions::LINEAR,
                    ));
                }
            }
            self.needs_recompute = false;
        }

        // Main viewport displaying the rendered image
        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(tex) = &self.texture {
                let response = ui.add(
                    egui::Image::new(tex)
                        .shrink_to_fit()
                        .sense(egui::Sense::click_and_drag()),
                );

                let is_pressed_on_image = response.is_pointer_button_down_on();

                if is_pressed_on_image && !self.is_holding_sticker {
                    // Pointer just pressed down: extract once and show overlay + left preview
                    self.is_holding_sticker = true;

                    if let Some(pointer) = ctx.input(|i| i.pointer.interact_pos()) {
                        let u = ((pointer.x - response.rect.left()) / response.rect.width())
                            .clamp(0.0, 1.0);
                        let v = ((pointer.y - response.rect.top()) / response.rect.height())
                            .clamp(0.0, 1.0);

                        let cache = self
                            .yolo_cache
                            .get_or_init(|| YoloDetectionCache::build(&self.proxy_source));
                        let extracted_opt = match cache {
                            Some(yolo) => yolo.extract_at(&self.proxy_source, u, v),
                            None => extract_fallback_region_grow(&self.proxy_source, u, v, 40, 70),
                        };

                        match extracted_opt {
                            Some(extracted) => {
                                // 1. Update persistent left preview box texture
                                let prev_size = [
                                    extracted.image.width() as usize,
                                    extracted.image.height() as usize,
                                ];
                                let prev_color_img = egui::ColorImage::from_rgba_unmultiplied(
                                    prev_size,
                                    extracted.image.as_raw(),
                                );
                                match &mut self.sticker_preview_texture {
                                    Some(t) => t.set(prev_color_img, egui::TextureOptions::LINEAR),
                                    None => {
                                        self.sticker_preview_texture = Some(ctx.load_texture(
                                            "sticker_preview_tex",
                                            prev_color_img,
                                            egui::TextureOptions::LINEAR,
                                        ));
                                    }
                                }

                                // 2. Build temporary white-bordered overlay texture
                                let outlined = Self::build_outlined_overlay(&extracted.image, 3);
                                let ov_size =
                                    [outlined.width() as usize, outlined.height() as usize];
                                let ov_color_img = egui::ColorImage::from_rgba_unmultiplied(
                                    ov_size,
                                    outlined.as_raw(),
                                );
                                let ov_tex = ctx.load_texture(
                                    "sticker_hold_overlay_tex",
                                    ov_color_img,
                                    egui::TextureOptions::LINEAR,
                                );

                                self.active_overlay = Some(ActiveOverlay {
                                    bounds_norm: extracted.bounds_norm,
                                    texture: ov_tex,
                                });
                                self.extracted_sticker = Some(extracted);
                                self.extraction_status = Some("Sticker extracted.".to_string());
                            }
                            None => {
                                self.active_overlay = None;
                                self.extraction_status =
                                    Some("No distinct region found at that point.".to_string());
                            }
                        }
                    }
                } else if !is_pressed_on_image && self.is_holding_sticker {
                    // Pointer released: remove the overlay immediately, keep left preview
                    self.is_holding_sticker = false;
                    self.active_overlay = None;
                }

                // Paint temporary overlay ONLY while holding click
                if self.is_holding_sticker {
                    if let Some(overlay) = &self.active_overlay {
                        let (bx, by, bw, bh) = overlay.bounds_norm;
                        let overlay_rect = egui::Rect::from_min_size(
                            egui::pos2(
                                response.rect.left() + bx * response.rect.width(),
                                response.rect.top() + by * response.rect.height(),
                            ),
                            egui::vec2(bw * response.rect.width(), bh * response.rect.height()),
                        );

                        // Draw outlined sticker cutout
                        ui.painter().image(
                            overlay.texture.id(),
                            overlay_rect,
                            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                            egui::Color32::WHITE,
                        );

                        // Draw crisp white bounding frame around selection
                        ui.painter().rect_stroke(
                            overlay_rect,
                            2.0,
                            egui::Stroke::new(1.5, egui::Color32::WHITE),
                        );
                    }
                }
            }
        });
    }
}

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let img_path = args
        .get(1)
        .cloned()
        .unwrap_or_else(|| "cli_output.jpg".to_string());

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 720.0])
            .with_title("Opsis Module 3 - Real-Time Slider Benchmark"),
        ..Default::default()
    };

    eframe::run_native(
        "Opsis Module 3 - Real-Time Slider Benchmark",
        options,
        Box::new(|_cc| Ok(Box::new(LiveBenchApp::new(&img_path)))),
    )
}
