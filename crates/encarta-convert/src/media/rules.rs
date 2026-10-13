//! Classification rules: what happens to every file on the disc (docs/media.md §2 and §4).
//! Each EIT member and each loose ISO file gets exactly one [`Disposition`]. Media files go
//! through the media pipeline; everything else is delegated to another converter or omitted
//! with a stated reason. A file that matches no rule is `Unexplained`, which the report treats
//! as an error.

use super::sniff::Format;

/// How the media pipeline treats a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Kind {
    /// JPEG/GIF/PNG/BMP still or animation: original + WebP (+ AVIF for JPEG, PNG for BMP).
    Image,
    /// Windows cursor: original + PNG.
    Cursor,
    /// WAV/WMA: original + Opus + AAC.
    Audio,
    /// MP3: kept as-is (browser-native; also loaded by the Flash players).
    Mp3,
    /// Shockwave Audio: original + stripped MP3 + Opus + AAC.
    Swa,
    /// WMV: original + MP4/H.264 + WebM/VP9 + poster.
    Video,
    /// SAMI captions: original + one WebVTT per caption class.
    Captions,
    /// Standard MIDI file, kept for an in-browser soundfont synth.
    Midi,
    /// SWF, kept for Ruffle.
    Flash,
    /// `.iax` interactive descriptor, kept; its file references go into `iax_ref`.
    Interactive,
    /// IAF player configuration XML (`flashvr.xml`), kept.
    PlayerConfig,
    /// Director movie or cast used by a live interactive, kept (shockwave.md §6).
    Director,
}

impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Kind::Image => "image",
            Kind::Cursor => "cursor",
            Kind::Audio => "audio",
            Kind::Mp3 => "audio",
            Kind::Swa => "audio",
            Kind::Video => "video",
            Kind::Captions => "captions",
            Kind::Midi => "midi",
            Kind::Flash => "flash",
            Kind::Interactive => "interactive",
            Kind::PlayerConfig => "interactive",
            Kind::Director => "director",
        }
    }

    /// Whether the pipeline makes derivatives for this kind (otherwise the file is "kept").
    pub fn converts(self) -> bool {
        !matches!(
            self,
            Kind::Mp3 | Kind::Midi | Kind::Flash | Kind::Interactive | Kind::PlayerConfig | Kind::Director
        )
    }
}

/// Colour key that turns into alpha when a BMP is converted to PNG.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ColourKey {
    /// Atlas flags (`MSWORLD.EIT/dfg`): the pure-green background is transparent
    /// (atlas-and-3d.md §2). On the disc it is (0, 254, 0); any pixel that is pure green in
    /// RGB555 counts.
    Green,
    /// 3D-tour BMP textures: pixels that quantise to magenta in RGB555 are transparent
    /// (tours-3d.md §5.5).
    Magenta,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Disposition {
    Media { kind: Kind, key: Option<ColourKey> },
    Delegated { owner: &'static str, reason: &'static str },
    Omitted { reason: &'static str },
    /// An ITOLITLS container on the ISO; its members are classified one by one.
    Container,
    Unexplained,
}

fn media(kind: Kind) -> Disposition {
    Disposition::Media { kind, key: None }
}

fn ext_of(name: &str) -> String {
    name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default()
}

fn basename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// Director files that no movie on the disc links (shockwave.md §1 and §2.3).
const DIRECTOR_ORPHANS: &[&str] = &[
    "dswmedia/dummy.cct",
    "dswmedia/shared/players/loc_vwr/loc_vwr.dcr",
    "dswmedia/shared/code/animscpt.cct",
    "dswmedia/shared/code/animscpt2.cct",
    "dswmedia/shared/code/animtips.cct",
    "dswmedia/shared/code/chrt.cct",
    "dswmedia/shared/code/entrtxt.cct",
    "dswmedia/shared/code/linkscpt.cct",
    "dswmedia/shared/code/scroltxt.cct",
    "dswmedia/shared/code/tooltip.cct",
    "dswmedia/shared/ia/buildit.cct",
    "dswmedia/shared/ia/iashared.cct",
    "dswmedia/shared/ia/seganim.cct",
    "dswmedia/shared/ia/sldfade.cct",
];

/// Default rule for a media file, by sniffed format (and extension where the format is generic).
fn by_format(member: &str, fmt: Format) -> Disposition {
    let ext = ext_of(member);
    match fmt {
        Format::Jpeg | Format::Gif | Format::Png | Format::Bmp => media(Kind::Image),
        Format::Cursor => media(Kind::Cursor),
        Format::Wav => media(Kind::Audio),
        Format::Asf if ext == "wmv" => media(Kind::Video),
        Format::Asf => media(Kind::Audio),
        Format::Mp3 => media(Kind::Mp3),
        Format::Swa => media(Kind::Swa),
        Format::Swf => media(Kind::Flash),
        Format::Midi => media(Kind::Midi),
        Format::Sami => media(Kind::Captions),
        Format::Xml if ext == "iax" => media(Kind::Interactive),
        _ => Disposition::Unexplained,
    }
}

/// Classifies one member of an unpacked EIT. `container` is the container's path relative to
/// `extracted/eit` (`CONTENT/MDDLX01.EIT`), `member` the member path inside it.
pub fn classify_member(container: &str, member: &str, fmt: Format) -> Disposition {
    let c = container.to_ascii_uppercase();
    let m = member.to_ascii_lowercase();
    let name = basename(&m);
    let ext = ext_of(name);
    let cname = basename(&c);

    if m == "_label.xml" || m == "_labelw.xml" {
        return Disposition::Omitted {
            reason: "container build label (BuildBaggage tool name, build dates, package GUID); no content",
        };
    }
    if c.starts_with("EE/ENCARTA/EDICT/") {
        return Disposition::Delegated {
            owner: "content",
            reason: "dictionary, thesaurus and bilingual entries (XML per id)",
        };
    }
    if cname == "CATALOG.DLX" || cname == "CATALOG.KDC" {
        return Disposition::Delegated {
            owner: "content",
            reason: "catalog2 tables (search-catalog-misc.md §3); media reads them for catalog_key",
        };
    }
    if cname == "ENCXSL.EIT" {
        return Disposition::Omitted {
            reason: "original XSL/JS UI templates: reference only, never shipped (CLEAN_ROOM rule 7)",
        };
    }
    if cname == "TIMELINE.EIT" || cname == "MSWORLD.ITR" {
        return if m.starts_with("$ww$ctcatalog/") {
            Disposition::Delegated {
                owner: "content",
                reason: "ITIR $WW$CTCATALOG strings: timeline titles / MSWORLD media titles and credits (msworld.md §3)",
            }
        } else {
            Disposition::Omitted {
                reason: "ITIR full-text and word-wheel index; search is rebuilt with tantivy (search-catalog-misc.md §2)",
            }
        };
    }
    if c.starts_with("CONTENT/WORLD/7015/") {
        match ext.as_str() {
            "x" | "3sc" | "3wm" | "3lt" | "32d" | "3cl" | "3vp" => {
                return Disposition::Delegated {
                    owner: "tours",
                    reason: "3D tour geometry, scripts, walk maps, lights, HUD layers, point clouds (tours-3d.md)",
                };
            }
            "log" => {
                return Disposition::Omitted {
                    reason: "authoring error.log listing textures missing on the build share (tours-3d.md §5.7)",
                };
            }
            "psd" => {
                return Disposition::Omitted {
                    reason: "Photoshop authoring source; referenced by nothing (tours-3d.md §5.7)",
                };
            }
            _ if fmt == Format::Ole && name == "thumbs.db" => {
                return Disposition::Omitted { reason: "Windows Explorer thumbnail cache (Thumbs.db)" };
            }
            _ => {}
        }
        if fmt == Format::Bmp {
            return Disposition::Media { kind: Kind::Image, key: Some(ColourKey::Magenta) };
        }
    }
    if cname == "MINDMAZE.EIT" && ext == "lst" {
        return Disposition::Delegated {
            owner: "content",
            reason: "MindMaze area lists (article ids per room; mindmaze.md)",
        };
    }
    if cname == "MSWORLD.EIT" {
        if m == "map/map.dat" {
            return Disposition::Omitted {
                reason: "HTML/CSS fragment of the original map UI: reference only (CLEAN_ROOM rule 7)",
            };
        }
        if m.starts_with("dfg/") && fmt == Format::Bmp {
            return Disposition::Media { kind: Kind::Image, key: Some(ColourKey::Green) };
        }
    }
    if (cname.starts_with("MD") || cname.starts_with("PICON") || cname.starts_with("THUMB"))
        && m.starts_with("baggage/")
        && ext == "xml"
        && fmt == Format::Xml
    {
        return Disposition::Delegated {
            owner: "content",
            reason: "sidebar XML (article sidebars, tables, quotations)",
        };
    }
    if fmt == Format::Director {
        return if DIRECTOR_ORPHANS.contains(&m.as_str()) {
            Disposition::Omitted {
                reason: "orphan Director movie/cast: no movie or content links it (shockwave.md §2.3)",
            }
        } else {
            media(Kind::Director)
        };
    }
    if m == "dswmedia/shared/players/iafplayer/flashvr.xml" {
        return media(Kind::PlayerConfig);
    }
    by_format(&m, fmt)
}

/// Classifies one loose file of the ISO (`path` relative to `extracted/iso`).
pub fn classify_iso(path: &str, fmt: Format) -> Disposition {
    let p = path.to_ascii_uppercase();
    let name = basename(&p);
    let ext = ext_of(name);
    let omit = |reason| Disposition::Omitted { reason };
    let delegate = |owner, reason| Disposition::Delegated { owner, reason };

    if p.starts_with("CONTENT/DSV/") || p.starts_with("EE/KIDS/CONTENT/DSV/") {
        return by_format(path, fmt);
    }
    if p.starts_with("SUPPORT/DIRHTML/") {
        return omit("1999 Shockwave support diagnostic page; the app never opens it (shockwave.md §2.3)");
    }
    if p.starts_with("EE/WORKS/") {
        return omit("Microsoft Works Task Launcher integration (icons, registration); installer asset, not content");
    }
    match ext.as_str() {
        "eit" | "itr" | "dlx" | "kdc" => Disposition::Container,
        "akc" => delegate("content", "article bodies, metadata and related-article records (akc.md)"),
        "mad" => delegate("atlas", "atlas map data (mad.md)"),
        "ttf" | "fon" => delegate("atlas", "disc fonts (atlas glyphs; reference for the web client)"),
        "lex" => delegate("content", "Megalith speller/fuzzy-lookup lexicons (lex.md)"),
        "sdb" => delegate("content", "world statistics database (msworld.md §1)"),
        "gpd" => omit("empty user pushpin database templates (search-catalog-misc.md §4)"),
        "chm" => omit("help for the original Windows UI; the web client has its own help"),
        "exe" | "dll" | "ocx" | "msi" | "lm8" | "manifest" | "cat" | "policy" | "xtr" | "aw" => {
            omit("Microsoft binary or installer component; never loaded (CLEAN_ROOM rule 6)")
        }
        "cab" if name == "FINDDLX.CAB" => delegate("search", "Find index CAB (unpacked to extracted/finddlx; search.rs)"),
        "cab" => omit("Microsoft installer cabinet; never loaded (CLEAN_ROOM rule 6)"),
        "reg" | "url" | "htm" | "inf" | "cfg" => omit("setup/support configuration, links and registry snippets"),
        "ebk" => omit("installer bookkeeping for the dictionary EITs"),
        "ini" if name == "DELUXE.INI" || name == "KIDSCD.INI" => {
            delegate("content", "catalog package table ([Packages] GUID → file; search-catalog-misc.md §3)")
        }
        "ini" => omit("setup/support configuration"),
        "txt" if name.starts_with('D') && name.len() <= 12 && p.starts_with("EE/ENCARTA/") => {
            delegate("content", "Geography Quiz settings and place pools (msworld.md §2)")
        }
        "txt" => omit("readme, licence and update notes"),
        "dat" if name == "TIMELINE.DAT" => delegate("content", "timeline events (timeline.md)"),
        "dat" if name == "P2MTBL.DAT" => delegate("content", "article → atlas map links (p2mtbl.md)"),
        "db" | "idx" if name.starts_with("MINDMAZE") => delegate("content", "MindMaze question bank (mindmaze.md)"),
        _ if p.starts_with("WINDOWS/") => omit("Microsoft side-by-side runtime component; never loaded (CLEAN_ROOM rule 6)"),
        _ => Disposition::Unexplained,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eit_rules() {
        let img = Disposition::Media { kind: Kind::Image, key: None };
        assert_eq!(classify_member("CONTENT/MDDLX01.EIT", "baggage/00012b7d.jpg", Format::Jpeg), img);
        // Odd Encarta extensions are images by content.
        assert_eq!(classify_member("CONTENT/PICONDLX.EIT", "baggage/00012b6c.gsm", Format::Gif), img);
        assert_eq!(
            classify_member("CONTENT/WORLD/7015/05917.EIT", "Kevin.jpg", Format::Bmp),
            Disposition::Media { kind: Kind::Image, key: Some(ColourKey::Magenta) }
        );
        assert_eq!(
            classify_member("CONTENT/MSWORLD.EIT", "dfg/100.dfg", Format::Bmp),
            Disposition::Media { kind: Kind::Image, key: Some(ColourKey::Green) }
        );
        assert!(matches!(
            classify_member("CONTENT/WORLD/7015/04257.EIT", "acr.x", Format::Other),
            Disposition::Delegated { owner: "tours", .. }
        ));
        assert!(matches!(
            classify_member("CONTENT/MDDLX01.EIT", "_labelw.xml", Format::Xml),
            Disposition::Omitted { .. }
        ));
        assert_eq!(
            classify_member("CONTENT/SWDLX01.EIT", "dswMedia/IAF/a/000a3efc/000A3F0B.MP3", Format::Swa),
            Disposition::Media { kind: Kind::Swa, key: None }
        );
        assert!(matches!(
            classify_member("EE/ENCARTA/SWSHARED.EIT", "dswMedia/shared/players/LOC_vwr/LOC_vwr.dcr", Format::Director),
            Disposition::Omitted { .. }
        ));
        assert_eq!(
            classify_member("EE/ENCARTA/SWALLDSK.EIT", "dswMedia/ia/pop_CPO/dtus.cct", Format::Director),
            Disposition::Media { kind: Kind::Director, key: None }
        );
        assert_eq!(
            classify_member("CONTENT/SWDLX01.EIT", "baggage/0005d840.iax", Format::Xml),
            Disposition::Media { kind: Kind::Interactive, key: None }
        );
        assert_eq!(classify_member("CONTENT/X.EIT", "weird.bin", Format::Other), Disposition::Unexplained);
    }

    #[test]
    fn iso_rules() {
        assert_eq!(
            classify_iso("CONTENT/DSV/00045123.WMV", Format::Asf),
            Disposition::Media { kind: Kind::Video, key: None }
        );
        assert_eq!(classify_iso("CONTENT/MDDLX01.EIT", Format::Other), Disposition::Container);
        assert!(matches!(classify_iso("EE/ENCARTA/ENCARTA.EXE", Format::Other), Disposition::Omitted { .. }));
        assert!(matches!(classify_iso("EE/ENCARTA/D3PLACES.TXT", Format::Other), Disposition::Delegated { .. }));
        assert!(matches!(classify_iso("README.TXT", Format::Other), Disposition::Omitted { .. }));
    }
}
