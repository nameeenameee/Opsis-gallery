# Opsis Pipeline Demo

## Tổng quan

Đây là một chương trình chỉnh sửa ảnh viết bằng Rust. Hiện tại chương trình
chạy bằng command line (Terminal), chưa có giao diện cửa sổ hoặc giao diện web.

Chương trình giữ nguyên ảnh gốc và lưu các chỉnh sửa dưới dạng một "edit
state". Khi cần xem preview hoặc xuất ảnh, pipeline sẽ dựng lại ảnh dựa trên
state đó.

## Hiện tại chương trình làm được gì?

- Đọc ảnh JPG/PNG.
- Cắt ảnh.
- Xoay ảnh quanh tâm.
- Chỉnh exposure, contrast, brightness, saturation, vibrance và temperature.
- Vẽ đường tự do lên ảnh.
- Thêm sticker hình chữ nhật màu.
- Tạo preview nhỏ hơn để xử lý nhanh.
- Lưu và tải lại trạng thái chỉnh sửa bằng JSON.
- Xuất ảnh đã chỉnh sửa thành một file mới mà không thay đổi ảnh gốc.
- Áp dụng các preset:
  - `natural_clean`
  - `vivid_landscape`
  - `warm_sunlight`
  - `classic_bw`

Phần xử lý pixel sử dụng nhiều luồng CPU thông qua thư viện `rayon`.

## Cài đặt và chạy

Cần cài Rust và Cargo trước. Từ thư mục dự án, chạy:

```powershell
cargo run --release -- --input ..\<imgname>.jpg --output cli_output.jpg
```

Có thể truyền đường dẫn ảnh khác:

```powershell
cargo run --release -- --input path\to\input.jpg --output path\to\result.jpg
```

Xem toàn bộ tùy chọn:

```powershell
cargo run --release -- --help
```

## Ví dụ sử dụng

### Chỉnh màu và xoay ảnh

```powershell
cargo run --release -- `
  --input ..\<imgname>.jpg `
  --output edited.jpg `
  --rotate 15 `
  --exposure 0.3 `
  --contrast 0.2 `
  --saturation 0.4
```

### Dùng preset và preview

```powershell
cargo run --release -- `
  --input ..\<imgname>.jpg `
  --output preview.jpg `
  --preview 1080 `
  --preset vivid_landscape
```

### Cắt ảnh

Giá trị crop có dạng `x,y,width,height`:

```powershell
cargo run --release -- `
  --input ..\<imgname>.jpg `
  --output cropped.jpg `
  --crop 100,50,1200,800
```

### Vẽ và thêm sticker

```powershell
cargo run --release -- `
  --input ..\<imgname>.jpg `
  --output annotated.jpg `
  --stroke 20,20:200,150 `
  --sticker 300,100,120,80,0,255,0,180
```

`--stroke` hiện tạo một nét màu đỏ. Sticker có dạng:
`x,y,width,height,r,g,b,a`.

## Session và lưu chỉnh sửa

### Lưu một session

```powershell
cargo run --release -- `
  --input ..\<imgname>.jpg `
  --output session_output.jpg `
  --preset warm_sunlight `
  --save-session test_session
```

Lệnh trên lưu:

```text
app_cache/test_session.json
app_cache/test_session_preview.jpg
```

File JSON chỉ chứa các thông số chỉnh sửa, không chứa toàn bộ ảnh.

### Tải lại session

```powershell
cargo run --release -- `
  --input ..\<imgname>.jpg `
  --output reloaded.jpg `
  --session test_session
```

Khi tải session, chương trình đọc state từ
`app_cache/<session-id>.json` rồi áp dụng lại lên ảnh đầu vào.

## Pipeline xử lý ảnh

Khi render một ảnh, chương trình thực hiện các bước sau:

1. Cắt ảnh nếu có crop.
2. Xoay ảnh quanh tâm.
3. Điều chỉnh ánh sáng và màu sắc.
4. Vẽ các nét tự do.
5. Đặt các sticker.
6. Xuất ảnh kết quả.

Lõi pipeline nằm trong [src/engine.rs](src/engine.rs).

## Cấu trúc các file chính

| File | Vai trò |
| --- | --- |
| `src/main.rs` | Chạy demo mặc định và điều phối workflow mẫu |
| `src/cli.rs` | Đọc và xử lý tham số command line |
| `src/engine.rs` | Crop, rotate, chỉnh màu, vẽ và render ảnh |
| `src/lifecycle.rs` | Save, load state, save as copy và revert |
| `src/state.rs` | Định nghĩa edit state, preset, stroke và sticker |
| `Cargo.toml` | Khai báo package và dependencies |
| `Intro to CS - Editing Stuffs.md` | Tài liệu chi tiết về ý tưởng và công thức xử lý ảnh |

## Thư mục đầu ra

- `app_cache/`: chứa JSON session và ảnh preview.
- `gallery_output/`: chứa các ảnh bản sao được xuất từ demo.
- `target/`: các file build do Cargo tạo ra.

Ảnh gốc không bị ghi đè trong quá trình chỉnh sửa.

## Kiểm thử

Chạy test bằng:

```powershell
cargo test
```

Các test hiện tại kiểm tra crop, render trung tính, alpha blending, vẽ đường,
xoay ảnh, sticker và serialize edit state. Trạng thái kiểm thử hiện tại:

```text
7 passed, 0 failed
```

## Những phần chưa có

- Chưa có giao diện đồ họa.
- Chưa có ứng dụng mobile hoặc web.
- Chưa có AI daemon thật sự; channel hiện chỉ mô phỏng việc gửi file cho
  AI indexer.
- Chưa có cơ sở dữ liệu thật; state hiện được lưu bằng file JSON.
- Chưa có undo/redo nhiều bước.
- Chưa có upload hoặc quản lý thư viện ảnh.
- Revert hiện xóa file JSON edit state; các preview và ảnh bản sao đã xuất
  vẫn còn trên đĩa.

## Công nghệ sử dụng

- Rust 2021
- [`image`](https://crates.io/crates/image): đọc, xử lý và ghi ảnh
- [`rayon`](https://crates.io/crates/rayon): xử lý song song
- [`serde`](https://crates.io/crates/serde) và
  [`serde_json`](https://crates.io/crates/serde_json): lưu edit state
- [`uuid`](https://crates.io/crates/uuid): tạo ID cho ảnh bản sao
