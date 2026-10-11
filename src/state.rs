use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct CropArea {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FreehandStroke {
    pub points: Vec<(i32, i32)>,
    pub color: [u8; 4],
    pub thickness: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sticker {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub color: [u8; 4],
}

/// Trạng thái chỉnh sửa lưu vào DB hoặc file JSON (chỉ nặng vài chục bytes)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditState {
    pub image_id: String,
    pub crop: Option<CropArea>,
    #[serde(default)]
    pub rotation_degrees: f32,
    pub exposure: f32, // -2.0 đến +2.0 (EV stops)
    pub contrast: f32, // -1.0 đến +1.0
    #[serde(default)]
    pub brightness: f32, // -1.0 đến +1.0
    pub saturation: f32, // -1.0 đến +1.0
    #[serde(default)]
    pub vibrance: f32, // -1.0 đến +1.0
    pub temperature: f32, // -1.0 (Lạnh/Xanh) đến +1.0 (Ấm/Vàng)
    #[serde(default)]
    pub strokes: Vec<FreehandStroke>,
    #[serde(default)]
    pub stickers: Vec<Sticker>,
}

#[derive(Debug, Clone, Copy)]
pub enum Preset {
    NaturalClean,
    VividLandscape,
    WarmSunlight,
    ClassicBw,
}

impl Preset {
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "natural_clean" => Some(Self::NaturalClean),
            "vivid_landscape" => Some(Self::VividLandscape),
            "warm_sunlight" => Some(Self::WarmSunlight),
            "classic_bw" => Some(Self::ClassicBw),
            _ => None,
        }
    }

    pub fn apply(self, state: &mut EditState) {
        let values = match self {
            Self::NaturalClean => (0.1, 0.1, 0.05, 0.1, 0.0),
            Self::VividLandscape => (0.2, 0.15, 0.25, 0.35, 0.0),
            Self::WarmSunlight => (0.2, -0.05, -0.05, 0.15, 0.7),
            Self::ClassicBw => (0.0, 0.25, 0.0, -1.0, 0.0),
        };
        state.exposure = values.0;
        state.contrast = values.1;
        state.brightness = values.2;
        state.saturation = values.3;
        state.temperature = values.4;
    }
}

impl Default for EditState {
    fn default() -> Self {
        Self {
            image_id: String::from("default_img"),
            crop: None,
            rotation_degrees: 0.0,
            exposure: 0.0,
            contrast: 0.0,
            brightness: 0.0,
            saturation: 0.0,
            vibrance: 0.0,
            temperature: 0.0,
            strokes: Vec::new(),
            stickers: Vec::new(),
        }
    }
}
