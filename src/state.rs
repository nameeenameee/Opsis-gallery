use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Default)]
pub struct CropArea {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Default)]
pub struct NormalizedCrop {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl From<CropArea> for NormalizedCrop {
    fn from(c: CropArea) -> Self {
        Self {
            x: c.x as f32,
            y: c.y as f32,
            width: c.width as f32,
            height: c.height as f32,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FreehandStroke {
    pub points: Vec<(i32, i32)>,
    pub color: [u8; 4],
    pub thickness: u32,
}

#[allow(dead_code)]
pub type LineStroke = FreehandStroke;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Sticker {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub color: [u8; 4],
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Default)]
pub enum Preset {
    #[default]
    None,
    Sepia,
    Vintage,
    Monochrome,
}

impl Preset {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "none" => Some(Self::None),
            "sepia" => Some(Self::Sepia),
            "vintage" => Some(Self::Vintage),
            "monochrome" | "mono" | "bw" => Some(Self::Monochrome),
            _ => None,
        }
    }

    pub fn apply(&self, state: &mut EditState) {
        state.preset = *self;
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EditState {
    #[serde(default)]
    pub image_id: String,

    #[serde(default)]
    pub crop: Option<CropArea>,

    #[serde(default)]
    pub exposure: f32,

    #[serde(default = "default_temperature")]
    pub temperature: f32,

    #[serde(default)]
    pub brightness: f32,

    #[serde(default = "default_contrast")]
    pub contrast: f32,

    #[serde(default = "default_saturation")]
    pub saturation: f32,

    #[serde(default)]
    pub vibrance: f32,

    #[serde(default)]
    pub rotation_degrees: f32,

    #[serde(default)]
    pub preset: Preset,

    #[serde(default)]
    pub strokes: Vec<FreehandStroke>,

    #[serde(default)]
    pub stickers: Vec<Sticker>,
}

fn default_temperature() -> f32 {
    6500.0
}

fn default_contrast() -> f32 {
    1.0
}

fn default_saturation() -> f32 {
    1.0
}

impl Default for EditState {
    fn default() -> Self {
        Self {
            image_id: String::new(),
            crop: None,
            exposure: 0.0,
            temperature: 6500.0,
            brightness: 0.0,
            contrast: 1.0,
            saturation: 1.0,
            vibrance: 0.0,
            rotation_degrees: 0.0,
            preset: Preset::None,
            strokes: Vec::new(),
            stickers: Vec::new(),
        }
    }
}
