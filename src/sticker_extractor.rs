use image::{ImageBuffer, Luma, Rgba, RgbaImage};
use imageproc::gradients::sobel_gradient_map;
use std::collections::VecDeque;

#[derive(Debug, Clone)]
pub struct ExtractedSticker {
    pub bounds_norm: (f32, f32, f32, f32), // (x, y, w, h)
    pub image: RgbaImage,
}

/// Extracts a sticker around normalized (seed_u, seed_v) via gradient-bounded region growing.
pub fn extract_sticker_from_seed(
    proxy: &RgbaImage,
    seed_u: f32,
    seed_v: f32,
    color_tolerance: u32,    // e.g. 35..45
    edge_stop_threshold: u8, // e.g. 60..80
) -> Option<ExtractedSticker> {
    let (width, height) = proxy.dimensions();
    if width == 0 || height == 0 {
        return None;
    }
    let seed_x = (seed_u * width as f32).clamp(0.0, (width - 1) as f32) as u32;
    let seed_y = (seed_v * height as f32).clamp(0.0, (height - 1) as f32) as u32;

    let gray_img = image::imageops::colorops::grayscale(proxy);
    let gradients = sobel_gradient_map(&gray_img, |p| {
        let val = p[0] as f32;
        Luma([val.min(255.0) as u8])
    });

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

                    if grad < edge_stop_threshold && color_dist < (color_tolerance * color_tolerance) {
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