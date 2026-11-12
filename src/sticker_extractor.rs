use image::{imageops::FilterType, ImageBuffer, Luma, Rgba, RgbaImage};
use imageproc::gradients::sobel_gradient_map;
use ndarray::Array4;
use ort::session::Session;
use ort::value::TensorRef;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

const INPUT_DIM: u32 = 640;
const PROTO_DIM: usize = 160;
const NUM_CLASSES: usize = 80;
const NUM_MASKS: usize = 32;
const NUM_ANCHORS: usize = 8400;

#[derive(Debug, Clone)]
pub struct ExtractedSticker {
    pub bounds_norm: (f32, f32, f32, f32), // (x, y, w, h)
    pub image: RgbaImage,
}

#[derive(Debug, Clone)]
pub struct YoloInstance {
    pub x1: f32,
    pub y1: f32,
    pub x2: f32,
    pub y2: f32,
    pub area: f32,
    pub score: f32,
    pub mask_coeffs: [f32; NUM_MASKS],
}

/// Cached per-image YOLO inference output stored inside `EditorSession`
#[derive(Debug, Clone)]
pub struct YoloDetectionCache {
    pub scale: f32,
    pub pad_x: f32,
    pub pad_y: f32,
    pub instances: Vec<YoloInstance>,
    pub protos: Vec<f32>, // Flattened [32, 160, 160]
}

static YOLO_SESSION: OnceLock<Result<Mutex<Session>, String>> = OnceLock::new();
static ORT_ENVIRONMENT: OnceLock<Result<ort::environment::Environment, String>> = OnceLock::new();

fn get_ort_environment() -> Result<&'static ort::environment::Environment, String> {
    ORT_ENVIRONMENT
        .get_or_init(|| {
            ort::init()
                .build()
                .map_err(|e| format!("ORT environment initialization failed: {e}"))
        })
        .as_ref()
        .map_err(Clone::clone)
}

fn find_yolo_model() -> Option<PathBuf> {
    let candidates = [
        PathBuf::from("models/yolo11n-seg.onnx"),
        PathBuf::from("models/yolo11n-seg-int8.onnx"),
        PathBuf::from("../models/yolo11n-seg.onnx"),
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("models/yolo11n-seg.onnx"),
    ];
    candidates.into_iter().find(|p| p.exists())
}

fn get_yolo_session() -> Result<&'static Mutex<Session>, String> {
    let res = YOLO_SESSION.get_or_init(|| {
        let model_path = find_yolo_model().ok_or_else(|| {
            format!(
                "Model not found in models/yolo11n-seg.onnx or {}/models/yolo11n-seg.onnx",
                env!("CARGO_MANIFEST_DIR")
            )
        })?;

        let environment = get_ort_environment()?;
        let session = Session::builder(environment)
            .map_err(|e| format!("ORT Session::builder failed: {e}"))?
            .with_intra_threads(4)
            .map_err(|e| format!("ORT with_intra_threads failed: {e}"))?
            .commit_from_file(&model_path)
            .map_err(|e| format!("ORT commit_from_file failed: {e}"))?;

        Ok(Mutex::new(session))
    });

    res.as_ref().map_err(|e| e.clone())
}

fn letterbox(src: &RgbaImage, target: u32) -> (RgbaImage, f32, f32, f32) {
    let (w, h) = src.dimensions();
    let scale = (target as f32 / w as f32).min(target as f32 / h as f32);
    let nw = (w as f32 * scale).round().max(1.0) as u32;
    let nh = (h as f32 * scale).round().max(1.0) as u32;

    let resized = image::imageops::resize(src, nw, nh, FilterType::Triangle);
    let pad_x = (target - nw) as f32 / 2.0;
    let pad_y = (target - nh) as f32 / 2.0;

    let mut canvas = RgbaImage::from_pixel(target, target, Rgba([114, 114, 114, 255]));
    image::imageops::overlay(
        &mut canvas,
        &resized,
        pad_x.round() as i64,
        pad_y.round() as i64,
    );

    (canvas, scale, pad_x, pad_y)
}

#[inline]
fn sigmoid(x: f32) -> f32 {
    1.0 / (1.0 + (-x).exp())
}

fn box_iou(a: &YoloInstance, b: &YoloInstance) -> f32 {
    let ix1 = a.x1.max(b.x1);
    let iy1 = a.y1.max(b.y1);
    let ix2 = a.x2.min(b.x2);
    let iy2 = a.y2.min(b.y2);

    let iw = (ix2 - ix1).max(0.0);
    let ih = (iy2 - iy1).max(0.0);
    let inter = iw * ih;
    let union = a.area + b.area - inter;

    if union <= 0.0 {
        0.0
    } else {
        inter / union
    }
}

impl YoloDetectionCache {
    /// Runs a single YOLO forward pass on `proxy` and caches all detected instances + prototypes.
    pub fn build(proxy: &RgbaImage) -> Option<Self> {
        let (src_w, src_h) = proxy.dimensions();
        if src_w == 0 || src_h == 0 {
            return None;
        }

        let session_mutex = match get_yolo_session() {
            Ok(s) => s,
            Err(err) => {
                eprintln!("[YOLO] Unavailable ({err})");
                return None;
            }
        };

        let (letterboxed, scale, pad_x, pad_y) = letterbox(proxy, INPUT_DIM);

        let mut input = Array4::<f32>::zeros((1, 3, INPUT_DIM as usize, INPUT_DIM as usize));
        for y in 0..INPUT_DIM as usize {
            for x in 0..INPUT_DIM as usize {
                let p = letterboxed.get_pixel(x as u32, y as u32);
                input[[0, 0, y, x]] = p[0] as f32 / 255.0;
                input[[0, 1, y, x]] = p[1] as f32 / 255.0;
                input[[0, 2, y, x]] = p[2] as f32 / 255.0;
            }
        }

        let mut session = session_mutex.lock().ok()?;
        let input_value = TensorRef::from_array_view((
            [1usize, 3, INPUT_DIM as usize, INPUT_DIM as usize],
            input.as_slice()?,
        ))
        .ok()?;
        let outputs = session.run(ort::inputs![input_value]).ok()?;

        let dets = outputs[0].try_extract_tensor::<f32>().ok()?.1;
        let protos_view = outputs[1].try_extract_tensor::<f32>().ok()?.1;

        // Flatten [1, 32, 160, 160] into contiguous RAM
        let mut protos = vec![0.0f32; NUM_MASKS * PROTO_DIM * PROTO_DIM];
        for m in 0..NUM_MASKS {
            for y in 0..PROTO_DIM {
                for x in 0..PROTO_DIM {
                    protos[(m * PROTO_DIM + y) * PROTO_DIM + x] =
                        protos_view[(m * PROTO_DIM + y) * PROTO_DIM + x];
                }
            }
        }

        // Collect confident anchors
        let mut raw_candidates = Vec::new();
        for a in 0..NUM_ANCHORS {
            let mut max_cls_score = 0.0f32;
            for c in 0..NUM_CLASSES {
                let s = dets[(4 + c) * NUM_ANCHORS + a];
                if s > max_cls_score {
                    max_cls_score = s;
                }
            }
            if max_cls_score < 0.25 {
                continue;
            }

            let cx = dets[a];
            let cy = dets[NUM_ANCHORS + a];
            let bw = dets[2 * NUM_ANCHORS + a];
            let bh = dets[3 * NUM_ANCHORS + a];

            let mut mask_coeffs = [0.0f32; NUM_MASKS];
            for m in 0..NUM_MASKS {
                mask_coeffs[m] = dets[(4 + NUM_CLASSES + m) * NUM_ANCHORS + a];
            }

            raw_candidates.push(YoloInstance {
                x1: cx - bw * 0.5,
                y1: cy - bh * 0.5,
                x2: cx + bw * 0.5,
                y2: cy + bh * 0.5,
                area: bw * bh,
                score: max_cls_score,
                mask_coeffs,
            });
        }

        // Greedy Non-Maximum Suppression (IoU > 0.50)
        raw_candidates.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let mut instances: Vec<YoloInstance> = Vec::new();
        for cand in raw_candidates {
            if instances.iter().all(|kept| box_iou(kept, &cand) < 0.50) {
                instances.push(cand);
            }
        }

        if instances.is_empty() {
            return None;
        }

        Some(Self {
            scale,
            pad_x,
            pad_y,
            instances,
            protos,
        })
    }

    /// Fast <1ms mask query against precomputed detections
    pub fn extract_at(
        &self,
        proxy: &RgbaImage,
        seed_u: f32,
        seed_v: f32,
    ) -> Option<ExtractedSticker> {
        let (src_w, src_h) = proxy.dimensions();
        if src_w == 0 || src_h == 0 {
            return None;
        }

        let seed_lx = seed_u * src_w as f32 * self.scale + self.pad_x;
        let seed_ly = seed_v * src_h as f32 * self.scale + self.pad_y;

        let proto_scale = PROTO_DIM as f32 / INPUT_DIM as f32;
        let seed_px = (seed_lx * proto_scale).clamp(0.0, (PROTO_DIM - 1) as f32) as usize;
        let seed_py = (seed_ly * proto_scale).clamp(0.0, (PROTO_DIM - 1) as f32) as usize;

        let mut best: Option<&YoloInstance> = None;

        for inst in &self.instances {
            if seed_lx < inst.x1 || seed_lx > inst.x2 || seed_ly < inst.y1 || seed_ly > inst.y2 {
                continue;
            }

            let mut seed_logit = 0.0f32;
            for m in 0..NUM_MASKS {
                let p_idx = (m * PROTO_DIM + seed_py) * PROTO_DIM + seed_px;
                seed_logit += inst.mask_coeffs[m] * self.protos[p_idx];
            }

            if sigmoid(seed_logit) < 0.45 {
                continue;
            }

            let is_better = match best {
                None => true,
                Some(prev) => {
                    inst.area < prev.area * 0.85
                        || (inst.area <= prev.area * 1.15 && inst.score > prev.score)
                }
            };

            if is_better {
                best = Some(inst);
            }
        }

        let winner = best?;

        // Reconstruct 160x160 mask for the selected instance only
        let mut proto_mask = vec![0.0f32; PROTO_DIM * PROTO_DIM];
        for py in 0..PROTO_DIM {
            for px in 0..PROTO_DIM {
                let mut sum = 0.0f32;
                for m in 0..NUM_MASKS {
                    let p_idx = (m * PROTO_DIM + py) * PROTO_DIM + px;
                    sum += winner.mask_coeffs[m] * self.protos[p_idx];
                }
                proto_mask[py * PROTO_DIM + px] = sigmoid(sum);
            }
        }

        let box_x0 = ((winner.x1 - self.pad_x) / self.scale)
            .floor()
            .clamp(0.0, (src_w - 1) as f32) as u32;
        let box_y0 = ((winner.y1 - self.pad_y) / self.scale)
            .floor()
            .clamp(0.0, (src_h - 1) as f32) as u32;
        let box_x1 = ((winner.x2 - self.pad_x) / self.scale)
            .ceil()
            .clamp(0.0, (src_w - 1) as f32) as u32;
        let box_y1 = ((winner.y2 - self.pad_y) / self.scale)
            .ceil()
            .clamp(0.0, (src_h - 1) as f32) as u32;

        if box_x1 <= box_x0 || box_y1 <= box_y0 {
            return None;
        }

        let sample_mask = |gx: u32, gy: u32| -> f32 {
            let lx = (gx as f32 * self.scale + self.pad_x) * proto_scale;
            let ly = (gy as f32 * self.scale + self.pad_y) * proto_scale;

            let x0 = lx.floor().clamp(0.0, (PROTO_DIM - 1) as f32) as usize;
            let y0 = ly.floor().clamp(0.0, (PROTO_DIM - 1) as f32) as usize;
            let x1 = (x0 + 1).min(PROTO_DIM - 1);
            let y1 = (y0 + 1).min(PROTO_DIM - 1);

            let fx = (lx - x0 as f32).clamp(0.0, 1.0);
            let fy = (ly - y0 as f32).clamp(0.0, 1.0);

            let v00 = proto_mask[y0 * PROTO_DIM + x0];
            let v10 = proto_mask[y0 * PROTO_DIM + x1];
            let v01 = proto_mask[y1 * PROTO_DIM + x0];
            let v11 = proto_mask[y1 * PROTO_DIM + x1];

            let top = v00 * (1.0 - fx) + v10 * fx;
            let bot = v01 * (1.0 - fx) + v11 * fx;
            top * (1.0 - fy) + bot * fy
        };

        let mut min_x = box_x1;
        let mut max_x = box_x0;
        let mut min_y = box_y1;
        let mut max_y = box_y0;
        let mut has_fg = false;

        for gy in box_y0..=box_y1 {
            for gx in box_x0..=box_x1 {
                if sample_mask(gx, gy) >= 0.45 {
                    min_x = min_x.min(gx);
                    max_x = max_x.max(gx);
                    min_y = min_y.min(gy);
                    max_y = max_y.max(gy);
                    has_fg = true;
                }
            }
        }

        if !has_fg || max_x <= min_x || max_y <= min_y {
            return None;
        }

        let crop_w = max_x - min_x + 1;
        let crop_h = max_y - min_y + 1;
        let mut sticker_img = RgbaImage::new(crop_w, crop_h);

        for y in 0..crop_h {
            let gy = min_y + y;
            for x in 0..crop_w {
                let gx = min_x + x;
                let prob = sample_mask(gx, gy);

                let alpha_f = ((prob - 0.38) / 0.22).clamp(0.0, 1.0);
                let alpha = (alpha_f * 255.0).round() as u8;

                if alpha > 0 {
                    let mut p = *proxy.get_pixel(gx, gy);
                    p[3] = alpha;
                    sticker_img.put_pixel(x, y, p);
                } else {
                    sticker_img.put_pixel(x, y, Rgba([0, 0, 0, 0]));
                }
            }
        }

        Some(ExtractedSticker {
            bounds_norm: (
                min_x as f32 / src_w as f32,
                min_y as f32 / src_h as f32,
                crop_w as f32 / src_w as f32,
                crop_h as f32 / src_h as f32,
            ),
            image: sticker_img,
        })
    }
}

/// Stateless fallback / unit-test helper
pub fn extract_sticker_from_seed(
    proxy: &RgbaImage,
    seed_u: f32,
    seed_v: f32,
    color_tolerance: u32,
    edge_stop_threshold: u8,
) -> Option<ExtractedSticker> {
    if let Some(cache) = YoloDetectionCache::build(proxy) {
        return cache.extract_at(proxy, seed_u, seed_v);
    }
    extract_fallback_region_grow(proxy, seed_u, seed_v, color_tolerance, edge_stop_threshold)
}

pub fn extract_fallback_region_grow(
    proxy: &RgbaImage,
    seed_u: f32,
    seed_v: f32,
    color_tolerance: u32,
    edge_stop_threshold: u8,
) -> Option<ExtractedSticker> {
    let (width, height) = proxy.dimensions();
    if width == 0 || height == 0 {
        return None;
    }
    let seed_x = (seed_u * width as f32).clamp(0.0, (width - 1) as f32) as u32;
    let seed_y = (seed_v * height as f32).clamp(0.0, (height - 1) as f32) as u32;

    let gray_img = image::imageops::colorops::grayscale(proxy);
    let gradients = sobel_gradient_map(&gray_img, |p| Luma([(p[0] as f32).min(255.0) as u8]));

    let seed_pixel = proxy.get_pixel(seed_x, seed_y);
    let mut visited = vec![false; (width * height) as usize];
    let mut mask = ImageBuffer::<Luma<u8>, Vec<u8>>::new(width, height);
    let mut queue = VecDeque::new();

    queue.push_back((seed_x, seed_y));
    visited[(seed_y * width + seed_x) as usize] = true;

    let mut min_x = seed_x;
    let mut max_x = seed_x;
    let mut min_y = seed_y;
    let mut max_y = seed_y;

    while let Some((cx, cy)) = queue.pop_front() {
        mask.put_pixel(cx, cy, Luma([255]));
        min_x = min_x.min(cx);
        max_x = max_x.max(cx);
        min_y = min_y.min(cy);
        max_y = max_y.max(cy);

        let neighbors = [
            (cx.wrapping_sub(1), cy),
            (cx + 1, cy),
            (cx, cy.wrapping_sub(1)),
            (cx, cy + 1),
        ];

        for (nx, ny) in neighbors {
            if nx < width && ny < height {
                let idx = (ny * width + nx) as usize;
                if !visited[idx] {
                    visited[idx] = true;
                    let grad = gradients.get_pixel(nx, ny)[0];
                    let pix = proxy.get_pixel(nx, ny);

                    let dr = (pix[0] as i32 - seed_pixel[0] as i32).abs() as u32;
                    let dg = (pix[1] as i32 - seed_pixel[1] as i32).abs() as u32;
                    let db = (pix[2] as i32 - seed_pixel[2] as i32).abs() as u32;
                    let color_dist = (dr * dr + dg * dg + db * db) / 3;

                    if grad < edge_stop_threshold
                        && color_dist < (color_tolerance * color_tolerance)
                    {
                        queue.push_back((nx, ny));
                    }
                }
            }
        }
    }

    if max_x <= min_x || max_y <= min_y {
        return None;
    }

    let crop_w = max_x - min_x + 1;
    let crop_h = max_y - min_y + 1;
    let mut sticker_img = RgbaImage::new(crop_w, crop_h);

    for y in 0..crop_h {
        for x in 0..crop_w {
            let src_x = min_x + x;
            let src_y = min_y + y;
            if mask.get_pixel(src_x, src_y)[0] == 255 {
                let mut p = *proxy.get_pixel(src_x, src_y);
                p[3] = 255;
                sticker_img.put_pixel(x, y, p);
            } else {
                sticker_img.put_pixel(x, y, Rgba([0, 0, 0, 0]));
            }
        }
    }

    Some(ExtractedSticker {
        bounds_norm: (
            min_x as f32 / width as f32,
            min_y as f32 / height as f32,
            crop_w as f32 / width as f32,
            crop_h as f32 / height as f32,
        ),
        image: sticker_img,
    })
}
