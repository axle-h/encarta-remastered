//! Content sniffing: what a file really is, from its first bytes (never from its extension).
//! The disc has mislabelled files (a BMP named `.jpg`, a GIF named `.bmp`, SWA audio named `.mp3`),
//! so every media decision starts here. See docs/media.md §1.

/// File formats seen on the disc, as identified by their leading bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Format {
    Jpeg,
    Gif,
    Png,
    Bmp,
    /// RIFF/WAVE (PCM, GSM 6.10, MS ADPCM …).
    Wav,
    /// Advanced Systems Format (WMA audio or WMV video).
    Asf,
    /// MPEG audio, with or without an ID3v2 tag.
    Mp3,
    /// Shockwave Audio: a big-endian header length, then MPEG audio frames (shockwave.md §4).
    Swa,
    Swf,
    Midi,
    /// Director Afterburner movie or cast (`XFIR`).
    Director,
    /// Windows cursor (ICO layout with type 2).
    Cursor,
    Psd,
    /// OLE compound file (Thumbs.db).
    Ole,
    /// SAMI captions (`<SAMI>`).
    Sami,
    /// XML (including `.iax` interactives, sidebars, labels, XSL).
    Xml,
    /// Other markup (HTML fragments).
    Markup,
    /// Anything else (binary tables, scripts, text).
    Other,
    Empty,
}

impl Format {
    /// Canonical file extension for the stored original.
    pub fn ext(self) -> &'static str {
        match self {
            Format::Jpeg => "jpg",
            Format::Gif => "gif",
            Format::Png => "png",
            Format::Bmp => "bmp",
            Format::Wav => "wav",
            Format::Asf => "asf",
            Format::Mp3 => "mp3",
            Format::Swa => "swa",
            Format::Swf => "swf",
            Format::Midi => "mid",
            Format::Director => "dcr",
            Format::Cursor => "cur",
            Format::Psd => "psd",
            Format::Ole => "ole",
            Format::Sami => "smi",
            Format::Xml => "xml",
            Format::Markup => "html",
            Format::Other | Format::Empty => "bin",
        }
    }

    pub fn mime(self) -> &'static str {
        match self {
            Format::Jpeg => "image/jpeg",
            Format::Gif => "image/gif",
            Format::Png => "image/png",
            Format::Bmp => "image/bmp",
            Format::Wav => "audio/wav",
            Format::Asf => "video/x-ms-asf",
            Format::Mp3 => "audio/mpeg",
            Format::Swa => "audio/x-swa",
            Format::Swf => "application/x-shockwave-flash",
            Format::Midi => "audio/midi",
            Format::Director => "application/x-director",
            Format::Cursor => "image/x-win-bitmap",
            Format::Psd => "image/vnd.adobe.photoshop",
            Format::Ole => "application/x-ole-storage",
            Format::Sami => "application/x-sami",
            Format::Xml => "application/xml",
            Format::Markup => "text/html",
            Format::Other | Format::Empty => "application/octet-stream",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Format::Jpeg => "jpeg",
            Format::Gif => "gif",
            Format::Png => "png",
            Format::Bmp => "bmp",
            Format::Wav => "wav",
            Format::Asf => "asf",
            Format::Mp3 => "mp3",
            Format::Swa => "swa",
            Format::Swf => "swf",
            Format::Midi => "midi",
            Format::Director => "director",
            Format::Cursor => "cursor",
            Format::Psd => "psd",
            Format::Ole => "ole",
            Format::Sami => "sami",
            Format::Xml => "xml",
            Format::Markup => "markup",
            Format::Other => "other",
            Format::Empty => "empty",
        }
    }
}

/// The ASF header object GUID `75B22630-668E-11CF-A6D9-00AA0062CE6C`, as stored.
const ASF_GUID: [u8; 16] = [
    0x30, 0x26, 0xB2, 0x75, 0x8E, 0x66, 0xCF, 0x11, 0xA6, 0xD9, 0x00, 0xAA, 0x00, 0x62, 0xCE, 0x6C,
];

/// Number of leading bytes [`sniff`] looks at.
pub const SNIFF_LEN: usize = 64;

/// Identifies a file from its first bytes (pass at least [`SNIFF_LEN`] bytes when available).
pub fn sniff(b: &[u8]) -> Format {
    if b.is_empty() {
        return Format::Empty;
    }
    let starts = |m: &[u8]| b.starts_with(m);
    if starts(&[0xFF, 0xD8, 0xFF]) {
        return Format::Jpeg;
    }
    if starts(b"GIF87a") || starts(b"GIF89a") {
        return Format::Gif;
    }
    if starts(b"\x89PNG\r\n\x1a\n") {
        return Format::Png;
    }
    if starts(b"BM") && b.len() >= 14 {
        return Format::Bmp;
    }
    if starts(b"RIFF") && b.get(8..12) == Some(b"WAVE") {
        return Format::Wav;
    }
    if starts(&ASF_GUID) {
        return Format::Asf;
    }
    if starts(b"FWS") || starts(b"CWS") || starts(b"ZWS") {
        return Format::Swf;
    }
    if starts(b"MThd") {
        return Format::Midi;
    }
    if starts(b"XFIR") || starts(b"RIFX") {
        return Format::Director;
    }
    if starts(b"8BPS") {
        return Format::Psd;
    }
    if starts(&[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1]) {
        return Format::Ole;
    }
    if starts(&[0, 0, 2, 0]) && b.len() >= 6 && b[4..6] != [0, 0] {
        return Format::Cursor;
    }
    if is_swa(b) {
        return Format::Swa;
    }
    // UTF-16LE text starts with FF FE, which would also pass as an MPEG frame sync.
    if starts(&[0xFF, 0xFE]) {
        return sniff_text(b);
    }
    if starts(b"ID3") || is_mpeg_audio_sync(b) {
        return Format::Mp3;
    }
    sniff_text(b)
}

/// SWA: big-endian u32 header length L (0x140, or 0x164 on four IAF files), then u32 3, then the
/// sample rate. The MPEG frames start at L + 4.
fn is_swa(b: &[u8]) -> bool {
    if b.len() < 12 {
        return false;
    }
    let be = |o: usize| u32::from_be_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]);
    let header = be(0);
    (0x40..0x1000).contains(&header) && be(4) == 3 && matches!(be(8), 8000..=48000)
}

/// Offset of the first MPEG frame in an SWA file, if the header is well formed.
pub fn swa_audio_offset(b: &[u8]) -> Option<usize> {
    if !is_swa(b) {
        return None;
    }
    let off = u32::from_be_bytes([b[0], b[1], b[2], b[3]]) as usize + 4;
    (b.len() > off + 2 && is_mpeg_audio_sync(&b[off..])).then_some(off)
}

/// MPEG audio frame sync: 11 set bits, a valid layer and a non-reserved bitrate.
fn is_mpeg_audio_sync(b: &[u8]) -> bool {
    b.len() >= 3
        && b[0] == 0xFF
        && (b[1] & 0xE0) == 0xE0
        && (b[1] >> 1) & 3 != 0
        && (b[2] >> 4) != 0xF
        && (b[1] >> 3) & 3 != 1
}

fn sniff_text(b: &[u8]) -> Format {
    // UTF-16LE text (with BOM): decode the ASCII range only.
    let text: Vec<u8> = if b.starts_with(&[0xFF, 0xFE]) {
        b[2..].chunks(2).filter(|c| c.len() == 2 && c[1] == 0).map(|c| c[0]).collect()
    } else if let Some(rest) = b.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        rest.to_vec()
    } else {
        b.to_vec()
    };
    let t = text.trim_ascii_start();
    let lower: Vec<u8> = t.iter().take(16).map(u8::to_ascii_lowercase).collect();
    if lower.starts_with(b"<sami") {
        Format::Sami
    } else if lower.starts_with(b"<?xml") {
        Format::Xml
    } else if lower.starts_with(b"<") {
        Format::Markup
    } else {
        Format::Other
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn images() {
        assert_eq!(sniff(&[0xFF, 0xD8, 0xFF, 0xE0, 0, 0x10]), Format::Jpeg);
        assert_eq!(sniff(b"GIF89a\x10\x00"), Format::Gif);
        assert_eq!(sniff(b"GIF87a\x10\x00"), Format::Gif);
        assert_eq!(sniff(b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR"), Format::Png);
        assert_eq!(sniff(b"BM\x36\x10\0\0\0\0\0\0\x36\0\0\0\x28\0"), Format::Bmp);
        // A cursor directory: reserved 0, type 2, one image.
        assert_eq!(sniff(&[0, 0, 2, 0, 1, 0, 32, 32]), Format::Cursor);
        // Type 1 is an icon, not a cursor.
        assert_ne!(sniff(&[0, 0, 1, 0, 1, 0, 32, 32]), Format::Cursor);
    }

    #[test]
    fn audio_video() {
        let mut wav = b"RIFF\x24\0\0\0WAVEfmt ".to_vec();
        wav.extend([0; 8]);
        assert_eq!(sniff(&wav), Format::Wav);
        assert_eq!(sniff(b"RIFF\x24\0\0\0AVI LIST"), Format::Other);
        let mut asf = ASF_GUID.to_vec();
        asf.extend([0; 8]);
        assert_eq!(sniff(&asf), Format::Asf);
        assert_eq!(sniff(b"ID3\x03\0\0\0\0\0\0"), Format::Mp3);
        // MPEG-1 Layer III, 128 kbit/s, 44.1 kHz.
        assert_eq!(sniff(&[0xFF, 0xFB, 0x90, 0x64]), Format::Mp3);
        // A UTF-16LE BOM (FF FE) must not be taken for an MPEG sync.
        assert_eq!(sniff(&"\u{feff}<?xml".encode_utf16().flat_map(|u| u.to_le_bytes()).collect::<Vec<_>>()), Format::Xml);
        assert_eq!(sniff(b"MThd\0\0\0\x06"), Format::Midi);
        assert_eq!(sniff(b"FWS\x06"), Format::Swf);
        assert_eq!(sniff(b"CWS\x08"), Format::Swf);
    }

    fn swa_bytes(header: u32) -> Vec<u8> {
        let mut b = header.to_be_bytes().to_vec();
        b.extend(3u32.to_be_bytes());
        b.extend(44100u32.to_be_bytes());
        b.extend(48000u32.to_be_bytes());
        b.extend(b"MACR");
        b.resize(header as usize + 4, 0);
        b.extend([0xFF, 0xFB, 0x50, 0xC4, 0, 0]);
        b
    }

    #[test]
    fn swa() {
        for h in [0x140u32, 0x160] {
            let b = swa_bytes(h);
            assert_eq!(sniff(&b), Format::Swa);
            assert_eq!(swa_audio_offset(&b), Some(h as usize + 4));
        }
        // Header that does not end at a frame sync is rejected for stripping.
        let mut b = swa_bytes(0x140);
        b[0x144] = 0;
        assert_eq!(swa_audio_offset(&b), None);
    }

    #[test]
    fn text_formats() {
        assert_eq!(sniff(b"<SAMI>\r\n<Head>"), Format::Sami);
        assert_eq!(sniff(b"\xEF\xBB\xBF<?xml version=\"1.0\"?><iai"), Format::Xml);
        assert_eq!(sniff(b"<IFRAME id=\"Overlay\""), Format::Markup);
        assert_eq!(sniff(b"xof 0302bin 0032"), Format::Other);
        assert_eq!(sniff(b""), Format::Empty);
        assert_eq!(sniff(b"XFIR\0\0\0\0MDGF"), Format::Director);
        assert_eq!(sniff(b"8BPS\0\x01"), Format::Psd);
    }
}
