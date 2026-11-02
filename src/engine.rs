use crate::state::{CropArea, EditState, FreehandStroke, Preset, Sticker};
use image::{DynamicImage, ImageBuffer, Rgba, RgbaImage};
use rayon::prelude::*;

pub struct PipelineEngine;

impl PipelineEngine {
    /// Two-tier downsampler: creates an aspect-ratio-preserving 1080p proxy buffer
    pub fn create_preview_buffer(img: &RgbaImage, max_dim: u32) -> RgbaImage {
        let (w, h) = img.dimensions();
        if w > max_dim || h > max_dim {
            let ratio = if w > h {
                max_dim as f32 / w as f32
            } else {
                max_dim as f32 / h as f32
            };
            let nw = (w as f32 * ratio).round().max(1.0) as u32;
            let nh = (h as f32 * ratio).round().max(1.0) as u32;
            let dyn_img = DynamicImage::ImageRgba8(img.clone());
            dyn_img
                .resize(nw, nh, image::imageops::FilterType::Triangle)
                .to_rgba8()
        } else {
            img.clone()
        }
    }

    /// Primary execution flow returning RgbaImage
    pub fn render(source: &RgbaImage, state: &EditState) -> RgbaImage {
        let mut buffer = source.clone();

        // 1. Safe ROI crop
        if let Some(crop) = &state.crop {
            buffer = Self::apply_crop(&buffer, crop);
        }

        // 2. Geometric rotation
        if state.rotation_degrees.abs() > 0.001 {
            buffer = Self::rotate(&buffer, state.rotation_degrees);
        }

        // 3. Scanline-parallel color adjustments (skipped if default state)
        if !Self::is_color_identity(state) {
            Self::process_pixels(&mut buffer, state);
        }

        // 4. Presets
        Self::apply_preset(&mut buffer, state.preset);

        // 5. Freehand canvas strokes
        for stroke in &state.strokes {
            Self::draw_stroke(&mut buffer, stroke);
        }

        // 6. Stickers
        for sticker in &state.stickers {
            Self::draw_sticker(&mut buffer, sticker);
        }

        buffer
    }

    #[inline]
    fn is_color_identity(state: &EditState) -> bool {
        state.exposure.abs() <= 0.0001
            && (state.temperature - 6500.0).abs() <= 0.01
            && state.brightness.abs() <= 0.0001
            && (state.contrast - 1.0).abs() <= 0.0001
            && (state.saturation - 1.0).abs() <= 0.0001
            && state.vibrance.abs() <= 0.0001
    }

    /// Clamped crop avoiding out-of-bounds or zero-dimension panics
    pub fn apply_crop(img: &RgbaImage, crop: &CropArea) -> RgbaImage {
        let (w, h) = img.dimensions();
        if w == 0 || h == 0 || crop.width == 0 || crop.height == 0 {
            return img.clone();
        }

        let x = crop.x.min(w.saturating_sub(1));
        let y = crop.y.min(h.saturating_sub(1));
        let safe_w = crop.width.min(w.saturating_sub(x));
        let safe_h = crop.height.min(h.saturating_sub(y));

        if safe_w == 0 || safe_h == 0 {
            return img.clone();
        }

        image::imageops::crop_imm(img, x, y, safe_w, safe_h).to_image()
    }

    /// Multithreaded scanline color pipeline
    pub fn process_pixels(buffer: &mut RgbaImage, state: &EditState) {
        let (width, _) = buffer.dimensions();
        let stride = (width * 4) as usize;

        let exposure_mult = 2.0_f32.powf(state.exposure);
        let (r_temp, g_temp, b_temp) = Self::kelvin_to_rgb(state.temperature);

        buffer.par_chunks_exact_mut(stride).for_each(|scanline| {
            for pixel in scanline.chunks_exact_mut(4) {
                let mut r = pixel[0] as f32 / 255.0;
                let mut g = pixel[1] as f32 / 255.0;
                let mut b = pixel[2] as f32 / 255.0;

                // sRGB to Linear RGB
                r = Self::srgb_to_linear(r);
                g = Self::srgb_to_linear(g);
                b = Self::srgb_to_linear(b);

                // Exposure and White Balance multipliers
                r *= exposure_mult * r_temp;
                g *= exposure_mult * g_temp;
                b *= exposure_mult * b_temp;

                // Linear RGB back to sRGB
                r = Self::linear_to_srgb(r);
                g = Self::linear_to_srgb(g);
                b = Self::linear_to_srgb(b);

                // Adjust value instead of each RGB channel independently so
                // reducing contrast does not desaturate the image.
                let contrast = state.contrast.max(0.1);
                let (h_val, s_val, mut v_val) = Self::rgb_to_hsv(r, g, b);
                v_val = (v_val - 0.5) * contrast + 0.5 + state.brightness;
                let (contrasted_r, contrasted_g, contrasted_b) =
                    Self::hsv_to_rgb(h_val, s_val, v_val.clamp(0.0, 1.0));
                r = contrasted_r;
                g = contrasted_g;
                b = contrasted_b;

                r = r.clamp(0.0, 1.0);
                g = g.clamp(0.0, 1.0);
                b = b.clamp(0.0, 1.0);

                // Saturation & Vibrance in HSV space
                if (state.saturation - 1.0).abs() > 0.001 || state.vibrance.abs() > 0.001 {
                    let (h_val, mut s_val, v_val) = Self::rgb_to_hsv(r, g, b);
                    s_val *= state.saturation;

                    if state.vibrance >= 0.0 {
                        s_val += state.vibrance * s_val * (1.0 - s_val);
                    } else {
                        s_val *= 1.0 + state.vibrance;
                    }
                    s_val = s_val.clamp(0.0, 1.0);

                    let (new_r, new_g, new_b) = Self::hsv_to_rgb(h_val, s_val, v_val);
                    r = new_r;
                    g = new_g;
                    b = new_b;
                }

                pixel[0] = (r.clamp(0.0, 1.0) * 255.0) as u8;
                pixel[1] = (g.clamp(0.0, 1.0) * 255.0) as u8;
                pixel[2] = (b.clamp(0.0, 1.0) * 255.0) as u8;
            }
        });
    }

    pub fn apply_preset(buffer: &mut RgbaImage, preset: Preset) {
        match preset {
            Preset::None => {}
            Preset::Monochrome => {
                buffer.par_chunks_exact_mut(4).for_each(|pixel| {
                    let lum = (0.299 * pixel[0] as f32
                        + 0.587 * pixel[1] as f32
                        + 0.114 * pixel[2] as f32) as u8;
                    pixel[0] = lum;
                    pixel[1] = lum;
                    pixel[2] = lum;
                });
            }
            Preset::Sepia => {
                buffer.par_chunks_exact_mut(4).for_each(|pixel| {
                    let (r, g, b) = (pixel[0] as f32, pixel[1] as f32, pixel[2] as f32);
                    pixel[0] = ((r * 0.393) + (g * 0.769) + (b * 0.189)).min(255.0) as u8;
                    pixel[1] = ((r * 0.349) + (g * 0.686) + (b * 0.168)).min(255.0) as u8;
                    pixel[2] = ((r * 0.272) + (g * 0.534) + (b * 0.131)).min(255.0) as u8;
                });
            }
            Preset::Vintage => {
                buffer.par_chunks_exact_mut(4).for_each(|pixel| {
                    let (r, g, b) = (pixel[0] as f32, pixel[1] as f32, pixel[2] as f32);
                    pixel[0] = ((r * 0.9) + 20.0).min(255.0) as u8;
                    pixel[1] = ((g * 0.85) + 15.0).min(255.0) as u8;
                    pixel[2] = (b * 0.75).min(255.0) as u8;
                });
            }
        }
    }

    /// Inverse-mapped bilinear interpolation rotation
    pub fn rotate(img: &RgbaImage, degrees: f32) -> RgbaImage {
        let rad = degrees.to_radians();
        let cos_a = rad.cos();
        let sin_a = rad.sin();

        let (w, h) = img.dimensions();
        let (wf, hf) = (w as f32, h as f32);

        // Round to eliminate small sub-epsilon trigonometric residuals (e.g. cos(90 deg) == -4e-8)
        let new_w = (wf * cos_a.abs() + hf * sin_a.abs()).round().max(1.0) as u32;
        let new_h = (wf * sin_a.abs() + hf * cos_a.abs()).round().max(1.0) as u32;

        let src_cx = wf / 2.0;
        let src_cy = hf / 2.0;
        let dst_cx = new_w as f32 / 2.0;
        let dst_cy = new_h as f32 / 2.0;

        let mut out = ImageBuffer::new(new_w, new_h);

        out.par_enumerate_pixels_mut().for_each(|(x, y, pixel)| {
            let dx = x as f32 - dst_cx;
            let dy = y as f32 - dst_cy;

            let src_x = cos_a * dx + sin_a * dy + src_cx;
            let src_y = -sin_a * dx + cos_a * dy + src_cy;

            if src_x >= 0.0 && src_x < wf - 1.0 && src_y >= 0.0 && src_y < hf - 1.0 {
                *pixel = Self::bilinear_sample(img, src_x, src_y);
            } else {
                *pixel = Rgba([0, 0, 0, 0]);
            }
        });

        out
    }

    pub fn bilinear_sample(img: &RgbaImage, x: f32, y: f32) -> Rgba<u8> {
        let x0 = x.floor() as u32;
        let y0 = y.floor() as u32;
        let x1 = (x0 + 1).min(img.width() - 1);
        let y1 = (y0 + 1).min(img.height() - 1);

        let fx = x - x0 as f32;
        let fy = y - y0 as f32;

        let p00 = img.get_pixel(x0, y0).0;
        let p10 = img.get_pixel(x1, y0).0;
        let p01 = img.get_pixel(x0, y1).0;
        let p11 = img.get_pixel(x1, y1).0;

        let mut res = [0u8; 4];
        for c in 0..4 {
            let top = p00[c] as f32 * (1.0 - fx) + p10[c] as f32 * fx;
            let bottom = p01[c] as f32 * (1.0 - fx) + p11[c] as f32 * fx;
            res[c] = (top * (1.0 - fy) + bottom * fy).clamp(0.0, 255.0) as u8;
        }
        Rgba(res)
    }

    fn draw_stroke(img: &mut RgbaImage, stroke: &FreehandStroke) {
        if stroke.points.is_empty() {
            return;
        }
        let r = (stroke.thickness as i32 / 2).max(1);
        let color = stroke.color;

        for window in stroke.points.windows(2) {
            let (x0, y0) = window[0];
            let (x1, y1) = window[1];
            Self::draw_line_segment(img, x0, y0, x1, y1, r, color);
        }

        if stroke.points.len() == 1 {
            let (x, y) = stroke.points[0];
            Self::draw_circle_brush(img, x, y, r, color);
        }
    }

    fn draw_line_segment(
        img: &mut RgbaImage,
        mut x0: i32,
        mut y0: i32,
        x1: i32,
        y1: i32,
        r: i32,
        color: [u8; 4],
    ) {
        let dx = (x1 - x0).abs();
        let dy = -(y1 - y0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut err = dx + dy;

        loop {
            Self::draw_circle_brush(img, x0, y0, r, color);
            if x0 == x1 && y0 == y1 {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x0 += sx;
            }
            if e2 <= dx {
                err += dx;
                y0 += sy;
            }
        }
    }

    fn draw_circle_brush(img: &mut RgbaImage, cx: i32, cy: i32, r: i32, color: [u8; 4]) {
        let (w, h) = img.dimensions();
        let src_color = Rgba(color);

        for dy in -r..=r {
            for dx in -r..=r {
                if dx * dx + dy * dy <= r * r {
                    let px = cx + dx;
                    let py = cy + dy;
                    if px >= 0 && px < w as i32 && py >= 0 && py < h as i32 {
                        let dst = img.get_pixel_mut(px as u32, py as u32);
                        *dst = Self::blend_porter_duff(*dst, src_color);
                    }
                }
            }
        }
    }

    fn draw_sticker(img: &mut RgbaImage, sticker: &Sticker) {
        let (w, h) = img.dimensions();
        let sticker_color = Rgba(sticker.color);

        for dy in 0..sticker.height as i32 {
            for dx in 0..sticker.width as i32 {
                let px = sticker.x + dx;
                let py = sticker.y + dy;
                if px >= 0 && px < w as i32 && py >= 0 && py < h as i32 {
                    let dst = img.get_pixel_mut(px as u32, py as u32);
                    *dst = Self::blend_porter_duff(*dst, sticker_color);
                }
            }
        }
    }

    pub fn blend_porter_duff(dst: Rgba<u8>, src: Rgba<u8>) -> Rgba<u8> {
        let sa = src[3] as f32 / 255.0;
        let da = dst[3] as f32 / 255.0;
        let out_a = sa + da * (1.0 - sa);

        if out_a <= 0.0 {
            return Rgba([0, 0, 0, 0]);
        }

        let blend = |sc: u8, dc: u8| -> u8 {
            let sc_f = sc as f32 / 255.0;
            let dc_f = dc as f32 / 255.0;
            let out_c = (sc_f * sa + dc_f * da * (1.0 - sa)) / out_a;
            (out_c.clamp(0.0, 1.0) * 255.0) as u8
        };

        Rgba([
            blend(src[0], dst[0]),
            blend(src[1], dst[1]),
            blend(src[2], dst[2]),
            (out_a * 255.0) as u8,
        ])
    }

    #[inline(always)]
    fn srgb_to_linear(c: f32) -> f32 {
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    }

    #[inline(always)]
    fn linear_to_srgb(c: f32) -> f32 {
        if c <= 0.0031308 {
            c * 12.92
        } else {
            1.055 * c.powf(1.0 / 2.4) - 0.055
        }
    }

    fn kelvin_to_rgb(k: f32) -> (f32, f32, f32) {
        if (k - 6500.0).abs() <= 0.01 {
            return (1.0, 1.0, 1.0);
        }

        let temp = (k / 100.0).clamp(20.0, 120.0);
        let r = if temp <= 66.0 {
            1.0
        } else {
            (temp - 60.0).powf(-0.1332) * 1.2
        };
        let b = if temp >= 66.0 {
            1.0
        } else if temp <= 19.0 {
            0.0
        } else {
            (temp - 10.0).ln() * 0.5432 - 1.196
        };
        (r.clamp(0.0, 2.0), 1.0, b.clamp(0.0, 2.0))
    }

    fn rgb_to_hsv(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let delta = max - min;

        let h = if delta == 0.0 {
            0.0
        } else if max == r {
            60.0 * (((g - b) / delta) % 6.0)
        } else if max == g {
            60.0 * (((b - r) / delta) + 2.0)
        } else {
            60.0 * (((r - g) / delta) + 4.0)
        };

        let h_norm = if h < 0.0 { h + 360.0 } else { h };
        let s = if max == 0.0 { 0.0 } else { delta / max };
        (h_norm, s, max)
    }

    fn hsv_to_rgb(h: f32, s: f32, v: f32) -> (f32, f32, f32) {
        let c = v * s;
        let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
        let m = v - c;

        let (r1, g1, b1) = match (h as u32) / 60 {
            0 => (c, x, 0.0),
            1 => (x, c, 0.0),
            2 => (0.0, c, x),
            3 => (0.0, x, c),
            4 => (x, 0.0, c),
            _ => (c, 0.0, x),
        };

        (r1 + m, g1 + m, b1 + m)
    }
}
