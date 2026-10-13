//! MAD atlas database (Encarta Virtual Globe), from docs/formats/mad.md.
//!
//! A database is two files, `<name>_HD.MAD` (indexes, styles, small tables) and `<name>_CD.MAD`
//! (rasters and bulk grids), joined by section name into one [`Atlas`].
//!
//! Layers, bottom up:
//! - [`toc`]: file header and section list;
//! - [`index`]: `INDX` key → record maps;
//! - [`huffman`], [`bits`], [`grid`]: the field codec shared by every Grid table;
//! - [`rmap`]: the raster pyramid (`Terrain`), with [`octahedral`] for its projection;
//! - [`schemas`]: record layouts of the Grid tables; [`tables`]: the plain tables;
//!   [`cells`]: the spatial cell keys of the vector tables; [`g1g2`]: the place-name encoding;
//! - [`gazetteer`]: joins entities across GeoObj, EntityConv, Find and EntityCenter;
//! - [`check`]: whole-database validation.

pub mod bits;
pub mod cells;
pub mod check;
pub mod g1g2;
pub mod gazetteer;
pub mod grid;
pub mod huffman;
pub mod index;
pub mod octahedral;
pub mod rmap;
pub mod schemas;
pub mod tables;
pub mod toc;

use anyhow::{Context, Result};
use memmap2::Mmap;
use std::path::{Path, PathBuf};

pub use grid::Grid;
pub use index::Index;

/// One mapped MAD file.
pub struct MadFile {
    pub path: PathBuf,
    pub toc: toc::Toc,
    map: Mmap,
}

impl MadFile {
    pub fn open(path: &Path) -> Result<MadFile> {
        let f = std::fs::File::open(path).with_context(|| format!("opening {}", path.display()))?;
        // SAFETY: the disc files are read-only inputs that are not modified while mapped.
        let map = unsafe { Mmap::map(&f) }.with_context(|| format!("mapping {}", path.display()))?;
        let toc = toc::parse_toc(&map, map.len() as u64).with_context(|| format!("{}", path.display()))?;
        Ok(MadFile { path: path.to_path_buf(), toc, map })
    }

    pub fn section(&self, name: &str) -> Option<&[u8]> {
        let s = self.toc.sections.iter().find(|s| s.name == name)?;
        Some(&self.map[s.offset as usize..(s.offset + s.length) as usize])
    }

    pub fn bytes(&self) -> &[u8] {
        &self.map
    }
}

/// Which of the two shipped databases.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edition {
    /// `WLD`: the full world atlas.
    World,
    /// `SWLD`: the limited atlas.
    Student,
}

impl Edition {
    pub fn prefix(self) -> &'static str {
        match self {
            Edition::World => "WLD",
            Edition::Student => "SWLD",
        }
    }

    pub fn parse(s: &str) -> Option<Edition> {
        match s.to_ascii_uppercase().as_str() {
            "WLD" | "WORLD" => Some(Edition::World),
            "SWLD" | "STUDENT" => Some(Edition::Student),
            _ => None,
        }
    }
}

/// Grid tables and the index section that locates their records.
pub const GRID_PAIRS: &[(&str, &str)] = &[
    ("GeoGemGrid", "GeoGemLookup"),
    ("EntityConv", "EntityConvLookup"),
    ("Bitmap", "BitmapLookup"),
    ("LegendGrid", "LegendLookup"),
    ("PDCAltitudeGrid", "PDCAltitudeLookup"),
    ("MillerTerrainGrid", "MillerTerrainLku"),
];

/// Grid tables with a language suffix, and their index sections (same suffix).
pub const LCID_GRID_PAIRS: &[(&str, &str)] = &[
    ("GeoObjGrid", "GeoObjLookup"),
    ("Find Grid", "Find Ndx"),
    ("FindGECContextGrid", "FindGECContextLookup"),
];

/// Section name with its 4-hex-digit language suffix (`GeoObjGrid0409`).
pub fn lcid_name(base: &str, lcid: u16) -> String {
    format!("{base}{lcid:04x}")
}

/// One logical database made of any number of MAD files (normally the HD and CD halves).
pub struct Atlas {
    pub files: Vec<MadFile>,
}

impl Atlas {
    pub fn open(paths: &[PathBuf]) -> Result<Atlas> {
        Ok(Atlas { files: paths.iter().map(|p| MadFile::open(p)).collect::<Result<_>>()? })
    }

    /// The HD and CD files of `edition` under an extracted disc (`<root>/iso/…`). File names are
    /// matched case-insensitively.
    pub fn locate(extracted: &Path, edition: Edition) -> Result<Vec<PathBuf>> {
        let p = edition.prefix();
        let hd = find_ci(&extracted.join("iso/EE/ENCARTA"), &format!("{p}_HD.MAD"))?;
        let cd = find_ci(&extracted.join("iso/CONTENT/MAD"), &format!("{p}_CD.MAD"))
            .or_else(|_| find_ci(&extracted.join("iso/EE/ENCARTA"), &format!("{p}_CD.MAD")))?;
        Ok(vec![hd, cd])
    }

    pub fn open_edition(extracted: &Path, edition: Edition) -> Result<Atlas> {
        Self::open(&Self::locate(extracted, edition)?)
    }

    pub fn section(&self, name: &str) -> Option<&[u8]> {
        self.files.iter().find_map(|f| f.section(name))
    }

    pub fn require(&self, name: &str) -> Result<&[u8]> {
        self.section(name).with_context(|| format!("section {name:?} not in the database"))
    }

    pub fn section_names(&self) -> Vec<String> {
        self.files.iter().flat_map(|f| f.toc.sections.iter().map(|s| s.name.clone())).collect()
    }

    /// The LCIDs listed in `LangId`.
    pub fn languages(&self) -> Result<Vec<u16>> {
        tables::lang_ids(self.require("LangId")?)
    }

    pub fn index(&self, name: &str) -> Result<Index> {
        Index::parse(self.require(name)?).with_context(|| format!("index {name}"))
    }

    pub fn grid(&self, grid: &str, index: &str) -> Result<Grid<'_>> {
        Grid::new(grid, self.require(grid)?, self.index(index)?)
    }

    /// A grid by table name, with or without a language suffix (`GeoObjGrid0409`, `LegendGrid`).
    pub fn grid_named(&self, name: &str) -> Result<Grid<'_>> {
        if let Some((g, i)) = GRID_PAIRS.iter().find(|(g, _)| *g == name) {
            return self.grid(g, i);
        }
        for (g, i) in LCID_GRID_PAIRS {
            if let Some(suffix) = name.strip_prefix(g)
                && suffix.len() == 4
            {
                return self.grid(name, &format!("{i}{suffix}"));
            }
        }
        anyhow::bail!("{name} is not a known grid table")
    }

    pub fn geo_gem(&self) -> Result<Grid<'_>> {
        self.grid("GeoGemGrid", "GeoGemLookup")
    }

    pub fn geo_obj(&self, lcid: u16) -> Result<Grid<'_>> {
        self.grid(&lcid_name("GeoObjGrid", lcid), &lcid_name("GeoObjLookup", lcid))
    }

    pub fn find_grid(&self, lcid: u16) -> Result<Grid<'_>> {
        self.grid(&lcid_name("Find Grid", lcid), &lcid_name("Find Ndx", lcid))
    }

    pub fn terrain(&self) -> Result<rmap::Terrain<'_>> {
        rmap::Terrain::parse(self.require("Terrain")?)
    }

    pub fn classes(&self) -> Result<Vec<tables::FeatureClass>> {
        tables::pdc_attribute(self.require("PDCAttribute")?)
    }

    pub fn class_kinds(&self) -> Result<Vec<tables::GeometryKind>> {
        Ok(self.classes()?.into_iter().map(|c| c.kind).collect())
    }

    pub fn cells(&self) -> Result<cells::CellSystem> {
        cells::CellSystem::parse(self.require("GeoFaceScale")?)
    }

    pub fn g1g2(&self) -> Result<g1g2::G1G2> {
        g1g2::G1G2::parse(self.require("G1G2 to Unicode")?)
    }

    pub fn find_data(&self, lcid: u16) -> Result<tables::FindData> {
        tables::find_data(self.require(&lcid_name("Find Data", lcid))?, &self.g1g2()?)
    }

    pub fn entity_centers(&self) -> Result<Vec<tables::EntityCenter>> {
        tables::entity_centers(self.require("EntityCenter")?)
    }

    pub fn map_styles(&self) -> Result<Vec<tables::MapStyle>> {
        tables::map_styles(self.require("MapStyle")?)
    }

    pub fn bitmap_palette(&self) -> Result<Vec<tables::PaletteEntry>> {
        tables::bitmap_palette(self.require("BitmapPalette")?)
    }
}

fn find_ci(dir: &Path, name: &str) -> Result<PathBuf> {
    let exact = dir.join(name);
    if exact.is_file() {
        return Ok(exact);
    }
    let rd = std::fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))?;
    for e in rd.flatten() {
        if e.file_name().to_string_lossy().eq_ignore_ascii_case(name) {
            return Ok(e.path());
        }
    }
    anyhow::bail!("{name} not found in {}", dir.display())
}
