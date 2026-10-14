//! SoundFont 2 reader (the public SoundFont 2.04 specification): RIFF `sfbk` with `INFO`, `sdta`
//! (16-bit sample data) and `pdta` (presets, instruments, zones, generators, sample headers).
//!
//! [`SoundFont::regions`] flattens a preset into self-contained regions: each pairs one preset zone
//! with one instrument zone whose key and velocity ranges overlap, with the generators resolved
//! (instrument global → local, plus the preset's additive offsets) as the specification describes.
//! Modulators are not read: the synth applies the specification's default modulators itself.

use anyhow::{Context, Result, bail, ensure};

/// Generator numbers (SoundFont 2.04 §8.1.2).
pub mod op {
    pub const START_OFFSET: usize = 0;
    pub const END_OFFSET: usize = 1;
    pub const LOOP_START_OFFSET: usize = 2;
    pub const LOOP_END_OFFSET: usize = 3;
    pub const START_COARSE: usize = 4;
    pub const MOD_ENV_TO_PITCH: usize = 7;
    pub const FILTER_FC: usize = 8;
    pub const FILTER_Q: usize = 9;
    pub const MOD_ENV_TO_FILTER_FC: usize = 11;
    pub const END_COARSE: usize = 12;
    pub const CHORUS_SEND: usize = 15;
    pub const REVERB_SEND: usize = 16;
    pub const PAN: usize = 17;
    pub const VIB_LFO_TO_PITCH: usize = 6;
    pub const DELAY_VIB_LFO: usize = 23;
    pub const FREQ_VIB_LFO: usize = 24;
    pub const DELAY_MOD_ENV: usize = 25;
    pub const RELEASE_MOD_ENV: usize = 30;
    pub const KEY_TO_MOD_ENV_HOLD: usize = 31;
    pub const KEY_TO_MOD_ENV_DECAY: usize = 32;
    pub const DELAY_VOL_ENV: usize = 33;
    pub const KEY_TO_VOL_ENV_HOLD: usize = 39;
    pub const KEY_TO_VOL_ENV_DECAY: usize = 40;
    pub const INSTRUMENT: usize = 41;
    pub const KEY_RANGE: usize = 43;
    pub const VEL_RANGE: usize = 44;
    pub const LOOP_START_COARSE: usize = 45;
    pub const ATTENUATION: usize = 48;
    pub const LOOP_END_COARSE: usize = 50;
    pub const COARSE_TUNE: usize = 51;
    pub const FINE_TUNE: usize = 52;
    pub const SAMPLE_ID: usize = 53;
    pub const SAMPLE_MODES: usize = 54;
    pub const SCALE_TUNING: usize = 56;
    pub const EXCLUSIVE_CLASS: usize = 57;
    pub const ROOT_KEY: usize = 58;
    pub const COUNT: usize = 61;

    /// Generators that are meaningless at preset level (§8.1.3) and are ignored there.
    pub fn instrument_only(g: usize) -> bool {
        matches!(g, 0..=4 | 12 | 45 | 46 | 47 | 50 | 53 | 54 | 57 | 58)
    }

    /// Default values (§8.1.3); ranges are handled separately.
    pub fn default(g: usize) -> i32 {
        match g {
            FILTER_FC => 13500,
            21 | 23 | 25..=28 | 30 | 33..=36 | 38 => -12000,
            SCALE_TUNING => 100,
            46 | 47 | ROOT_KEY => -1,
            _ => 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Range {
    pub lo: u8,
    pub hi: u8,
}

impl Range {
    pub const FULL: Range = Range { lo: 0, hi: 127 };
    fn intersect(self, o: Range) -> Option<Range> {
        let r = Range { lo: self.lo.max(o.lo), hi: self.hi.min(o.hi) };
        (r.lo <= r.hi).then_some(r)
    }
}

/// One zone: the generators it sets (others are absent) and its ranges.
#[derive(Debug, Clone, Default)]
struct Zone {
    gens: Vec<(usize, i32)>,
    keys: Option<Range>,
    vels: Option<Range>,
}

impl Zone {
    fn get(&self, g: usize) -> Option<i32> {
        self.gens.iter().rev().find(|(n, _)| *n == g).map(|&(_, v)| v)
    }
}

/// A preset or an instrument: an optional global zone and its local zones.
#[derive(Debug, Clone, Default)]
struct ZoneList {
    name: String,
    global: Option<Zone>,
    zones: Vec<Zone>,
}

#[derive(Debug, Clone)]
pub struct Preset {
    pub name: String,
    pub bank: u16,
    pub program: u16,
    zones: ZoneList,
}

#[derive(Debug, Clone)]
pub struct SampleHeader {
    pub name: String,
    pub start: u32,
    pub end: u32,
    pub loop_start: u32,
    pub loop_end: u32,
    pub rate: u32,
    pub original_pitch: u8,
    pub pitch_correction: i8,
    pub kind: u16,
}

impl SampleHeader {
    pub fn len(&self) -> u32 {
        self.end.saturating_sub(self.start)
    }
    /// ROM samples (bit 15) have no data in the file.
    pub fn is_rom(&self) -> bool {
        self.kind & 0x8000 != 0
    }
}

pub struct SoundFont<'a> {
    pub info: Vec<(String, String)>,
    pub presets: Vec<Preset>,
    pub samples: Vec<SampleHeader>,
    instruments: Vec<ZoneList>,
    /// `smpl`: 16-bit little-endian mono sample words.
    smpl: &'a [u8],
}

/// A preset zone × instrument zone with every generator resolved.
#[derive(Debug, Clone, PartialEq)]
pub struct Region {
    pub keys: Range,
    pub vels: Range,
    pub sample: usize,
    /// Generator values: instrument value (local, else global, else default) plus the preset
    /// offset (local, else global) for generators allowed at preset level.
    pub gens: [i32; op::COUNT],
}

impl Region {
    pub fn get(&self, g: usize) -> i32 {
        self.gens[g]
    }
}

fn chunks(b: &[u8]) -> impl Iterator<Item = Result<([u8; 4], &[u8])>> {
    let mut pos = 0usize;
    std::iter::from_fn(move || {
        if pos + 8 > b.len() {
            return None;
        }
        let id: [u8; 4] = b[pos..pos + 4].try_into().unwrap();
        let len = u32::from_le_bytes(b[pos + 4..pos + 8].try_into().unwrap()) as usize;
        let start = pos + 8;
        let Some(body) = b.get(start..start + len) else {
            pos = b.len();
            return Some(Err(anyhow::anyhow!("SF2: chunk {} overruns its list", String::from_utf8_lossy(&id))));
        };
        pos = start + len + (len & 1);
        Some(Ok((id, body)))
    })
}

fn name(b: &[u8]) -> String {
    let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    String::from_utf8_lossy(&b[..end]).trim().to_string()
}

fn u16le(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}
fn u32le(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(b[at..at + 4].try_into().unwrap())
}

fn records<'a>(b: &'a [u8], size: usize, what: &str) -> Result<Vec<&'a [u8]>> {
    ensure!(b.len().is_multiple_of(size) && b.len() >= size, "SF2: {what} is {} bytes, not a multiple of {size}", b.len());
    Ok(b.chunks_exact(size).collect())
}

/// Reads one level of zones: `headers[i]` gives the first bag of item `i` (the terminal record
/// closes the last), each bag the first generator of its zone.
fn zone_lists(headers: &[(String, usize)], bags: &[&[u8]], gens: &[&[u8]], index_gen: usize) -> Result<Vec<ZoneList>> {
    let mut out = Vec::new();
    for w in headers.windows(2) {
        let (name, b0) = (&w[0].0, w[0].1);
        let b1 = w[1].1;
        ensure!(b0 <= b1 && b1 < bags.len(), "SF2: bag indices out of order for {name}");
        let mut list = ZoneList { name: name.clone(), ..Default::default() };
        for bi in b0..b1 {
            let g0 = u16le(bags[bi], 0) as usize;
            let g1 = u16le(bags[bi + 1], 0) as usize;
            ensure!(g0 <= g1 && g1 <= gens.len(), "SF2: generator indices out of order for {name}");
            let mut z = Zone::default();
            for g in &gens[g0..g1] {
                let op = u16le(g, 0) as usize;
                let (lo, hi) = (g[2], g[3]);
                let amount = i16::from_le_bytes([lo, hi]) as i32;
                match op {
                    op::KEY_RANGE => z.keys = Some(Range { lo, hi }),
                    op::VEL_RANGE => z.vels = Some(Range { lo, hi }),
                    // Unsigned word amounts.
                    op::INSTRUMENT | op::SAMPLE_ID => z.gens.push((op, u16::from_le_bytes([lo, hi]) as i32)),
                    op if op < op::COUNT => z.gens.push((op, amount)),
                    _ => {}
                }
            }
            // A zone without its index generator (instrument / sampleID) is global if it comes first.
            if z.get(index_gen).is_none() {
                if bi == b0 && list.global.is_none() {
                    list.global = Some(z);
                }
            } else {
                list.zones.push(z);
            }
        }
        out.push(list);
    }
    Ok(out)
}

impl<'a> SoundFont<'a> {
    pub fn parse(b: &'a [u8]) -> Result<Self> {
        ensure!(b.len() >= 12 && &b[0..4] == b"RIFF" && &b[8..12] == b"sfbk", "not a SoundFont 2 file (RIFF sfbk)");
        let riff_len = (u32le(b, 4) as usize).min(b.len() - 8);
        let mut info = Vec::new();
        let mut smpl: &[u8] = &[];
        let mut pdta = std::collections::HashMap::new();
        for c in chunks(&b[12..8 + riff_len]) {
            let (id, body) = c?;
            if &id != b"LIST" || body.len() < 4 {
                continue;
            }
            let kind = &body[..4];
            for sc in chunks(&body[4..]) {
                let (sid, sbody) = sc?;
                match kind {
                    b"INFO" => info.push((String::from_utf8_lossy(&sid).into_owned(), name(sbody))),
                    b"sdta" if &sid == b"smpl" => smpl = sbody,
                    b"pdta" => {
                        pdta.insert(sid, sbody);
                    }
                    _ => {}
                }
            }
        }
        let get = |id: &[u8; 4]| pdta.get(id).copied().with_context(|| format!("SF2: missing pdta {}", String::from_utf8_lossy(id)));
        let phdr = records(get(b"phdr")?, 38, "phdr")?;
        let pbag = records(get(b"pbag")?, 4, "pbag")?;
        let pgen = records(get(b"pgen")?, 4, "pgen")?;
        let inst = records(get(b"inst")?, 22, "inst")?;
        let ibag = records(get(b"ibag")?, 4, "ibag")?;
        let igen = records(get(b"igen")?, 4, "igen")?;
        let shdr = records(get(b"shdr")?, 46, "shdr")?;

        let inst_heads: Vec<(String, usize)> = inst.iter().map(|r| (name(&r[..20]), u16le(r, 20) as usize)).collect();
        let instruments = zone_lists(&inst_heads, &ibag, &igen, op::SAMPLE_ID)?;
        let preset_heads: Vec<(String, usize)> = phdr.iter().map(|r| (name(&r[..20]), u16le(r, 24) as usize)).collect();
        let preset_zones = zone_lists(&preset_heads, &pbag, &pgen, op::INSTRUMENT)?;
        let presets = phdr
            .iter()
            .zip(preset_zones)
            .map(|(r, zones)| Preset { name: zones.name.clone(), program: u16le(r, 20), bank: u16le(r, 22), zones })
            .collect();
        let samples = shdr[..shdr.len() - 1]
            .iter()
            .map(|r| SampleHeader {
                name: name(&r[..20]),
                start: u32le(r, 20),
                end: u32le(r, 24),
                loop_start: u32le(r, 28),
                loop_end: u32le(r, 32),
                rate: u32le(r, 36),
                original_pitch: r[40],
                pitch_correction: r[41] as i8,
                kind: u16le(r, 44),
            })
            .collect();
        Ok(SoundFont { info, presets, samples, instruments, smpl })
    }

    pub fn info(&self, id: &str) -> Option<&str> {
        self.info.iter().find(|(k, _)| k == id).map(|(_, v)| v.as_str())
    }

    /// The sample's 16-bit words, `start..end`.
    pub fn sample_data(&self, s: &SampleHeader) -> Result<&'a [u8]> {
        let (a, z) = (s.start as usize * 2, s.end as usize * 2);
        if a > z || z > self.smpl.len() {
            bail!("SF2: sample {} ({}..{}) outside smpl ({} words)", s.name, s.start, s.end, self.smpl.len() / 2);
        }
        Ok(&self.smpl[a..z])
    }

    /// Every region of a preset, in zone order.
    pub fn regions(&self, p: &Preset) -> Vec<Region> {
        let mut out = Vec::new();
        let pg = p.zones.global.as_ref();
        for pz in &p.zones.zones {
            let Some(inst) = pz.get(op::INSTRUMENT).and_then(|i| self.instruments.get(i as usize)) else { continue };
            let pkeys = pz.keys.or(pg.and_then(|g| g.keys)).unwrap_or(Range::FULL);
            let pvels = pz.vels.or(pg.and_then(|g| g.vels)).unwrap_or(Range::FULL);
            let ig = inst.global.as_ref();
            for iz in &inst.zones {
                let Some(sample) = iz.get(op::SAMPLE_ID).map(|s| s as usize) else { continue };
                if self.samples.get(sample).is_none_or(|s| s.is_rom()) {
                    continue;
                }
                let ikeys = iz.keys.or(ig.and_then(|g| g.keys)).unwrap_or(Range::FULL);
                let ivels = iz.vels.or(ig.and_then(|g| g.vels)).unwrap_or(Range::FULL);
                let (Some(keys), Some(vels)) = (pkeys.intersect(ikeys), pvels.intersect(ivels)) else { continue };
                let mut gens = [0i32; op::COUNT];
                for (g, v) in gens.iter_mut().enumerate() {
                    let base = iz.get(g).or(ig.and_then(|z| z.get(g))).unwrap_or(op::default(g));
                    let offset = if op::instrument_only(g) { 0 } else { pz.get(g).or(pg.and_then(|z| z.get(g))).unwrap_or(0) };
                    *v = base + offset;
                }
                gens[op::SAMPLE_ID] = sample as i32;
                out.push(Region { keys, vels, sample, gens });
            }
        }
        out
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    fn chunk(id: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut v = id.to_vec();
        v.extend((body.len() as u32).to_le_bytes());
        v.extend(body);
        if body.len() % 2 == 1 {
            v.push(0);
        }
        v
    }
    fn list(kind: &[u8; 4], parts: &[Vec<u8>]) -> Vec<u8> {
        let mut body = kind.to_vec();
        for p in parts {
            body.extend(p);
        }
        chunk(b"LIST", &body)
    }
    fn name20(s: &str) -> Vec<u8> {
        let mut v = s.as_bytes().to_vec();
        v.resize(20, 0);
        v
    }
    fn g(op: u16, amount: i16) -> Vec<u8> {
        let mut v = op.to_le_bytes().to_vec();
        v.extend(amount.to_le_bytes());
        v
    }
    fn range(op: u16, lo: u8, hi: u8) -> Vec<u8> {
        let mut v = op.to_le_bytes().to_vec();
        v.extend([lo, hi]);
        v
    }
    fn bag(gen_index: u16) -> Vec<u8> {
        let mut v = gen_index.to_le_bytes().to_vec();
        v.extend(0u16.to_le_bytes());
        v
    }

    /// A tiny synthetic SoundFont: one preset (bank 0, program 5) with a global zone (attenuation
    /// offset +20 cB) and one zone over keys 40–90 → instrument "Inst" whose global zone sets a
    /// release, with two zones: keys 0–59 → sample 0, keys 60–127 → sample 1 (root key override 72,
    /// looped). Samples are 100 words each.
    pub fn synthetic() -> Vec<u8> {
        let mut smpl = Vec::new();
        for i in 0..(100 + 46 + 100 + 46) {
            smpl.extend(((i as i16).wrapping_mul(97)).to_le_bytes());
        }
        let phdr = [
            [name20("Test Preset"), 5u16.to_le_bytes().to_vec(), 0u16.to_le_bytes().to_vec(), 0u16.to_le_bytes().to_vec(), vec![0; 12]].concat(),
            [name20("EOP"), vec![0; 4], 2u16.to_le_bytes().to_vec(), vec![0; 12]].concat(),
        ]
        .concat();
        let pbag = [bag(0), bag(1), bag(3)].concat();
        let pgen = [g(48, 20), range(43, 40, 90), g(41, 0), g(0, 0)].concat();
        let inst = [[name20("Inst"), 0u16.to_le_bytes().to_vec()].concat(), [name20("EOI"), 3u16.to_le_bytes().to_vec()].concat()].concat();
        let ibag = [bag(0), bag(1), bag(3), bag(7)].concat();
        let igen = [g(38, -1200), range(43, 0, 59), g(53, 0), range(43, 60, 127), g(58, 72), g(54, 1), g(53, 1), g(0, 0)].concat();
        let sh = |n: &str, start: u32, end: u32, ls: u32, le: u32, pitch: u8| {
            [name20(n), start.to_le_bytes().to_vec(), end.to_le_bytes().to_vec(), ls.to_le_bytes().to_vec(), le.to_le_bytes().to_vec(), 22050u32.to_le_bytes().to_vec(), vec![pitch, (-3i8) as u8], vec![0, 0], 1u16.to_le_bytes().to_vec()].concat()
        };
        let shdr = [sh("Low", 0, 100, 10, 90, 48), sh("High", 146, 246, 156, 236, 60), sh("EOS", 0, 0, 0, 0, 0)].concat();
        let body = [
            b"sfbk".to_vec(),
            list(b"INFO", &[chunk(b"ifil", &[2, 0, 1, 0]), chunk(b"INAM", b"Synthetic\0"), chunk(b"ICOP", b"Ours\0")]),
            list(b"sdta", &[chunk(b"smpl", &smpl)]),
            list(
                b"pdta",
                &[
                    chunk(b"phdr", &phdr),
                    chunk(b"pbag", &pbag),
                    chunk(b"pmod", &[0; 10]),
                    chunk(b"pgen", &pgen),
                    chunk(b"inst", &inst),
                    chunk(b"ibag", &ibag),
                    chunk(b"imod", &[0; 10]),
                    chunk(b"igen", &igen),
                    chunk(b"shdr", &shdr),
                ],
            ),
        ]
        .concat();
        chunk(b"RIFF", &body)
    }

    #[test]
    fn parses_and_flattens() {
        let b = synthetic();
        let sf = SoundFont::parse(&b).unwrap();
        assert_eq!(sf.info("INAM"), Some("Synthetic"));
        assert_eq!(sf.presets.len(), 1); // the EOP terminal record is not a preset
        let p = &sf.presets[0];
        assert_eq!((p.name.as_str(), p.bank, p.program), ("Test Preset", 0, 5));
        assert_eq!(sf.samples.len(), 2);
        assert_eq!(sf.samples[1].pitch_correction, -3);
        assert_eq!(sf.sample_data(&sf.samples[1]).unwrap().len(), 200);

        let r = sf.regions(p);
        assert_eq!(r.len(), 2);
        // Key ranges intersect with the preset zone's 40–90.
        assert_eq!(r[0].keys, Range { lo: 40, hi: 59 });
        assert_eq!(r[1].keys, Range { lo: 60, hi: 90 });
        assert_eq!(r[0].vels, Range::FULL);
        assert_eq!((r[0].sample, r[1].sample), (0, 1));
        // Instrument global release applies to both; preset global attenuation adds.
        assert_eq!(r[0].get(op::DELAY_VOL_ENV + 5), -1200);
        assert_eq!(r[1].get(op::ATTENUATION), 20);
        // Defaults and local values.
        assert_eq!(r[0].get(op::ROOT_KEY), -1);
        assert_eq!(r[1].get(op::ROOT_KEY), 72);
        assert_eq!(r[1].get(op::SAMPLE_MODES), 1);
        assert_eq!(r[0].get(op::FILTER_FC), 13500);
        assert_eq!(r[0].get(op::SCALE_TUNING), 100);
        assert_eq!(r[0].get(op::DELAY_VOL_ENV + 1), -12000);
    }

    #[test]
    fn rejects_non_sf2() {
        assert!(SoundFont::parse(b"RIFF\x04\0\0\0WAVE").is_err());
    }
}
