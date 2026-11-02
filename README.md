# Opsis Pipeline Demo

## Overview

Opsis is an image-editing pipeline written in Rust and currently used through
the command line. The original image is always preserved; edits are stored in
`EditState` and reapplied when creating a preview or exporting an image.

## Features

- Read JPG and PNG images.
- Crop, rotate, and adjust exposure, contrast, brightness, saturation,
  vibrance, and temperature.
- Draw freehand strokes and add colored rectangular stickers.
- Create previews with a maximum edge size.
- Save and load edit state as JSON.
- Export an edited copy without overwriting the original image.
- Presets: `natural_clean`, `vivid_landscape`, `warm_sunlight`, and
  `classic_bw`.
- Process pixels in parallel with `rayon`.

## Installation and usage

Install Rust and Cargo first. From the project directory, run
`cargo run --release -- --input <input.jpg> --output <output.jpg>`.
You can replace the input and output paths as needed. Run
`cargo run --release -- --help` to see all available options.

A complete command can combine multiple edits, for example:
`cargo run --release -- --input photo.jpg --output edited.jpg --rotate 15
--exposure 0.3 --contrast 0.2 --saturation 0.4`.

Common options:

- `--preview <pixels>`: resize the image before rendering.
- `--preset <name>`: apply an available preset.
- `--crop x,y,width,height`: crop the image.
- `--stroke x,y:x,y`: add a red freehand stroke.
- `--sticker x,y,width,height,r,g,b,a`: add an RGBA sticker.
- `--session <id>`: load state from `app_cache/<id>.json`.
- `--save-session <id>`: save state and a preview to `app_cache/`.

## Sessions

To save a session, add `--save-session test_session` to the render command.
Then use `--session test_session` to load the state onto an input image.
The JSON file contains only edit settings, not the complete image.

## Pipeline

When rendering, the pipeline performs these steps in order:

1. Crop if configured.
2. Rotate the image around its center.
3. Adjust lighting and color.
4. Draw freehand strokes.
5. Place stickers.
6. Export the result.

The processing core is in [src/engine.rs](src/engine.rs). Main modules:

| File | Purpose |
| --- | --- |
| [src/main.rs](src/main.rs) | Run the default demo and sample workflow |
| [src/cli.rs](src/cli.rs) | Parse command-line arguments |
| [src/engine.rs](src/engine.rs) | Crop, rotate, adjust color, and render images |
| [src/lifecycle.rs](src/lifecycle.rs) | Save, load, export copies, and revert state |
| [src/state.rs](src/state.rs) | Define edit state, presets, strokes, and stickers |
| [Cargo.toml](Cargo.toml) | Package and dependencies |

## Generated directories

The following paths are created when running the demo or tests and should not
be committed:

- `app_cache/`: session JSON files and previews.
- `gallery_output/`: exported image copies.
- `target/`: Cargo build and test output.
- `cli_output.jpg`: the CLI's default output.

These paths are listed in `.gitignore`. They can be safely removed and
regenerated when needed.

## Testing

Run `cargo test` to test cropping, rendering, alpha blending, freehand
drawing, rotation, stickers, and edit-state serialization.

## Current limitations

- No graphical, mobile, or web interface.
- No real AI daemon; the channel currently simulates sending a file to an AI
  indexer.
- No real database; state is stored in JSON files.
- No multi-step undo/redo, upload, or image-library management.
- Revert removes the edit-state JSON file but keeps previews and exported
  images.

## Technologies

- Rust 2021
- [`image`](https://crates.io/crates/image): read, process, and write images
- [`rayon`](https://crates.io/crates/rayon): parallel processing
- [`serde`](https://crates.io/crates/serde) and
  [`serde_json`](https://crates.io/crates/serde_json): store edit state
- [`uuid`](https://crates.io/crates/uuid): generate IDs for image copies
