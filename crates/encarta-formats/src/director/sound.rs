//! Shockwave Audio: `.swa` files (a 0x144-byte header, then MPEG audio) and SWA-compressed
//! `snd ` members (a header, then the same kind of frames). Spec: `docs/formats/shockwave.md` §4.

use super::bytes::be_u32;
use anyhow::{Result, bail, ensure};
use serde::Serialize;

/// Size of the `.swa` file header.
/// The usual `.swa` header length (L = 0x140 plus the 4-byte length field).
pub const SWA_HEADER_LEN: usize = 0x144;

/// The fields of an MPEG audio frame header we need to walk a stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct FrameHeader {
    /// 1 = MPEG-1, 2 = MPEG-2, 25 = MPEG-2.5
    pub version: u8,
    pub layer: u8,
    pub bitrate_kbps: u32,
    pub sample_rate: u32,
    pub mono: bool,
    pub len: usize,
}

/// Parse a frame header at the start of `b` (Layer II and III).
pub fn frame_header(b: &[u8]) -> Option<FrameHeader> {
    let h = u32::from_be_bytes(b.get(..4)?.try_into().ok()?);
    if h >> 21 != 0x7ff {
        return None;
    }
    let version = match (h >> 19) & 3 {
        0 => 25,
        2 => 2,
        3 => 1,
        _ => return None,
    };
    let layer = match (h >> 17) & 3 {
        1 => 3,
        2 => 2,
        _ => return None,
    };
    let br_idx = ((h >> 12) & 0xf) as usize;
    let sr_idx = ((h >> 10) & 3) as usize;
    if br_idx == 0 || br_idx == 15 || sr_idx == 3 {
        return None;
    }
    const BR_V1_L3: [u32; 15] = [
        0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320,
    ];
    const BR_V1_L2: [u32; 15] = [
        0, 32, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320, 384,
    ];
    const BR_V2: [u32; 15] = [0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160];
    let bitrate = match (version, layer) {
        (1, 3) => BR_V1_L3[br_idx],
        (1, _) => BR_V1_L2[br_idx],
        _ => BR_V2[br_idx],
    };
    let base = [44100, 48000, 32000][sr_idx];
    let sample_rate = match version {
        1 => base,
        2 => base / 2,
        _ => base / 4,
    };
    let pad = ((h >> 9) & 1) as usize;
    let coeff = if layer == 3 && version != 1 { 72 } else { 144 };
    let len = (coeff * bitrate as usize * 1000) / sample_rate as usize + pad;
    let mono = (h >> 6) & 3 == 3;
    Some(FrameHeader {
        version,
        layer,
        bitrate_kbps: bitrate,
        sample_rate,
        mono,
        len,
    })
}

/// Summary of a run of back-to-back frames.
#[derive(Debug, Clone, Serialize)]
pub struct Mp3Stream {
    pub offset: usize,
    pub frames: usize,
    /// bytes covered by whole frames, from `offset`
    pub len: usize,
    pub first: FrameHeader,
    pub duration_secs: f64,
}

/// Walk back-to-back frames from `at`.
pub fn walk_frames(b: &[u8], at: usize) -> Option<Mp3Stream> {
    let first = frame_header(b.get(at..)?)?;
    let (mut p, mut frames, mut samples) = (at, 0usize, 0f64);
    while let Some(h) = b.get(p..).and_then(frame_header) {
        if p + h.len > b.len() || h.sample_rate != first.sample_rate || h.layer != first.layer {
            break;
        }
        let per_frame = if h.layer == 3 && h.version != 1 {
            576.0
        } else {
            1152.0
        };
        samples += per_frame / h.sample_rate as f64;
        frames += 1;
        p += h.len;
    }
    (frames > 0).then(|| Mp3Stream {
        offset: at,
        frames,
        len: p - at,
        first,
        duration_secs: samples,
    })
}

/// First offset from which at least `min_frames` frames follow each other (or the stream ends).
pub fn find_stream(b: &[u8], min_frames: usize) -> Option<Mp3Stream> {
    (0..b.len().saturating_sub(4)).find_map(|i| {
        if b[i] != 0xff {
            return None;
        }
        let s = walk_frames(b, i)?;
        (s.frames >= min_frames || s.offset + s.len == b.len()).then_some(s)
    })
}

/// `.swa` header fields (big-endian).
#[derive(Debug, Clone, Serialize)]
pub struct SwaHeader {
    pub header_len: u32,
    pub sample_rate: u32,
    pub bit_rate: u32,
}

/// Validate an `.swa` file and return the MP3 bytes after its header with the stream summary.
/// The header starts with BE u32 L (the length after the first 4 bytes: 0x140 usually, 0x164 on a
/// few IAF files), u32 3, the sample rate and the bit rate, and holds `MACR`; frames must start
/// right after it, at L + 4.
pub fn swa_to_mp3(file: &[u8]) -> Result<(&[u8], SwaHeader, Mp3Stream)> {
    let header_len = be_u32(file, 0).unwrap_or(0);
    ensure!(
        (0x40..=0x1000).contains(&header_len),
        "unexpected SWA header length {header_len:#x}"
    );
    let start = header_len as usize + 4;
    ensure!(file.len() > start + 4, "file too short for an SWA header");
    ensure!(
        file[..start].windows(4).any(|w| w == b"MACR"),
        "no MACR tag in the SWA header"
    );
    let hdr = SwaHeader {
        header_len,
        sample_rate: be_u32(file, 8).unwrap_or(0),
        bit_rate: be_u32(file, 12).unwrap_or(0),
    };
    let Some(s) = walk_frames(file, start) else {
        bail!("no MPEG frame sync at offset {start:#x}")
    };
    ensure!(
        s.frames >= 2,
        "only {} MPEG frame(s) after the SWA header",
        s.frames
    );
    ensure!(
        s.first.sample_rate == hdr.sample_rate,
        "SWA header says {} Hz, frames say {} Hz",
        hdr.sample_rate,
        s.first.sample_rate
    );
    Ok((&file[start..], hdr, s))
}

/// MP3 bytes inside an SWA-compressed `snd ` member: the first run of chained frames to the end.
pub fn sound_member_mp3(snd: &[u8]) -> Result<(&[u8], Mp3Stream)> {
    match find_stream(snd, 3) {
        Some(s) => Ok((&snd[s.offset..], s)),
        None => bail!("no MPEG audio frames in sound data"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A synthetic MPEG-1 Layer III frame header: 48 kbit/s, 44.1 kHz, mono, no padding.
    fn frame() -> Vec<u8> {
        let mut f = vec![0xff, 0xfb, 0x30, 0xc4];
        f.resize(156, 0);
        f
    }

    #[test]
    fn frame_lengths() {
        let h = frame_header(&frame()).unwrap();
        assert_eq!(
            (
                h.version,
                h.layer,
                h.bitrate_kbps,
                h.sample_rate,
                h.mono,
                h.len
            ),
            (1, 3, 48, 44100, true, 156)
        );
        let h2 = frame_header(&[0xff, 0xf3, 0x62, 0xc4]).unwrap();
        assert_eq!(
            (h2.version, h2.sample_rate, h2.bitrate_kbps, h2.len),
            (2, 22050, 48, 157)
        );
        assert!(frame_header(&[0xff, 0xff, 0xff, 0xff]).is_none());
    }

    #[test]
    fn swa_header_stripped() {
        let mut f = vec![0u8; SWA_HEADER_LEN];
        f[0..4].copy_from_slice(&0x140u32.to_be_bytes());
        f[4..8].copy_from_slice(&3u32.to_be_bytes());
        f[8..12].copy_from_slice(&44100u32.to_be_bytes());
        f[12..16].copy_from_slice(&48000u32.to_be_bytes());
        f[16..20].copy_from_slice(b"MACR");
        for _ in 0..3 {
            f.extend(frame());
        }
        let (mp3, hdr, s) = swa_to_mp3(&f).unwrap();
        assert_eq!(mp3.len(), 3 * 156);
        assert_eq!((hdr.sample_rate, s.frames), (44100, 3));
        f[SWA_HEADER_LEN] = 0;
        assert!(swa_to_mp3(&f).is_err());
    }

    #[test]
    fn swa_longer_header_stripped() {
        let mut f = vec![0u8; 0x168];
        f[0..4].copy_from_slice(&0x164u32.to_be_bytes());
        f[4..8].copy_from_slice(&3u32.to_be_bytes());
        f[8..12].copy_from_slice(&44100u32.to_be_bytes());
        f[12..16].copy_from_slice(&48000u32.to_be_bytes());
        f[16..20].copy_from_slice(b"MACR");
        for _ in 0..2 {
            f.extend(frame());
        }
        let (mp3, hdr, _) = swa_to_mp3(&f).unwrap();
        assert_eq!((mp3.len(), hdr.header_len), (2 * 156, 0x164));
    }

    #[test]
    fn embedded_stream_found() {
        let mut b = vec![0u8, 0xff, 0x12, 0x40];
        for _ in 0..4 {
            b.extend(frame());
        }
        let (mp3, s) = sound_member_mp3(&b).unwrap();
        assert_eq!((s.offset, s.frames, mp3.len()), (4, 4, 4 * 156));
    }
}
