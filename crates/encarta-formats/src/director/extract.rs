//! Turning one cast member into a usable file: field text, Text Xtra text, bitmap PNG (or the
//! imported JPEG), palette
//! JSON, sound MP3, vector-shape SWF, linked SWA path, script handler names.

use super::bitmap::{self, BitmapInfo, Palette, PaletteRef};
use super::bytes::{be_u32, mac_roman, windows_1252};
use super::cast::{CastLib, DirectorFile, Member, MemberType};
use super::sound;
use super::text::field_text;
use anyhow::{Context, Result, bail};
use serde_json::json;

/// What a member extracted to.
pub enum Extracted {
    Text(String),
    Png(Vec<u8>),
    /// a bitmap kept as its imported JPEG (`ediM`), with no `BITD`
    Jpeg(Vec<u8>),
    Mp3(Vec<u8>),
    Swf(Vec<u8>),
    Json(serde_json::Value),
}

impl Extracted {
    pub fn extension(&self) -> &'static str {
        match self {
            Extracted::Text(_) => "txt",
            Extracted::Png(_) => "png",
            Extracted::Jpeg(_) => "jpg",
            Extracted::Mp3(_) => "mp3",
            Extracted::Swf(_) => "swf",
            Extracted::Json(_) => "json",
        }
    }
    pub fn into_bytes(self) -> Vec<u8> {
        match self {
            Extracted::Text(s) => s.into_bytes(),
            Extracted::Png(b) | Extracted::Jpeg(b) | Extracted::Mp3(b) | Extracted::Swf(b) => b,
            Extracted::Json(v) => serde_json::to_vec_pretty(&v).unwrap_or_default(),
        }
    }
}

/// Palettes from another file (the movie whose internal cast holds the palette that external
/// casts' bitmaps name), keyed by member number.
pub type PaletteSource = Vec<(u32, Palette)>;

/// The palette members of a file's casts, keyed by member number.
pub fn palettes_of(file: &DirectorFile) -> PaletteSource {
    file.members()
        .filter(|(_, m)| m.kind == MemberType::Palette)
        .filter_map(|(_, m)| {
            Some((
                m.number,
                bitmap::parse_clut(file.resource(m.data_of("CLUT")?)?),
            ))
        })
        .collect()
}

/// Pick a bitmap's palette: its own cast's member, a built-in palette, a member of the movie's
/// cast in `extra`, then any palette that is to hand, then the Mac system palette.
pub fn palette_for(
    file: &DirectorFile,
    cast: &CastLib,
    info: &BitmapInfo,
    extra: &PaletteSource,
) -> Palette {
    let own = |n: u32| {
        cast.member(n)
            .filter(|m| m.kind == MemberType::Palette)
            .and_then(|m| file.resource(m.data_of("CLUT")?))
            .map(bitmap::parse_clut)
    };
    match info.palette {
        Some(PaletteRef::BuiltIn(_)) | None => bitmap::mac_system_palette(),
        Some(PaletteRef::Member { number, same_cast }) => {
            let n = number as u32;
            let found = if same_cast { own(n) } else { None };
            found
                .or_else(|| extra.iter().find(|(k, _)| *k == n).map(|(_, p)| p.clone()))
                .or_else(|| own(n))
                .or_else(|| extra.first().map(|(_, p)| p.clone()))
                .or_else(|| palettes_of(file).into_iter().next().map(|(_, p)| p))
                .unwrap_or_else(bitmap::mac_system_palette)
        }
    }
}

/// Plain text of a Text Xtra member (`XMED`). The asset is a run of sections, each introduced by
/// 20 ASCII hex digits (u16 section id, u32 length, u32 count); section 2 holds the characters as
/// `<hex length>,<text>`.
pub fn text_xtra_plain(xmed: &[u8]) -> Option<String> {
    let is_hex = |b: &u8| b.is_ascii_hexdigit();
    let mut i = 0;
    while i + 20 <= xmed.len() {
        if xmed[i..i + 20].iter().all(is_hex) && &xmed[i..i + 4] == b"0002" {
            let mut p = i + 20;
            while p < xmed.len() && !xmed[p].is_ascii_hexdigit() {
                p += 1;
            }
            let start = p;
            while p < xmed.len() && xmed[p].is_ascii_hexdigit() {
                p += 1;
            }
            if xmed.get(p) != Some(&b',') {
                return None;
            }
            let n = usize::from_str_radix(std::str::from_utf8(&xmed[start..p]).ok()?, 16).ok()?;
            let text = xmed.get(p + 1..p + 1 + n)?;
            return Some(windows_1252(text));
        }
        i += 1;
    }
    None
}

/// Path of the external file an SWA streaming member plays (`@\Clim_in.swa`; `@` = movie folder).
/// Specific data: u32 name length, `swa`, u32 block length, `MACR`, u32 path length, u32, u32,
/// u32, path.
pub fn swa_member_path(specific: &[u8]) -> Option<String> {
    let name_len = be_u32(specific, 0)? as usize;
    let block = 4 + name_len;
    let block_len = be_u32(specific, block)? as usize;
    let body = specific.get(block + 4..block + 4 + block_len)?;
    if body.get(..4)? != b"MACR" {
        return None;
    }
    let n = be_u32(body, 4)? as usize;
    let path = body.get(body.len().checked_sub(n)?..)?;
    Some(mac_roman(path))
}

/// The SWF inside a vector-shape or Flash asset (`XMED`): u32 length, u32 1, u32 SWF length, SWF.
pub fn vector_shape_swf(xmed: &[u8]) -> Option<&[u8]> {
    let n = be_u32(xmed, 8)? as usize;
    let swf = xmed.get(12..12 + n)?;
    (swf.starts_with(b"FWS") || swf.starts_with(b"CWS")).then_some(swf)
}

/// Extract one member. `extra` supplies palettes for bitmaps whose palette lives in the movie.
pub fn extract_member(
    file: &DirectorFile,
    cast: &CastLib,
    m: &Member,
    extra: &PaletteSource,
) -> Result<Extracted> {
    let res = |kind: &str| {
        m.data_of(kind)
            .and_then(|id| file.resource(id))
            .with_context(|| format!("member {} has no {kind} data", m.number))
    };
    Ok(match m.kind {
        MemberType::Field => Extracted::Text(field_text(res("STXT")?)?),
        // KEY* may still name a BITD that the published file left out
        MemberType::Bitmap
            if m.data_of("BITD").and_then(|id| file.resource(id)).is_none()
                && m.data_of("ediM").is_some() =>
        {
            let jpeg = res("ediM")?;
            if !jpeg.starts_with(&[0xff, 0xd8]) {
                bail!("bitmap media is not JPEG");
            }
            Extracted::Jpeg(jpeg.to_vec())
        }
        MemberType::Bitmap => {
            let info = BitmapInfo::parse(&m.specific)?;
            let pal = palette_for(file, cast, &info, extra);
            Extracted::Png(bitmap::decode(&info, res("BITD")?, &pal)?.to_png()?)
        }
        MemberType::Palette => {
            let p = bitmap::parse_clut(res("CLUT")?);
            Extracted::Json(json!(
                p.iter()
                    .map(|[r, g, b]| format!("#{r:02x}{g:02x}{b:02x}"))
                    .collect::<Vec<_>>()
            ))
        }
        // a sound kept as a whole SWA stream (`ediM`) instead of `snd `
        MemberType::Sound if m.data_of("snd ").and_then(|id| file.resource(id)).is_none() => {
            Extracted::Mp3(sound::swa_to_mp3(res("ediM")?)?.0.to_vec())
        }
        MemberType::Sound => {
            let snd = res("snd ")?;
            let (mp3, _) = sound::sound_member_mp3(snd)?;
            Extracted::Mp3(mp3.to_vec())
        }
        MemberType::Script => Extracted::Json(json!({
            "kind": m.script_kind,
            "handlers": cast.handler_names(&file.container, m),
        })),
        MemberType::Xtra => match m.xtra.as_deref().map(str::to_ascii_lowercase).as_deref() {
            Some("text") => Extracted::Text(
                text_xtra_plain(res("XMED")?).context("no text in Text Xtra asset")?,
            ),
            Some("vectorshape" | "flash") => Extracted::Swf(
                vector_shape_swf(res("XMED")?)
                    .context("Flash/vector asset holds no SWF")?
                    .to_vec(),
            ),
            Some("swa") => Extracted::Json(json!({ "linked_file": swa_member_path(&m.specific) })),
            other => bail!("no extractor for Xtra member type {other:?}"),
        },
        other => bail!("no extractor for {} members", other.label()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_xtra_section_two() {
        let mut x = b"FFFF0000000600040001\x01junk".to_vec();
        x.extend_from_slice(b"00020000001C00000000\x0017,na = Data not available\x03");
        assert_eq!(
            text_xtra_plain(&x).as_deref(),
            Some("na = Data not available")
        );
        assert_eq!(text_xtra_plain(b"no sections"), None);
    }

    #[test]
    fn vector_shape() {
        let mut x = vec![0, 0, 0, 20, 0, 0, 0, 1, 0, 0, 0, 8];
        x.extend_from_slice(b"FWS\x01\x08\0\0\0tail");
        assert_eq!(vector_shape_swf(&x), Some(&b"FWS\x01\x08\0\0\0"[..]));
        assert_eq!(vector_shape_swf(&x[..10]), None);
    }

    #[test]
    fn swa_member_link() {
        let mut s = vec![0, 0, 0, 3];
        s.extend_from_slice(b"swa");
        let path = b"@\\Test.swa";
        let mut body = b"MACR".to_vec();
        body.extend_from_slice(&(path.len() as u32).to_be_bytes());
        body.extend_from_slice(&[0, 0, 0, 0xff, 0, 0, 0, 5, 0, 0, 0, 0]);
        body.extend_from_slice(path);
        s.extend_from_slice(&(body.len() as u32).to_be_bytes());
        s.extend_from_slice(&body);
        assert_eq!(swa_member_path(&s).as_deref(), Some("@\\Test.swa"));
    }
}
