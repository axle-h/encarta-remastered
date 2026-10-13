//! The 3D virtual tours (`CONTENT/WORLD/7015/<id>.EIT`): every file kind a tour container holds.
//!
//! Spec: `docs/formats/tours-3d.md`. One submodule per file kind, plus [`tour`], which loads a whole
//! tour directory by following its entry script and reports cross-reference problems.
//!
//! | Module | Files |
//! |---|---|
//! | [`xfile`] | `.x` binary DirectX geometry (the subset the tours use) |
//! | [`script`] | `.3sc` scripts: line loader, macro blocks, statements |
//! | [`walkmap`] | `.3wm` walk maps: ground, collision, hotspot and visibility cells |
//! | [`lights`] | `.3lt` lights and vertex paint |
//! | [`cloud`] | `.3cl` point clouds (statue impostors) |
//! | [`layer2d`] | `.32d` 2D layers (HUD, captions, hotspot boxes) |
//! | [`viewpoints`] | `.3vp` sample view points (unused at run time) |
//! | [`textures`] | texture lookup by stem, content sniffing, BMP decoding, magenta colour key |
//!
//! All coordinates are kept exactly as stored (Direct3D, left-handed, +Y up). Converting to glTF
//! (negate Z, reverse winding) is the converter's job.

pub mod cloud;
pub mod layer2d;
pub mod lights;
pub mod script;
pub mod textures;
pub mod tour;
pub mod viewpoints;
pub mod walkmap;
pub mod xfile;

use anyhow::{Result, bail};

/// A 3-component vector, as stored on disc.
pub type Vec3 = [f32; 3];

/// Decodes cp1252 bytes (names and texts in tour files).
pub(crate) fn cp1252(bytes: &[u8]) -> String {
    encoding_rs::WINDOWS_1252.decode_without_bom_handling(bytes).0.into_owned()
}

/// Text up to the first NUL of a fixed-size buffer (the rest is authoring junk).
pub(crate) fn c_string(buf: &[u8]) -> String {
    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    cp1252(&buf[..end])
}

/// Little-endian cursor over a byte slice that reports the offset of a short read.
pub(crate) struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Reader { data, pos: 0 }
    }

    pub fn pos(&self) -> usize {
        self.pos
    }

    pub fn remaining(&self) -> usize {
        self.data.len() - self.pos
    }

    pub fn at_end(&self) -> bool {
        self.pos == self.data.len()
    }

    pub fn bytes(&mut self, n: usize) -> Result<&'a [u8]> {
        if n > self.remaining() {
            bail!("unexpected end of data at offset {} (need {n} bytes, {} left)", self.pos, self.remaining());
        }
        let s = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }

    pub fn array<const N: usize>(&mut self) -> Result<[u8; N]> {
        Ok(self.bytes(N)?.try_into().unwrap())
    }

    pub fn u8(&mut self) -> Result<u8> {
        Ok(self.bytes(1)?[0])
    }

    pub fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_le_bytes(self.array()?))
    }

    pub fn i16(&mut self) -> Result<i16> {
        Ok(i16::from_le_bytes(self.array()?))
    }

    pub fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.array()?))
    }

    pub fn i32(&mut self) -> Result<i32> {
        Ok(i32::from_le_bytes(self.array()?))
    }

    pub fn f32(&mut self) -> Result<f32> {
        Ok(f32::from_le_bytes(self.array()?))
    }

    pub fn vec3(&mut self) -> Result<Vec3> {
        Ok([self.f32()?, self.f32()?, self.f32()?])
    }

    /// A count that must fit in the remaining bytes at `item_size` bytes per item.
    pub fn count(&mut self, item_size: usize) -> Result<usize> {
        let at = self.pos;
        let n = self.u32()? as usize;
        if item_size > 0 && n > self.remaining() / item_size {
            bail!("count {n} at offset {at} exceeds the remaining {} bytes", self.remaining());
        }
        Ok(n)
    }

    /// `u32 length` + that many bytes (no terminator).
    pub fn lpstr(&mut self) -> Result<&'a [u8]> {
        let n = self.count(1)?;
        self.bytes(n)
    }

    /// Fails unless every byte was consumed.
    pub fn finish(&self, what: &str) -> Result<()> {
        if !self.at_end() {
            bail!("{what}: {} unexplained trailing bytes at offset {}", self.remaining(), self.pos);
        }
        Ok(())
    }
}
