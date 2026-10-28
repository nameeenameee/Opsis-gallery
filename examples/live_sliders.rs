use eframe::egui;
use image::{ImageBuffer, Rgba, RgbaImage};
use std::time::{Duration, Instant};

// Adjust these imports to match your crate name and module structure!
use opsis_pipeline_demo::engine::PipelineEngine;
use opsis_pipeline_demo::state::EditState;

struct LiveBenchApp {
    proxy_source: RgbaImage,
    state: EditState,
    texture: Option<egui::TextureHandle>,
    last_render_time: Duration,
    needs_recompute: bool,
}

impl LiveBenchApp {
    fn new(image_path: &str) -> Self {
        // Load master image or generate a synthetic 50MP->1080p test pattern if no file is passed
        let proxy_source = if let Ok(img) = image::open(image_path) {
            // Downsample once to 1080p proxy (Triangle filter)
            img.resize(1920, 1080, image::imageops::FilterType::Triangle)
                .to_rgba8()
        } else {
            println!("No valid image found at '{}'. Generating synthetic 1920x1080 gradient test buffer...", image_path);
            ImageBuffer::from_fn(1920, 1080, |x, y| {
                let r = ((x as f32 / 1920.0) * 255.0) as u8;
                let g = ((y as f32 / 1080.0) * 255.0) as u8;
                let b = (((x + y) as f32 / 3000.0) * 255.0) as u8;
                Rgba([r, g, b, 255])
            })
        };

        Self {
            proxy_source,
            state: EditState::default(),
            texture: None,
            last_render_time: Duration::ZERO,
            needs_recompute: true,
        }
    }
}

impl eframe::App for LiveBenchApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Side panel for interactive sliders & telemetry
        egui::SidePanel::left("controls_panel").min_width(280.0).show(ctx, |ui| {
            ui.heading("Module 3 Live Bench");
            ui.separator();

            // Live Telemetry Readout
            let ms = self.last_render_time.as_secs_f64() * 1000.0;
            let fps = if ms > 0.0 { 1000.0 / ms } else { 0.0 };
            ui.label(format!("Render Time: {:.2} ms ({:.0} FPS)", ms, fps));
            
            let budget_ratio = (ms / 16.66) as f32;
            ui.add(
                egui::ProgressBar::new(budget_ratio.clamp(0.0, 1.0))
                    .text(format!("{:.0}% of 16.6ms (60 FPS) budget", budget_ratio * 100.0)),
            );
            ui.separator();

            // Sliders: `.changed()` returns true on the exact frame the user drags the slider
            let mut changed = false;
            changed |= ui.add(egui::Slider::new(&mut self.state.exposure, -3.0..=3.0).text("Exposure (EV)")).changed();
            changed |= ui.add(egui::Slider::new(&mut self.state.brightness, -0.5..=0.5).text("Brightness")).changed();
            changed |= ui.add(egui::Slider::new(&mut self.state.contrast, -0.5..=0.5).text("Contrast")).changed();
            changed |= ui.add(egui::Slider::new(&mut self.state.saturation, -1.0..=1.0).text("Saturation")).changed();
            changed |= ui.add(egui::Slider::new(&mut self.state.vibrance, -1.0..=1.0).text("Vibrance")).changed();
            changed |= ui.add(egui::Slider::new(&mut self.state.temperature, -1.0..=1.0).text("Temperature")).changed();
            changed |= ui.add(egui::Slider::new(&mut self.state.rotation_degrees, -180.0..=180.0).text("Rotation (°)")).changed();

            ui.separator();
            if ui.button("Revert (ED-05)").clicked() {
                self.state = EditState::default();
                changed = true;
            }

            if changed {
                self.needs_recompute = true;
            }
        });

        // Re-run Rayon pipeline ONLY when a slider is actively dragged
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

        // Main viewport displaying the 1080p image
        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(tex) = &self.texture {
                ui.add(egui::Image::new(tex).shrink_to_fit());
            }
        });
    }
}

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let img_path = args.get(1).map(|s| s.as_str()).unwrap_or("sample.jpg");

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1280.0, 720.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Opsis Module 3 - Real-Time Slider Benchmark",
        options,
        Box::new(|_cc| Ok(Box::new(LiveBenchApp::new(img_path)))),
    )
}