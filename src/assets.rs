//! Original resources embedded in the executable: independent of the working directory.
//! The manifest owns stable keys and provenance; unmapped resources remain procedural.
use serde_json::Value;
use std::sync::LazyLock;

pub struct EmbeddedResource {
    pub key: &'static str,
    pub bytes: &'static [u8],
}
include!(concat!(env!("OUT_DIR"), "/assets.rs"));
pub const MANIFEST: &str = include_str!("../assets/manifest.json");
pub static CATALOGUE: LazyLock<Value> =
    LazyLock::new(|| serde_json::from_str(MANIFEST).expect("invalid embedded assets/manifest.json"));
pub fn resource(key: &str) -> Option<&'static [u8]> {
    RESOURCES.iter().find(|r| r.key == key).map(|r| r.bytes)
}

pub fn validate() -> Result<(), String> {
    if CATALOGUE["schema"] != 1 {
        return Err("assets/manifest.json: unsupported schema".into());
    }
    for entry in CATALOGUE["resources"].as_array().ok_or("assets/manifest.json: resources must be an array")? {
        let key = entry["key"].as_str().ok_or("asset has no stable key")?;
        let bytes =
            resource(key).ok_or_else(|| format!("asset {key}: missing embedded resource; rebuild the executable"))?;
        match entry["kind"].as_str() {
            Some("texture") => {
                if bytes.get(..8) != Some(b"\x89PNG\r\n\x1a\n")
                    || bytes.get(16..24) != Some(&[0, 0, 0, 16, 0, 0, 0, 16])
                {
                    return Err(format!("asset {key}: expected a 16 x 16 PNG"));
                }
            }
            Some("model") => {
                model_boxes(key)?;
            }
            Some("audio") => {
                audio_samples(key)?;
            }
            other => return Err(format!("asset {key}: unsupported kind {other:?}")),
        }
    }
    Ok(())
}

/// Original box-model format, in sixteenths; references resolve by asset key.
pub fn model_boxes(key: &str) -> Result<Vec<crate::world::shape::Box16>, String> {
    let model: Value = serde_json::from_slice(resource(key).ok_or_else(|| format!("model {key}: missing resource"))?)
        .map_err(|e| format!("model {key}: {e}"))?;
    if model["schema"] != 1 {
        return Err(format!("model {key}: unsupported schema"));
    }
    let texture = model["texture"].as_str().ok_or_else(|| format!("model {key}: missing texture reference"))?;
    if resource(texture).is_none() {
        return Err(format!("model {key}: missing texture {texture}"));
    }
    let boxes = model["boxes"].as_array().ok_or_else(|| format!("model {key}: missing boxes"))?;
    if boxes.is_empty() || boxes.len() > crate::world::shape::Boxes::CAPACITY {
        return Err(format!("model {key}: too many or no model boxes"));
    }
    boxes
        .iter()
        .map(|b| {
            let coordinates = |part: &str| -> Result<[u8; 3], String> {
                let a = b[part]
                    .as_array()
                    .filter(|a| a.len() == 3)
                    .ok_or_else(|| format!("model {key}: {part} must have three coordinates"))?;
                let mut out = [0; 3];
                for (o, v) in out.iter_mut().zip(a) {
                    *o = v
                        .as_u64()
                        .filter(|&n| n <= 16)
                        .ok_or_else(|| format!("model {key}: coordinate outside 0..16"))?
                        as u8;
                }
                Ok(out)
            };
            let (min, max) = (coordinates("min")?, coordinates("max")?);
            if min.iter().zip(max).any(|(&a, b)| a >= b) {
                return Err(format!("model {key}: empty or reversed box"));
            }
            Ok(crate::world::shape::Box16 { min, max })
        })
        .collect()
}

/// PCM16 mono WAV, 48 kHz. Decode once when the audio bank loads, never during mixing.
pub fn audio_samples(key: &str) -> Result<Vec<f32>, String> {
    let bytes = resource(key).ok_or_else(|| format!("audio {key}: missing resource"))?;
    let bad = || format!("audio {key}: expected mono PCM16 WAV at 48000 Hz");
    if bytes.get(..4) != Some(b"RIFF") || bytes.get(8..12) != Some(b"WAVE") {
        return Err(bad());
    }
    let mut at = 12;
    let mut format = false;
    let mut data = None;
    while let Some(header) = bytes.get(at..at + 8) {
        let size = u32::from_le_bytes(header[4..8].try_into().unwrap()) as usize;
        let chunk = bytes.get(at + 8..at + 8 + size).ok_or_else(bad)?;
        match &header[..4] {
            b"fmt " => {
                if chunk.len() < 16
                    || chunk[..4] != [1, 0, 1, 0]
                    || chunk[4..8] != 48000u32.to_le_bytes()
                    || chunk[14..16] != [16, 0]
                {
                    return Err(bad());
                }
                format = true;
            }
            b"data" => data = Some(chunk),
            _ => {}
        }
        at += 8 + size + (size % 2);
    }
    let data = data.filter(|d| format && !d.is_empty() && d.len() % 2 == 0).ok_or_else(bad)?;
    Ok(data.as_chunks::<2>().0.iter().map(|s| f32::from(i16::from_le_bytes(*s)) / 32768.0).collect())
}

#[cfg(feature = "client")]
static TEXTURES: LazyLock<rustc_hash::FxHashMap<u16, Vec<u8>>> = LazyLock::new(|| {
    validate().expect("embedded asset validation failed; repair assets/manifest.json and rebuild");
    CATALOGUE["resources"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["kind"] == "texture")
        .map(|entry| {
            let key = entry["key"].as_str().unwrap();
            let layer =
                entry["texture_layer"].as_u64().filter(|&l| l < 2048).expect("asset texture layer must fit the atlas")
                    as u16;
            let mut reader = png::Decoder::new(std::io::Cursor::new(resource(key).unwrap()))
                .read_info()
                .unwrap_or_else(|e| panic!("asset {key}: PNG decode failed: {e}"));
            let mut pixels = vec![0; reader.output_buffer_size().expect("bounded asset size")];
            let info = reader.next_frame(&mut pixels).unwrap_or_else(|e| panic!("asset {key}: PNG data failed: {e}"));
            assert_eq!(
                (info.width, info.height, info.color_type, info.bit_depth),
                (16, 16, png::ColorType::Rgba, png::BitDepth::Eight),
                "asset {key}: expected RGBA8 16 x 16"
            );
            pixels.truncate(info.buffer_size());
            (layer, pixels)
        })
        .collect()
});
#[cfg(feature = "client")]
pub fn texture_pixel(layer: u16, x: usize, y: usize) -> Option<[u8; 4]> {
    if x >= 16 || y >= 16 {
        return None;
    }
    let p = TEXTURES.get(&layer)?.get((y * 16 + x) * 4..(y * 16 + x + 1) * 4)?;
    Some(p.try_into().unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn catalogue_and_original_formats_are_valid() {
        validate().unwrap();
        assert_eq!(RESOURCES.len(), CATALOGUE["resources"].as_array().unwrap().len());
        assert!(resource("voxelcraft:missing_optional").is_none());
        assert!(model_boxes("voxelcraft:missing").unwrap_err().contains("missing resource"));
        assert_eq!(audio_samples("voxelcraft:pickup").unwrap().len(), 4800);
    }
    #[cfg(feature = "client")]
    #[test]
    fn original_pixels_override_only_their_mapped_layer() {
        assert_eq!(texture_pixel(0, 1, 1), Some([117, 122, 130, 255]));
        assert_eq!(texture_pixel(1, 1, 1), None);
    }
}
