//! Whole-package verification: decode every record, check lengths, block tiling,
//! UTF-8 and XML well-formedness, and compute the spec's SHA-256 values (§11).

use super::{AkcFile, xml_text};
use anyhow::{Result, bail, ensure};
use quick_xml::XmlVersion;
use quick_xml::events::Event;
use sha2::{Digest, Sha256};
use std::sync::Mutex;

/// What [`check_file`] verifies beyond decoding.
#[derive(Debug, Clone, Copy)]
pub struct Options {
    /// Parse every record as XML.
    pub xml: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options { xml: true }
    }
}

pub type Digest32 = [u8; 32];

#[derive(Debug, Clone, Default)]
pub struct FileReport {
    pub records: usize,
    pub blocks: usize,
    pub direct: usize,
    pub bytes: u64,
    pub largest: u64,
    /// SHA-256 over the concatenated per-record digests, in key order.
    pub aggregate: Digest32,
    /// Per-record SHA-256, in key order (zero for records that failed).
    pub digests: Vec<Digest32>,
    /// Root element's `refid` attribute, per record in key order.
    pub root_refids: Vec<Option<u32>>,
    pub xml_ok: usize,
    pub non_utf8: usize,
    /// Blocks whose members do not tile the token stream exactly.
    pub tiling_errors: usize,
    /// Blocks with 16 or more trailing bits, or non-zero padding.
    pub padding_errors: usize,
    pub literals: u64,
    pub matches: u64,
    pub max_block_tokens: u64,
    /// Codes serialized as pre-tree / sparse / flat, and reuse bits, over all block dictionaries.
    pub codes: super::CodeStats,
    /// First errors (decode, XML or structural), as text.
    pub errors: Vec<String>,
    pub error_count: usize,
}

const MAX_ERRORS: usize = 20;

fn is_xml_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\r' | '\n')
}

/// Checks that `bytes` is one well-formed XML document (an optional UTF-8 BOM, prolog,
/// exactly one root element, matched tags, valid attributes and references). Returns the
/// root element's `refid` attribute when it is a number.
pub fn check_xml(bytes: &[u8]) -> Result<Option<u32>> {
    let text = xml_text(bytes)?;
    let mut r = quick_xml::Reader::from_str(text);
    let cfg = r.config_mut();
    cfg.check_end_names = true;
    cfg.check_comments = true;
    let mut depth = 0usize;
    let mut roots = 0usize;
    let mut refid = None;
    let mut first = true;
    loop {
        let ev = match r.read_event() {
            Ok(e) => e,
            Err(e) => bail!("XML error at byte {}: {e}", r.error_position()),
        };
        match &ev {
            Event::Start(e) | Event::Empty(e) => {
                if depth == 0 {
                    roots += 1;
                    ensure!(roots == 1, "more than one root element");
                }
                for a in e.attributes() {
                    let a = a.map_err(|e| anyhow::anyhow!("bad attribute: {e}"))?;
                    a.normalized_value(XmlVersion::Implicit1_0)
                        .map_err(|e| anyhow::anyhow!("bad attribute value: {e}"))?;
                    if depth == 0 && a.key.as_ref() == "refid" {
                        refid = a.value.parse().ok();
                    }
                }
                if matches!(ev, Event::Start(_)) {
                    depth += 1;
                }
            }
            Event::End(_) => depth -= 1,
            Event::Text(t) => {
                ensure!(
                    depth > 0 || t.chars().all(is_xml_space),
                    "text outside the root element"
                );
            }
            Event::GeneralRef(g) => {
                ensure!(depth > 0, "reference outside the root element");
                if g.is_char_ref() {
                    g.resolve_char_ref()?;
                } else {
                    ensure!(
                        matches!(&**g, "amp" | "lt" | "gt" | "quot" | "apos"),
                        "undeclared entity &{};",
                        &**g
                    );
                }
            }
            Event::CData(_) => ensure!(depth > 0, "CDATA outside the root element"),
            Event::Decl(_) => ensure!(first, "XML declaration not at the start"),
            Event::DocType(_) => ensure!(roots == 0, "DOCTYPE after the root element"),
            Event::Comment(_) | Event::PI(_) => {}
            Event::Eof => break,
        }
        first = false;
    }
    ensure!(roots == 1, "no root element");
    ensure!(depth == 0, "unclosed elements at end");
    Ok(refid)
}

struct Shared {
    rep: FileReport,
}

impl Shared {
    fn error(&mut self, e: String) {
        self.rep.error_count += 1;
        if self.rep.errors.len() < MAX_ERRORS {
            self.rep.errors.push(e);
        }
    }
}

/// Decodes and verifies every record of `f` (parallel over blocks).
pub fn check_file(f: &AkcFile, opts: &Options) -> FileReport {
    use rayon::prelude::*;
    let n = f.len();
    let st = Mutex::new(Shared {
        rep: FileReport {
            records: n,
            blocks: f.blocks().len(),
            direct: f.blocks().iter().filter(|b| b.direct).count(),
            digests: vec![[0; 32]; n],
            root_refids: vec![None; n],
            ..Default::default()
        },
    });
    (0..f.blocks().len() as u32).into_par_iter().for_each(|b| {
        let info = &f.blocks()[b as usize];
        let d = match f.decode_block(b) {
            Ok(d) => d,
            Err(e) => {
                st.lock().unwrap().error(format!("{e:#}"));
                return;
            }
        };
        // Members must tile the token stream exactly, and only padding may follow it.
        let mut next = 0u32;
        let mut tiles = true;
        for &m in &info.members {
            let r = &f.records()[m as usize];
            tiles &= r.start == next;
            next = r.start + r.ntok;
        }
        tiles &= next as usize == d.tokens.len();
        let (trail, zero) = d.trailing_bits;
        let padding_ok = trail < 16 && zero;

        struct Local {
            digest: Digest32,
            pos: usize,
            ulen: u64,
            refid: Option<u32>,
            xml: Result<(), String>,
            utf8: bool,
        }
        let mut local = Vec::with_capacity(info.members.len());
        let mut errs = Vec::new();
        for &m in &info.members {
            let rec = &f.records()[m as usize];
            match f.render(&d, rec) {
                Ok(bytes) => {
                    let digest: Digest32 = Sha256::digest(&bytes).into();
                    let utf8 = std::str::from_utf8(&bytes).is_ok();
                    let (xml, refid) = if opts.xml {
                        match check_xml(&bytes) {
                            Ok(id) => (Ok(()), id),
                            Err(e) => (Err(format!("record {}: {e:#}", rec.key)), None),
                        }
                    } else {
                        (Ok(()), None)
                    };
                    local.push(Local {
                        digest,
                        pos: m as usize,
                        ulen: bytes.len() as u64,
                        refid,
                        xml,
                        utf8,
                    });
                }
                Err(e) => errs.push(format!("{e:#}")),
            }
        }

        let mut s = st.lock().unwrap();
        let rep = &mut s.rep;
        rep.literals += d.stats.literals;
        rep.matches += d.stats.matches;
        rep.max_block_tokens = rep.max_block_tokens.max(d.tokens.len() as u64);
        rep.codes.add(&d.dict.stats);
        if !tiles {
            rep.tiling_errors += 1;
        }
        if !padding_ok {
            rep.padding_errors += 1;
        }
        let mut more = errs;
        for l in local {
            let rep = &mut s.rep;
            rep.digests[l.pos] = l.digest;
            rep.root_refids[l.pos] = l.refid;
            rep.bytes += l.ulen;
            rep.largest = rep.largest.max(l.ulen);
            if !l.utf8 {
                rep.non_utf8 += 1;
            }
            match l.xml {
                Ok(()) if opts.xml => rep.xml_ok += 1,
                Ok(()) => {}
                Err(e) => more.push(e),
            }
        }
        for e in more {
            s.error(e);
        }
        if !tiles {
            s.error(format!("block {b}: members do not tile its token stream"));
        }
        if !padding_ok {
            s.error(format!(
                "block {b}: {trail} trailing bits (all zero: {zero})"
            ));
        }
    });
    let mut rep = st.into_inner().unwrap().rep;
    rep.aggregate = aggregate(&rep.digests);
    rep
}

/// SHA-256 over concatenated binary digests.
pub fn aggregate(digests: &[Digest32]) -> Digest32 {
    let mut h = Sha256::new();
    for d in digests {
        h.update(d);
    }
    h.finalize().into()
}

pub fn hex(d: &[u8]) -> String {
    d.iter().map(|b| format!("{b:02x}")).collect()
}

/// Self-check values from spec §11: `(file, key, ulen, sha256)` of single records.
pub const SPEC_RECORDS: &[(&str, u32, u64, &str)] = &[
    (
        "CONTSTD.AKC",
        22865,
        16_503,
        "c91922f87c3114ce1412e70a3a50bec62212965ee7ac412305a12cb0941b182f",
    ),
    (
        "CONTSTD.AKC",
        11333,
        91_875,
        "f4a57313898db964ae487a41a37a9f27331f22d8a336216a8983c2436a4b94e0",
    ),
    (
        "CONTSTD.AKC",
        0,
        5_242,
        "004d72a2ea74ff044569fdcad654c9af4dc306d5858b5995c82fc4062c049174",
    ),
    (
        "CONTDLX.AKC",
        0,
        16_190,
        "12977be1c348e73fa4933b1df0c5612879a4b357f03ba4907ff0cae038824223",
    ),
    (
        "DATASTD.AKC",
        81969,
        2_567,
        "7b43f187e62a5d7275d73db710f956a19929d6203d0b0e414f7c71aa5a6cad37",
    ),
    (
        "DATASTD.AKC",
        70437,
        10_841,
        "282238a956f34f8671a966f39d8dfc6da2a11d28c6e6fdb4ad22c76225c40c6e",
    ),
    (
        "RELSTD.AKC",
        18691,
        26_655,
        "60ad29310d30ea1ff0059a54c71e4660ce2d22fdd87f744543fc52fdce42e03f",
    ),
    (
        "RELSTD.AKC",
        8667,
        92_461,
        "bbd282210fb6cc0f92bb73064eca03282a9d14e5e93209ea526310a9249e7743",
    ),
    (
        "DATAFSTD.AKC",
        0,
        2_381,
        "e95d937e42ff278de4280894ea882883dfb98ba54b4aee31a60d0431d954f3bb",
    ),
    (
        "DATAESK.AKC",
        0,
        902_785,
        "c4a945de759a8dfaef241f8478553ba4cd32a478a089fdd649994befca99a44e",
    ),
    (
        "DATAFKDC.AKC",
        37,
        280,
        "ed2d8093977c7d2f07a19f762725e40553776ddc57cf894113ff883726e529a8",
    ),
    (
        "CONTKDC.AKC",
        0,
        4_926,
        "f843ec5428d823c20e161d2cbf5e107673d772d393d0eae53236ccbfd344361e",
    ),
];

/// Whole-file values from spec §11: `(file, records, bytes, largest record, aggregate)`.
pub const SPEC_FILES: &[(&str, usize, u64, u64, &str)] = &[
    (
        "CONTDLX.AKC",
        69_949,
        244_017_253,
        241_389,
        "46f58a8660269878e62f9b22f52a044c2ced20c96357262de54b592935806dbc",
    ),
    (
        "CONTSTD.AKC",
        44_055,
        139_646_112,
        545_732,
        "a0d91a6e88244d522e9e8500508e80c0df3ad80b1204d7b8b9c59648ac68929b",
    ),
    (
        "DATADLX.AKC",
        175_793,
        132_029_781,
        109_432,
        "3e90d97ed830468f2912e62dfa2bc442d5a1d39579be0cb92465c697726874b6",
    ),
    (
        "DATASTD.AKC",
        121_055,
        116_438_663,
        70_635,
        "ccdae14250dcdc6269314f88cfc2ee9c73d6c6b76b06ffc6cc93c332fabc1a13",
    ),
    (
        "DATAFDLX.AKC",
        28_112,
        207_266_022,
        997_508,
        "d6097a546250f4172b818dfa656aef683f85adf4afc131395bcb6e32d136928c",
    ),
    (
        "DATAFSTD.AKC",
        152_433,
        841_228_341,
        2_668_109,
        "9a70dcfb2cf4d239bb844eb2cda46bd96bd6cde1b8e67cefe7d74ae8e19ab51d",
    ),
    (
        "RELDLX.AKC",
        970,
        3_824_215,
        168_613,
        "290e8aab2def2d9effc1062f932e1faa93f5ff189795676d27f95b0b58e8285b",
    ),
    (
        "RELSTD.AKC",
        34_606,
        440_840_392,
        4_193_022,
        "07a64ed293545751826eaf4f8a0c62aa818dc40f772c7c7f9a6d2610a3e5f3a3",
    ),
    (
        "DATAESK.AKC",
        1,
        902_785,
        902_785,
        "69b74216c3a297b38295174103c65d02740c7c2b5f93c635414757ded17d09cb",
    ),
    (
        "CONTKDC.AKC",
        637,
        2_903_551,
        9_722,
        "6c2fafc9218c3dbccc956bab55f5811ad9a7b26bca515881e8029662d42d8f2e",
    ),
    (
        "DATAKDC.AKC",
        5_874,
        6_071_915,
        13_998,
        "626a950aee1c2e52719157d0066b8bfe30b5823dceba330fa23fecc3c6d6930a",
    ),
    (
        "DATAFKDC.AKC",
        100,
        38_180,
        980,
        "e54b2a7396ecb753be822387454d997754f44b71a5272d972ad38e7ec49fa972",
    ),
    (
        "RELKDC.AKC",
        663,
        630_171,
        3_282,
        "8850a5cb66da744d12c97b18c8ae3ad8ab5a3b032fb5b86906a695d293446437",
    ),
];

/// Compares a report with the spec's values for `file_name` (case-insensitive).
/// Returns `None` when the spec lists nothing for that file, else the mismatches.
pub fn compare_with_spec(file_name: &str, rep: &FileReport, f: &AkcFile) -> Option<Vec<String>> {
    let mut known = false;
    let mut bad = Vec::new();
    for &(name, records, bytes, largest, agg) in SPEC_FILES {
        if name.eq_ignore_ascii_case(file_name) {
            known = true;
            let got = (rep.records, rep.bytes, rep.largest, hex(&rep.aggregate));
            if got != (records, bytes, largest, agg.to_string()) {
                bad.push(format!(
                    "whole file: got {got:?}, spec {:?}",
                    (records, bytes, largest, agg)
                ));
            }
        }
    }
    for &(name, key, ulen, sha) in SPEC_RECORDS {
        if name.eq_ignore_ascii_case(file_name) {
            known = true;
            let got = f
                .position(key)
                .map(|p| (f.records()[p].ulen as u64, hex(&rep.digests[p])));
            if got != Some((ulen, sha.to_string())) {
                bad.push(format!("record {key}: got {got:?}, spec ({ulen}, {sha})"));
            }
        }
    }
    known.then_some(bad)
}
