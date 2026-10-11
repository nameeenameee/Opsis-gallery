use crate::engine::PipelineEngine;
use crate::lifecycle::ImageLifecycleManager;
use crate::state::{CropArea, EditState, FreehandStroke, Preset, Sticker};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

struct Options {
    input: PathBuf,
    output: PathBuf,
    preview_edge: Option<u32>,
    session_id: Option<String>,
    save_session: Option<String>,
    state: EditState,
}

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let arguments: Vec<String> = env::args().skip(1).collect();
    if arguments
        .iter()
        .any(|argument| argument == "--help" || argument == "-h")
    {
        println!("{}", usage());
        return Ok(());
    }
    let options = parse_options(arguments)?;
    let original = image::open(&options.input)
        .map_err(|error| format!("could not open '{}': {error}", options.input.display()))?
        .to_rgba8();
    let source = match options.preview_edge {
        Some(max_edge) => PipelineEngine::create_preview_buffer(&original, max_edge),
        None => original.clone(),
    };
    let mut state = options.state;
    if let Some(session_id) = &options.session_id {
        if let Some(saved) = ImageLifecycleManager::load_state(Path::new("app_cache"), session_id)?
        {
            state = saved;
            state.image_id = session_id.clone();
        }
    }
    let rendered = PipelineEngine::render(&source, &state);

    if let Some(parent) = options
        .output
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    image::DynamicImage::ImageRgba8(rendered)
        .into_rgb8()
        .save(&options.output)?;
    if let Some(session_id) = options.save_session {
        state.image_id = session_id;
        ImageLifecycleManager::handle_save(Path::new("app_cache"), &original, &state)?;
    }

    println!("Input:  {}", options.input.display());
    println!("Output: {}", options.output.display());
    println!(
        "Edit: rotation={}°, exposure={}, contrast={}, brightness={}, saturation={}, vibrance={}, temperature={}",
        state.rotation_degrees,
        state.exposure,
        state.contrast,
        state.brightness,
        state.saturation,
        state.vibrance,
        state.temperature
    );
    Ok(())
}

fn parse_options<I>(args: I) -> Result<Options, String>
where
    I: IntoIterator<Item = String>,
{
    let mut input = None;
    let mut output = PathBuf::from("cli_output.jpg");
    let mut preview_edge = None;
    let mut session_id = None;
    let mut save_session = None;
    let mut state = EditState::default();
    let mut args = args.into_iter();

    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--input" | "-i" => input = Some(PathBuf::from(next_value(&mut args, &argument)?)),
            "--output" | "-o" => output = PathBuf::from(next_value(&mut args, &argument)?),
            "--preview" => {
                preview_edge = Some(parse_value(&mut args, &argument)?);
            }
            "--rotate" => state.rotation_degrees = parse_value(&mut args, &argument)?,
            "--exposure" => state.exposure = parse_value(&mut args, &argument)?,
            "--contrast" => state.contrast = parse_value(&mut args, &argument)?,
            "--brightness" => state.brightness = parse_value(&mut args, &argument)?,
            "--saturation" => state.saturation = parse_value(&mut args, &argument)?,
            "--vibrance" => state.vibrance = parse_value(&mut args, &argument)?,
            "--temperature" => state.temperature = parse_value(&mut args, &argument)?,
            "--crop" => state.crop = Some(parse_crop(&next_value(&mut args, &argument)?)?),
            "--session" => session_id = Some(next_value(&mut args, &argument)?),
            "--save-session" => save_session = Some(next_value(&mut args, &argument)?),
            "--preset" => {
                let name = next_value(&mut args, &argument)?;
                let preset =
                    Preset::parse(&name).ok_or_else(|| format!("unknown preset '{name}'"))?;
                preset.apply(&mut state);
            }
            "--stroke" => state
                .strokes
                .push(parse_stroke(&next_value(&mut args, &argument)?)?),
            "--sticker" => state
                .stickers
                .push(parse_sticker(&next_value(&mut args, &argument)?)?),
            positional if input.is_none() && !positional.starts_with('-') => {
                input = Some(PathBuf::from(positional))
            }
            unknown => return Err(format!("unknown argument '{unknown}'\n\n{}", usage())),
        }
    }

    let input = input.ok_or_else(|| format!("missing --input <path>\n\n{}", usage()))?;
    state.image_id = input
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("cli_image")
        .to_owned();
    Ok(Options {
        input,
        output,
        preview_edge,
        session_id,
        save_session,
        state,
    })
}

fn next_value(args: &mut impl Iterator<Item = String>, option: &str) -> Result<String, String> {
    args.next()
        .ok_or_else(|| format!("missing value for {option}\n\n{}", usage()))
}

fn parse_value<T: std::str::FromStr>(
    args: &mut impl Iterator<Item = String>,
    option: &str,
) -> Result<T, String> {
    let value = next_value(args, option)?;
    value
        .parse()
        .map_err(|_| format!("invalid value '{value}' for {option}"))
}

fn parse_crop(value: &str) -> Result<CropArea, String> {
    let values: Vec<u32> = value
        .split(',')
        .map(|part| part.parse().map_err(|_| format!("invalid crop '{value}'")))
        .collect::<Result<_, _>>()?;
    match values.as_slice() {
        [x, y, width, height] => Ok(CropArea {
            x: *x,
            y: *y,
            width: *width,
            height: *height,
        }),
        _ => Err(format!("crop must be x,y,width,height, got '{value}'")),
    }
}

fn parse_stroke(value: &str) -> Result<FreehandStroke, String> {
    let mut points = Vec::new();
    for point in value.split(':') {
        let coordinates: Vec<i32> = point
            .split(',')
            .map(|part| {
                part.parse()
                    .map_err(|_| format!("invalid stroke '{value}'"))
            })
            .collect::<Result<_, _>>()?;
        match coordinates.as_slice() {
            [x, y] => points.push((*x, *y)),
            _ => return Err(format!("stroke points must be x,y:x,y, got '{value}'")),
        }
    }
    if points.len() < 2 {
        return Err(format!("stroke needs at least two points, got '{value}'"));
    }
    Ok(FreehandStroke {
        points,
        color: [255, 0, 0, 255],
        thickness: 5,
    })
}

fn parse_sticker(value: &str) -> Result<Sticker, String> {
    let values: Vec<i32> = value
        .split(',')
        .map(|part| {
            part.parse()
                .map_err(|_| format!("invalid sticker '{value}'"))
        })
        .collect::<Result<_, _>>()?;
    match values.as_slice() {
        [x, y, width, height, r, g, b, a] if *width > 0 && *height > 0 => Ok(Sticker {
            x: *x,
            y: *y,
            width: *width as u32,
            height: *height as u32,
            color: [*r as u8, *g as u8, *b as u8, *a as u8],
        }),
        _ => Err(format!(
            "sticker must be x,y,width,height,r,g,b,a with positive dimensions, got '{value}'"
        )),
    }
}

fn usage() -> String {
    String::from(
        "Usage: cargo run -- --input <path> [options]\n\
         Options:\n\
           -i, --input <path>       Input image (also accepts a positional path)\n\
           -o, --output <path>      Output image (default: cli_output.jpg)\n\
               --preview <pixels>  Resize the input before rendering\n\
               --crop x,y,w,h       Crop rectangle\n\
                   --session <id>    Load saved edit state from app_cache/<id>.json\n\
                   --save-session <id> Save edit state to app_cache/<id>.json\n\
                   --preset <name>   natural_clean, vivid_landscape, warm_sunlight, classic_bw\n\
                   --stroke x,y:x,y  Add a persistent red freehand stroke\n\
                   --sticker x,y,w,h,r,g,b,a  Add a persistent color sticker\n\
               --rotate <degrees>  Rotation angle\n\
               --exposure <value>  Exposure in EV\n\
               --contrast <value>  Contrast (-1 to 1)\n\
               --brightness <value> Brightness (-1 to 1)\n\
               --saturation <value> Saturation (-1 to 1)\n\
               --vibrance <value>  Vibrance (-1 to 1)\n\
               --temperature <value> Temperature (-1 to 1)\n\
           -h, --help              Show this help",
    )
}
