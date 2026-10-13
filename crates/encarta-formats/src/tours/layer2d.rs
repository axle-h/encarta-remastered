//! `.32d` 2D layers (spec §8.1): the loading screen (`begin.32d`) and the captions and hotspot
//! boxes (`lh.32d`).

use anyhow::{Context, Result, ensure};
use serde::Serialize;

use super::{Reader, c_string, cp1252};

#[derive(Debug, Clone, Serialize)]
pub struct Layer {
    /// 4 on the disc; the loader rejects < 4.
    pub version: u32,
    /// Restored into the layer's ON flag.
    pub layer_on: u32,
    /// Authoring path of the icon strip (reported by `2D.ICONFILE?`, not loaded).
    pub icon_file: String,
    /// Written by the saver, skipped by the loader.
    pub icon_count: u32,
    pub elements: Vec<Element>,
}

/// One pre-transformed screen vertex.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct ScreenVertex {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub rhw: f32,
    pub diffuse: u32,
    pub specular: u32,
    pub u: f32,
    pub v: f32,
}

/// Win32 LOGFONTA.
#[derive(Debug, Clone, Serialize)]
pub struct LogFont {
    pub height: i32,
    pub width: i32,
    pub escapement: i32,
    pub orientation: i32,
    pub weight: i32,
    pub italic: u8,
    pub underline: u8,
    pub strike_out: u8,
    pub charset: u8,
    pub out_precision: u8,
    pub clip_precision: u8,
    pub quality: u8,
    pub pitch_and_family: u8,
    pub face: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum ElementKind {
    Plain,
    Hotspot,
    ButtonStrip,
    Other(u32),
}

#[derive(Debug, Clone, Serialize)]
pub struct Element {
    /// 6 (triangle fan) everywhere; hit testing only considers 6.
    pub primitive: u32,
    pub kind: ElementKind,
    pub additive: u32,
    pub visible: bool,
    pub textured: bool,
    pub captures_mouse: bool,
    pub anchored: bool,
    /// Magenta is transparent; for labels also selects the outlined-caption style.
    pub colour_key: bool,
    pub tex_blend: u32,
    pub diffuse: u32,
    pub shade_mode: u32,
    pub anchor: [f32; 3],
    pub blend: u32,
    pub is_label: bool,
    pub label_fill: u32,
    pub frame_colour: u32,
    pub icon_normal: i32,
    pub icon_hover: i32,
    pub icon_pressed: i32,
    pub fan_indices: [u16; 4],
    pub vertex_count: u32,
    /// Top-left, top-right, bottom-right, bottom-left, in client pixels.
    pub vertices: [ScreenVertex; 4],
    pub text_colour: u32,
    pub font: LogFont,
    pub text: String,
    pub has_text: bool,
    pub macro_mouse_down: String,
    pub macro_mouse_up: String,
    pub macro_mouse_in: String,
    pub macro_mouse_out: String,
    pub name: String,
    /// Image file for non-labels; empty for labels.
    pub image: String,
    /// Fields the runtime never reads (offsets 0x0C, 0x30, 0x34, 0x50, 0x5C, 0x102, 0x10A), kept
    /// so the record round-trips. 0x34 is authoring junk.
    pub unread: UnreadFields,
}

#[derive(Debug, Clone, Serialize)]
pub struct UnreadFields {
    pub at_0c: u32,
    pub at_30: u32,
    pub junk_34: u32,
    pub at_50: [u32; 2],
    pub at_5c: [u32; 2],
    pub at_102: [u32; 2],
    pub zero_10a: [u8; 12],
    pub pad_78: u16,
}

impl Element {
    /// (left, top, right, bottom) in pixels from the first and third vertex.
    pub fn rect(&self) -> [f32; 4] {
        [self.vertices[0].x, self.vertices[0].y, self.vertices[2].x, self.vertices[2].y]
    }

    /// Element class by name: `_H*` hotspot box, `_L*` caption, otherwise other.
    pub fn is_hotspot_box(&self) -> bool {
        self.name.len() > 2 && self.name[..2].eq_ignore_ascii_case("_H")
    }

    pub fn is_caption(&self) -> bool {
        self.name.len() > 2 && self.name[..2].eq_ignore_ascii_case("_L")
    }
}

impl Layer {
    /// Element by script id: case-insensitive, first match.
    pub fn element(&self, id: &str) -> Option<(usize, &Element)> {
        self.elements.iter().enumerate().find(|(_, e)| e.name.eq_ignore_ascii_case(id))
    }
}

fn flag(r: &mut Reader) -> Result<bool> {
    Ok(r.u32()? != 0)
}

fn lptext(r: &mut Reader) -> Result<String> {
    Ok(cp1252(r.lpstr()?))
}

fn element(r: &mut Reader) -> Result<Element> {
    let primitive = r.u32()?;
    let kind = match r.u32()? {
        0 => ElementKind::Plain,
        1 => ElementKind::Hotspot,
        2 => ElementKind::ButtonStrip,
        k => ElementKind::Other(k),
    };
    let additive = r.u32()?;
    let at_0c = r.u32()?;
    let visible = flag(r)?;
    let textured = flag(r)?;
    let captures_mouse = flag(r)?;
    let anchored = flag(r)?;
    let colour_key = flag(r)?;
    let tex_blend = r.u32()?;
    let diffuse = r.u32()?;
    let shade_mode = r.u32()?;
    let at_30 = r.u32()?;
    let junk_34 = r.u32()?;
    let anchor = r.vec3()?;
    let blend = r.u32()?;
    let is_label = flag(r)?;
    let label_fill = r.u32()?;
    let at_50 = [r.u32()?, r.u32()?];
    let frame_colour = r.u32()?;
    let at_5c = [r.u32()?, r.u32()?];
    let icon_normal = r.i32()?;
    let icon_hover = r.i32()?;
    let icon_pressed = r.i32()?;
    let fan_indices = [r.u16()?, r.u16()?, r.u16()?, r.u16()?];
    let pad_78 = r.u16()?;
    let vertex_count = r.u32()?;
    let mut vertices = [ScreenVertex { x: 0.0, y: 0.0, z: 0.0, rhw: 0.0, diffuse: 0, specular: 0, u: 0.0, v: 0.0 }; 4];
    for v in &mut vertices {
        *v = ScreenVertex { x: r.f32()?, y: r.f32()?, z: r.f32()?, rhw: r.f32()?, diffuse: r.u32()?, specular: r.u32()?, u: r.f32()?, v: r.f32()? };
    }
    let text_colour = r.u32()?;
    let at_102 = [r.u32()?, r.u32()?];
    let zero_10a = r.array::<12>()?;
    let font = LogFont {
        height: r.i32()?,
        width: r.i32()?,
        escapement: r.i32()?,
        orientation: r.i32()?,
        weight: r.i32()?,
        italic: r.u8()?,
        underline: r.u8()?,
        strike_out: r.u8()?,
        charset: r.u8()?,
        out_precision: r.u8()?,
        clip_precision: r.u8()?,
        quality: r.u8()?,
        pitch_and_family: r.u8()?,
        face: c_string(r.bytes(32)?),
    };
    let text = lptext(r)?;
    let has_text = flag(r)?;
    let macro_mouse_down = lptext(r)?;
    let macro_mouse_up = lptext(r)?;
    let macro_mouse_in = lptext(r)?;
    let macro_mouse_out = lptext(r)?;
    let name = lptext(r)?;
    let image = lptext(r)?;
    Ok(Element {
        primitive,
        kind,
        additive,
        visible,
        textured,
        captures_mouse,
        anchored,
        colour_key,
        tex_blend,
        diffuse,
        shade_mode,
        anchor,
        blend,
        is_label,
        label_fill,
        frame_colour,
        icon_normal,
        icon_hover,
        icon_pressed,
        fan_indices,
        vertex_count,
        vertices,
        text_colour,
        font,
        text,
        has_text,
        macro_mouse_down,
        macro_mouse_up,
        macro_mouse_in,
        macro_mouse_out,
        name,
        image,
        unread: UnreadFields { at_0c, at_30, junk_34, at_50, at_5c, at_102, zero_10a, pad_78 },
    })
}

pub fn parse(data: &[u8]) -> Result<Layer> {
    let mut r = Reader::new(data);
    let version = r.u32()?;
    ensure!(version >= 4, "2D layer version {version} < 4");
    let layer_on = r.u32()?;
    let icon_file = lptext(&mut r)?;
    let icon_count = r.u32()?;
    let n = r.i32()?;
    ensure!(n >= 0, "negative element count {n}");
    let mut elements = Vec::with_capacity(n as usize);
    for i in 0..n {
        let at = r.pos();
        elements.push(element(&mut r).with_context(|| format!("element {i} at offset {at}"))?);
    }
    r.finish("2D layer")?;
    Ok(Layer { version, layer_on, icon_file, icon_count, elements })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lp(v: &mut Vec<u8>, s: &str) {
        v.extend_from_slice(&(s.len() as u32).to_le_bytes());
        v.extend_from_slice(s.as_bytes());
    }

    fn label(name: &str) -> Vec<u8> {
        let mut v = Vec::new();
        let u = |v: &mut Vec<u8>, x: u32| v.extend_from_slice(&x.to_le_bytes());
        for x in [6, 0, 0, 0, 1, 1, 0, 0, 1, 1, 0xFFFFFF, 1, 0xFFFF_FFFF, 0x6C37_1C73] {
            u(&mut v, x);
        }
        for _ in 0..3 {
            v.extend_from_slice(&0f32.to_le_bytes());
        }
        for x in [2, 1, 0xFF00FF, 0xFF00FF, 0xFF00FF, 0xC0D1E3, 0xFFFFFF, 0xFFFFFF] {
            u(&mut v, x);
        }
        for x in [-1i32, 1, 0] {
            v.extend_from_slice(&x.to_le_bytes());
        }
        for x in [0u16, 1, 2, 3, 0] {
            v.extend_from_slice(&x.to_le_bytes());
        }
        u(&mut v, 4);
        for (x, y) in [(22.0f32, 34.0f32), (111.0, 34.0), (111.0, 56.0), (22.0, 56.0)] {
            for f in [x, y, 1e-5, 1.0] {
                v.extend_from_slice(&f.to_le_bytes());
            }
            u(&mut v, 0xFFFFFF);
            u(&mut v, 0);
            v.extend_from_slice(&0f32.to_le_bytes());
            v.extend_from_slice(&0f32.to_le_bytes());
        }
        for x in [0xFFFFFF, 0xFFFFFF, 0xFFFFFF] {
            u(&mut v, x);
        }
        v.extend_from_slice(&[0; 12]);
        for x in [-11i32, 0, 0, 0, 700] {
            v.extend_from_slice(&x.to_le_bytes());
        }
        v.extend_from_slice(&[0, 0, 0, 1, 0, 0, 0, 0x20]);
        let mut face = b"Tahoma\0".to_vec();
        face.resize(32, 0xAB);
        v.extend(face);
        lp(&mut v, "ERS Label");
        u(&mut v, 0);
        for _ in 0..4 {
            lp(&mut v, "");
        }
        lp(&mut v, name);
        lp(&mut v, "");
        v
    }

    #[test]
    fn parses_label_layer() {
        let mut f = Vec::new();
        f.extend_from_slice(&4u32.to_le_bytes());
        f.extend_from_slice(&1u32.to_le_bytes());
        lp(&mut f, "\\\\share\\3DVTicon.bmp");
        f.extend_from_slice(&0u32.to_le_bytes());
        f.extend_from_slice(&1i32.to_le_bytes());
        let rec = label("_L1");
        assert_eq!(rec.len(), 382, "record size of the spec's worked example");
        f.extend(rec);
        let l = parse(&f).unwrap();
        let e = &l.elements[0];
        assert_eq!((e.name.as_str(), e.text.as_str(), e.font.face.as_str(), e.font.weight), ("_L1", "ERS Label", "Tahoma", 700));
        assert_eq!(e.rect(), [22.0, 34.0, 111.0, 56.0]);
        assert!(e.is_label && e.colour_key && e.is_caption() && !e.is_hotspot_box());
        assert!(l.element("_l1").is_some());
        f.push(0);
        assert!(parse(&f).is_err());
    }
}
