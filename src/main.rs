mod cli;
mod engine;
mod lifecycle;
mod state;

use engine::PipelineEngine;
use lifecycle::ImageLifecycleManager;
use state::{CropArea, EditState};
use std::env;
use std::fs;
use std::path::Path;
use std::sync::mpsc::channel;
use std::time::Instant;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if env::args().nth(1).is_some() {
        return cli::run();
    }
    run_demo()
}

fn run_demo() -> Result<(), Box<dyn std::error::Error>> {
    let cache_dir = Path::new("./app_cache");
    let gallery_dir = Path::new("./gallery_output");
    fs::create_dir_all(cache_dir)?;
    fs::create_dir_all(gallery_dir)?;

    println!("=== OPSIS GALLERY: TEST VỚI ẢNH THẬT ===");

    // 1. Nạp ảnh từ tham số dòng lệnh hoặc dùng ảnh demo có sẵn
    let (original_img, input_file) = load_input_image()?;
    println!("\n[1] Đang đọc file '{}'...", input_file);
    println!(
        "    Độ phân giải ảnh gốc: {} x {} px",
        original_img.width(),
        original_img.height()
    );

    // 2. Thiết lập các thông số chỉnh sửa (chỉnh rõ để dễ thấy khác biệt)
    let mut edit_state = EditState::default();
    edit_state.image_id = String::from("real_photo_test");
    //edit_state.exposure = 0.6; // Tăng sáng (+0.6 EV)
    //edit_state.contrast = 0.3; // Tăng tương phản
    //edit_state.saturation = 0.5; // Tăng độ rực màu
    //edit_state.temperature = 0.4; // Kéo sang tông ấm (ngả vàng nắng)
    //edit_state.vibrance = 0.3; // Tăng rực màu thông minh
    edit_state.crop = Some(center_crop(&original_img));
    edit_state.rotation_degrees = 45.0;

    // 3. Chạy toàn bộ các stage của pipeline trên một bản preview
    let preview = PipelineEngine::create_preview_buffer(&original_img, 1080);
    let start = Instant::now();
    let rendered_preview = run_feature_pipeline(&preview, &edit_state);
    let duration = start.elapsed();
    let pipeline_preview_path = cache_dir.join("real_photo_test_pipeline.jpg");
    image::DynamicImage::ImageRgba8(rendered_preview)
        .into_rgb8()
        .save(&pipeline_preview_path)?;
    println!(
        "[2] Toàn bộ pipeline preview hoàn tất trong: {:.2?}",
        duration
    );
    println!(
        "    Pipeline preview đã lưu ở: {}",
        pipeline_preview_path.display()
    );

    // 4. Test ED-03: Lưu JSON State và ảnh preview vào app_cache
    ImageLifecycleManager::handle_save(cache_dir, &original_img, &edit_state)?;

    // 5. Test ED-04: Xuất ảnh đã chỉnh sửa full-res ra thư mục gallery_output
    let (ai_tx, ai_rx) = channel::<String>();
    let exported_path = ImageLifecycleManager::handle_save_as_copy(
        gallery_dir,
        &original_img,
        &edit_state,
        &ai_tx,
    )?;

    if let Ok(msg) = ai_rx.recv() {
        println!("    [Queue Event] Đã bắn file sang AI daemon: {}", msg);
    }

    println!("\n=== HOÀN TẤT ===");
    println!("-> Ảnh preview đã lưu ở: app_cache/real_photo_test_preview.jpg");
    println!("-> Ảnh full-res đã lưu ở: {:?}", exported_path);

    ImageLifecycleManager::handle_revert(cache_dir, &edit_state.image_id)?;
    Ok(())
}

fn center_crop(image: &image::RgbaImage) -> CropArea {
    let width = (image.width() * 4 / 5).max(1);
    let height = (image.height() * 4 / 5).max(1);
    CropArea {
        x: (image.width() - width) / 2,
        y: (image.height() - height) / 2,
        width,
        height,
    }
}

fn run_feature_pipeline(preview: &image::RgbaImage, state: &EditState) -> image::RgbaImage {
    println!("    [Pipeline] 1/6 Crop ROI");
    println!(
        "    [Pipeline] 2/3 Rotate {} degrees",
        state.rotation_degrees
    );
    println!("    [Pipeline] 3/3 Process pixels");
    PipelineEngine::render(preview, state)
}

fn load_input_image() -> Result<(image::RgbaImage, String), Box<dyn std::error::Error>> {
    let requested_path = env::args().nth(1);
    let candidates = ["Elaina3.jpg"];

    if let Some(path) = requested_path {
        let image = image::open(&path).map_err(|error| {
            format!(
                "Không thể đọc ảnh '{}'. Kiểm tra đường dẫn hoặc truyền file khác sau `cargo run --release --`: {}",
                path, error
            )
        })?;
        return Ok((image.to_rgba8(), path));
    }

    for path in candidates {
        if Path::new(path).is_file() {
            return Ok((image::open(path)?.to_rgba8(), path.to_owned()));
        }
    }

    println!("    Không tìm thấy ảnh đầu vào, tạo bảng màu demo để kiểm tra pipeline.");
    let mut image = image::RgbaImage::new(1920, 1080);
    for (x, y, pixel) in image.enumerate_pixels_mut() {
        let column = x / 480;
        let row = y / 540;
        let color = match (row, column) {
            (0, 0) => [220, 50, 50, 255],
            (0, 1) => [50, 200, 80, 255],
            (0, 2) => [50, 100, 230, 255],
            (0, _) => [240, 210, 40, 255],
            (1, 0) => [230, 80, 190, 255],
            (1, 1) => [40, 210, 210, 255],
            (1, 2) => [240, 130, 40, 255],
            _ => [220, 220, 220, 255],
        };
        *pixel = image::Rgba(color);
    }
    Ok((image, String::from("<generated-demo>")))
}
