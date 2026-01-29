use serde::{Deserialize, Deserializer, Serialize};

/// A dimension value that can be a number (pixels) or a string (percentage, auto, etc.)
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum DimensionValue {
    Pixels(f64),
    Percentage(f64),  // 0.0 to 1.0
    Auto,
}

impl Default for DimensionValue {
    fn default() -> Self {
        DimensionValue::Auto
    }
}

impl<'de> Deserialize<'de> for DimensionValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        use serde::de::{self, Visitor};
        
        struct DimensionVisitor;
        
        impl<'de> Visitor<'de> for DimensionVisitor {
            type Value = DimensionValue;
            
            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a number or a string like '100%' or 'auto'")
            }
            
            fn visit_f64<E>(self, v: f64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(DimensionValue::Pixels(v))
            }
            
            fn visit_i64<E>(self, v: i64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(DimensionValue::Pixels(v as f64))
            }
            
            fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(DimensionValue::Pixels(v as f64))
            }
            
            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                if v == "auto" {
                    Ok(DimensionValue::Auto)
                } else if v.ends_with('%') {
                    let num_str = v.trim_end_matches('%');
                    match num_str.parse::<f64>() {
                        Ok(n) => Ok(DimensionValue::Percentage(n / 100.0)),
                        Err(_) => Err(de::Error::custom(format!("invalid percentage: {}", v))),
                    }
                } else {
                    // Try to parse as a number
                    match v.parse::<f64>() {
                        Ok(n) => Ok(DimensionValue::Pixels(n)),
                        Err(_) => Err(de::Error::custom(format!("invalid dimension: {}", v))),
                    }
                }
            }
        }
        
        deserializer.deserialize_any(DimensionVisitor)
    }
}

/// Style description that can be serialized from JS
/// Note: This is only used for JSON deserialization, not direct napi binding
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StyleDesc {
    // Display
    pub display: Option<String>,
    pub visibility: Option<String>,

    // Flexbox
    pub flex_direction: Option<String>,
    pub flex_wrap: Option<String>,
    pub flex_grow: Option<f64>,
    pub flex_shrink: Option<f64>,
    pub flex_basis: Option<f64>,
    pub align_items: Option<String>,
    pub align_self: Option<String>,
    pub align_content: Option<String>,
    pub justify_content: Option<String>,
    pub gap: Option<f64>,
    pub row_gap: Option<f64>,
    pub column_gap: Option<f64>,

    // Sizing - now supports both numbers and strings like "100%" or "auto"
    pub width: Option<DimensionValue>,
    pub height: Option<DimensionValue>,
    pub min_width: Option<DimensionValue>,
    pub min_height: Option<DimensionValue>,
    pub max_width: Option<DimensionValue>,
    pub max_height: Option<DimensionValue>,

    // Spacing (padding)
    pub padding: Option<f64>,
    pub padding_top: Option<f64>,
    pub padding_right: Option<f64>,
    pub padding_bottom: Option<f64>,
    pub padding_left: Option<f64>,

    // Spacing (margin)
    pub margin: Option<f64>,
    pub margin_top: Option<f64>,
    pub margin_right: Option<f64>,
    pub margin_bottom: Option<f64>,
    pub margin_left: Option<f64>,

    // Position
    pub position: Option<String>,
    pub top: Option<f64>,
    pub right: Option<f64>,
    pub bottom: Option<f64>,
    pub left: Option<f64>,

    // Background & Colors
    pub background: Option<String>,
    pub background_color: Option<String>,
    pub color: Option<String>,
    pub opacity: Option<f64>,

    // Border
    pub border_width: Option<f64>,
    pub border_color: Option<String>,
    pub border_radius: Option<f64>,
    pub border_top_left_radius: Option<f64>,
    pub border_top_right_radius: Option<f64>,
    pub border_bottom_left_radius: Option<f64>,
    pub border_bottom_right_radius: Option<f64>,

    // Text
    pub font_size: Option<f64>,
    pub font_weight: Option<String>,
    pub text_align: Option<String>,
    pub line_height: Option<f64>,

    // Overflow
    pub overflow: Option<String>,
    pub overflow_x: Option<String>,
    pub overflow_y: Option<String>,

    // Cursor
    pub cursor: Option<String>,
}

/// Parse a color string (hex, rgb, etc.) to GPUI Hsla
pub fn parse_color(color: &str) -> Option<(f32, f32, f32, f32)> {
    let color = color.trim();

    // Handle hex colors
    if color.starts_with('#') {
        let hex = &color[1..];
        match hex.len() {
            3 => {
                // #RGB -> #RRGGBB
                let r = u8::from_str_radix(&hex[0..1].repeat(2), 16).ok()? as f32 / 255.0;
                let g = u8::from_str_radix(&hex[1..2].repeat(2), 16).ok()? as f32 / 255.0;
                let b = u8::from_str_radix(&hex[2..3].repeat(2), 16).ok()? as f32 / 255.0;
                return Some((r, g, b, 1.0));
            }
            6 => {
                let r = u8::from_str_radix(&hex[0..2], 16).ok()? as f32 / 255.0;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()? as f32 / 255.0;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()? as f32 / 255.0;
                return Some((r, g, b, 1.0));
            }
            8 => {
                let r = u8::from_str_radix(&hex[0..2], 16).ok()? as f32 / 255.0;
                let g = u8::from_str_radix(&hex[2..4], 16).ok()? as f32 / 255.0;
                let b = u8::from_str_radix(&hex[4..6], 16).ok()? as f32 / 255.0;
                let a = u8::from_str_radix(&hex[6..8], 16).ok()? as f32 / 255.0;
                return Some((r, g, b, a));
            }
            _ => return None,
        }
    }

    // Handle rgb/rgba
    if color.starts_with("rgb") {
        let inner = color
            .trim_start_matches("rgba(")
            .trim_start_matches("rgb(")
            .trim_end_matches(')');
        let parts: Vec<&str> = inner.split(',').map(|s| s.trim()).collect();

        if parts.len() >= 3 {
            let r = parts[0].parse::<f32>().ok()? / 255.0;
            let g = parts[1].parse::<f32>().ok()? / 255.0;
            let b = parts[2].parse::<f32>().ok()? / 255.0;
            let a = if parts.len() == 4 {
                parts[3].parse::<f32>().ok()?
            } else {
                1.0
            };
            return Some((r, g, b, a));
        }
    }

    None
}

/// Convert RGBA floats (0.0-1.0) to a hex u32 for GPUI's rgba() function
/// Format: 0xRRGGBBAA
pub fn rgba_to_hex(r: f32, g: f32, b: f32, a: f32) -> u32 {
    let r = (r.clamp(0.0, 1.0) * 255.0) as u32;
    let g = (g.clamp(0.0, 1.0) * 255.0) as u32;
    let b = (b.clamp(0.0, 1.0) * 255.0) as u32;
    let a = (a.clamp(0.0, 1.0) * 255.0) as u32;
    (r << 24) | (g << 16) | (b << 8) | a
}

/// Parse a color string and return a hex u32 for GPUI
pub fn parse_color_hex(color: &str) -> Option<u32> {
    let (r, g, b, a) = parse_color(color)?;
    Some(rgba_to_hex(r, g, b, a))
}
