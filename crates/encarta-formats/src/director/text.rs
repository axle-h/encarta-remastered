//! Field text (`STXT`): u32 header length, u32 text length, u32 style-run length, then the text
//! (Windows-1252, CR line breaks) and the style runs.

use super::bytes::{be_u32, windows_1252};
use anyhow::{Context, Result, ensure};

/// Decoded text of an `STXT` resource, line breaks as `\n`.
pub fn field_text(stxt: &[u8]) -> Result<String> {
    let header = be_u32(stxt, 0).context("STXT header")? as usize;
    let len = be_u32(stxt, 4).context("STXT text length")? as usize;
    ensure!(
        header >= 12 && header + len <= stxt.len(),
        "STXT text runs past end"
    );
    Ok(windows_1252(&stxt[header..header + len]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_synthetic_stxt() {
        let text = b"caf\xe9\rline";
        let mut b = vec![0, 0, 0, 12, 0, 0, 0, text.len() as u8, 0, 0, 0, 2];
        b.extend_from_slice(text);
        b.extend_from_slice(&[0, 0]);
        assert_eq!(field_text(&b).unwrap(), "café\nline");
        b[7] = 99;
        assert!(field_text(&b).is_err());
    }
}
