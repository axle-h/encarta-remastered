//! Reader for Afterburner-compressed Director 8.5 files (`.dcr` movies, `.cct` casts) and
//! Shockwave Audio (`.swa`), plus converters for the two interactives that use them.
//!
//! Spec: `docs/formats/shockwave.md`. Layers:
//! - [`container`]: the `XFIR` Afterburner container (`Fver`, `Fcdr`, `ABMP`, `FGEI`) → resources
//! - [`cast`]: `KEY*`, `MCsL`, `CAS*`, `CASt`, `LctX`/`Lnam` → cast libraries and members
//! - [`text`], [`bitmap`], [`sound`], [`extract`]: member data → text, PNG, MP3, SWF
//! - [`proplist`]: Lingo list literals in field text → values / JSON
//! - [`interactive`]: City Climate and Population Chart → our JSON
//! - [`locate`]: `dswMedia` keys and catalog lookups

pub mod bitmap;
pub mod bytes;
pub mod cast;
pub mod container;
pub mod extract;
pub mod interactive;
pub mod locate;
pub mod proplist;
pub mod sound;
pub mod text;

pub use cast::{CastLib, DirectorFile, Member, MemberType};
pub use container::{Container, FileKind};
pub use sound::swa_to_mp3;

/// Convert an `.swa` file to MP3 for the media converter: strips the header (L + 4 bytes, usually 0x144) after
/// checking that MPEG frames follow it.
pub fn swa_file_to_mp3(swa: &[u8]) -> anyhow::Result<Vec<u8>> {
    Ok(sound::swa_to_mp3(swa)?.0.to_vec())
}
