//! findidx [--dir DIR] [--kids] <command>
//!
//!   pfx            word-wheel entries (both collections), JSON lines
//!   idxe           IdXE records reachable from the postings (the live records), JSON lines
//!   idxe-walk      every IdXE record found by a sequential 16-byte-aligned walk
//!   idxw           vocabulary keys in B-tree order, JSON lines
//!   idxew          postings per distinct word id, JSON lines
//!   idxcm          character map (header line, then one line per char)
//!   word <key>     postings of one vocabulary key, with each hit's IdXE display text
//!   stats          counts and cross-checks over all five files
//!
//! DIR defaults to $ENCARTA_EXTRACTED/finddlx (else extracted/finddlx); --kids selects the KDC set.

use anyhow::{Context, Result, bail};
use encarta_formats::search::*;
use serde::Serialize;
use serde_json::json;
use std::collections::{BTreeMap, HashSet};
use std::io::{BufWriter, Write};
use std::path::PathBuf;

/// True when stdout was closed early (e.g. piped into `head`).
fn is_broken_pipe(e: &anyhow::Error) -> bool {
    use std::io::ErrorKind::BrokenPipe;
    e.chain().any(|c| {
        c.downcast_ref::<std::io::Error>()
            .is_some_and(|io| io.kind() == BrokenPipe)
            || c.downcast_ref::<serde_json::Error>()
                .and_then(|j| j.io_error_kind())
                == Some(BrokenPipe)
    })
}

fn main() {
    if let Err(e) = run() {
        if is_broken_pipe(&e) {
            return;
        }
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}

fn read(p: &PathBuf) -> Result<Vec<u8>> {
    std::fs::read(p).with_context(|| format!("reading {}", p.display()))
}

fn line<W: Write, T: Serialize>(out: &mut W, v: &T) -> Result<()> {
    serde_json::to_writer(&mut *out, v)?;
    out.write_all(b"\n")?;
    Ok(())
}

fn run() -> Result<()> {
    let extracted = std::env::var_os("ENCARTA_EXTRACTED")
        .map(PathBuf::from)
        .unwrap_or("extracted".into());
    let mut dir = extracted.join("finddlx");
    let mut tla = "DLX";
    let mut rest = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--dir" => {
                dir = args
                    .next()
                    .map(PathBuf::from)
                    .context("--dir needs a value")?
            }
            "--kids" => tla = "KDC",
            _ => rest.push(a),
        }
    }
    let files = FindFiles::locate(&dir, tla)?;
    let mut out = BufWriter::new(std::io::stdout().lock());
    match rest
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["pfx"] => {
            let pfx = Pfx::parse(&read(&files.pfx)?)?;
            for (i, c) in pfx.collections.iter().enumerate() {
                for e in &c.entries {
                    line(
                        &mut out,
                        &json!({"collection": i, "ref_id": e.ref_id, "class_id": e.class_id,
                        "sort_title": e.sort_title, "display_title": e.display_title}),
                    )?;
                }
            }
        }
        ["idxw"] => {
            let b = read(&files.idxw)?;
            for k in IdxW::new(&b)?.walk()?.0 {
                line(&mut out, &k)?;
            }
        }
        ["idxew"] => {
            let (wb, eb) = (read(&files.idxw)?, read(&files.idxew)?);
            let ew = IdxEw::new(&eb)?;
            for w in distinct_word_ids(&IdxW::new(&wb)?.walk()?.0) {
                line(&mut out, &json!({"word_id": w, "alts": ew.postings(w)?}))?;
            }
        }
        ["idxe"] => {
            let (wb, eb, xb) = (read(&files.idxw)?, read(&files.idxew)?, read(&files.idxe)?);
            let ew = IdxEw::new(&eb)?;
            let live = live_entry_ids(&ew, &distinct_word_ids(&IdxW::new(&wb)?.walk()?.0))?;
            drop(eb);
            let idxe = IdxE::new(&xb)?;
            for id in live {
                line(&mut out, &idxe.record(id)?)?;
            }
        }
        ["idxe-walk"] => {
            let xb = read(&files.idxe)?;
            let idxe = IdxE::new(&xb)?;
            let mut walk = idxe.walk();
            for r in walk.by_ref() {
                line(&mut out, &r)?;
            }
            eprintln!("unparseable slots skipped: {}", walk.failures);
        }
        ["idxcm"] => {
            let m = CharMap::parse(&read(&files.idxcm)?)?;
            line(
                &mut out,
                &json!({"culture": m.culture, "answers": m.answers, "count": m.entries.len()}),
            )?;
            for e in &m.entries {
                line(&mut out, e)?;
            }
        }
        ["word", key] => {
            let (wb, eb, xb) = (read(&files.idxw)?, read(&files.idxew)?, read(&files.idxe)?);
            let (ew, idxe) = (IdxEw::new(&eb)?, IdxE::new(&xb)?);
            let key = key.to_lowercase();
            let words = IdxW::new(&wb)?.walk()?.0;
            let Some(w) = words.iter().find(|w| w.key == key) else {
                bail!("{key:?} is not in the vocabulary")
            };
            line(&mut out, w)?;
            let text = |id: u32| {
                idxe.record(id)
                    .map(|r| r.display_text)
                    .unwrap_or_else(|e| format!("<{e}>"))
            };
            for alt in ew.postings(w.word_id)? {
                for h in &alt.t1 {
                    line(
                        &mut out,
                        &json!({"alt": alt.alt_id, "tier": 1, "hit": h, "text": text(h.entry_id)}),
                    )?;
                }
                for h in &alt.t2 {
                    line(
                        &mut out,
                        &json!({"alt": alt.alt_id, "tier": 2, "hit": h, "text": text(h.entry_id)}),
                    )?;
                }
                for h in &alt.t3 {
                    line(
                        &mut out,
                        &json!({"alt": alt.alt_id, "tier": 3, "hit": h, "text": text(h.entry_id)}),
                    )?;
                }
            }
        }
        ["stats"] => {
            let s = stats(&files)?;
            serde_json::to_writer_pretty(&mut out, &s)?;
            writeln!(out)?;
        }
        _ => bail!(
            "usage: findidx [--dir DIR] [--kids] <pfx|idxe|idxe-walk|idxw|idxew|idxcm|word KEY|stats>"
        ),
    }
    out.flush()?;
    Ok(())
}

fn stats(files: &FindFiles) -> Result<serde_json::Value> {
    // PFX
    let pfx = Pfx::parse(&read(&files.pfx)?)?;
    let pfx_stats: Vec<_> = pfx
        .collections
        .iter()
        .map(|c| {
            let ids: HashSet<u32> = c.entries.iter().map(|e| e.ref_id).collect();
            let mut classes: BTreeMap<u16, usize> = BTreeMap::new();
            for e in &c.entries {
                *classes.entry(e.class_id).or_default() += 1;
            }
            let articles: HashSet<u32> = c.entries.iter().filter(|e| e.class_id == 1).map(|e| e.ref_id).collect();
            let unsorted = c.entries.windows(2).filter(|w| w[0].sort_title > w[1].sort_title).count();
            let unsorted_ci = c
                .entries
                .windows(2)
                .filter(|w| w[0].sort_title.to_lowercase() > w[1].sort_title.to_lowercase())
                .count();
            json!({"lcid": format!("{:#x}", c.lcid), "entries": c.entries.len(), "distinct_ref_ids": ids.len(),
                "distinct_class1_ref_ids": articles.len(), "class_ids": classes,
                "sort_inversions_ordinal": unsorted, "sort_inversions_lowercase": unsorted_ci})
        })
        .collect();
    drop(pfx);

    // IdxW
    let wb = read(&files.idxw)?;
    let idxw = IdxW::new(&wb)?;
    let (words, bt) = idxw.walk()?;
    let word_ids = distinct_word_ids(&words);
    let key_inversions = words.windows(2).filter(|w| w[0].key >= w[1].key).count();
    let deleted = words.iter().filter(|w| w.deleted).count();
    let stop_words = words.iter().filter(|w| w.stop_word_type != 0).count();
    let no_postings = words.iter().filter(|w| !w.has_postings()).count();
    let no_postings_non_stop = words
        .iter()
        .filter(|w| !w.has_postings() && w.stop_word_type == 0)
        .count();
    let idxw_stats = json!({"header": idxw.header, "file_len": wb.len(), "btree": bt, "keys": words.len(),
        "deleted": deleted, "stop_word_keys": stop_words, "keys_without_postings": no_postings,
        "keys_without_postings_not_stop_words": no_postings_non_stop, "distinct_word_ids": word_ids.len(),
        "key_order_inversions_ordinal": key_inversions});

    // IdXEW: blocks should tile the file back to back, in word-id order.
    let eb = read(&files.idxew)?;
    let ew = IdxEw::new(&eb)?;
    let (mut gaps, mut overlaps, mut gap_bytes) = (0u64, 0u64, 0u64);
    let mut expect = ew.data_start;
    let (mut n_alts, mut n1, mut n2, mut n3, mut t1_bit31, mut t2_flag, mut t3_flag) =
        (0u64, 0u64, 0u64, 0u64, 0u64, 0u64, 0u64);
    let idxe_bytes = read(&files.idxe)?;
    let idxe = IdxE::new(&idxe_bytes)?;
    let slots = idxe.slot_count() as usize;
    // refId each live entry is posted under (0 = not posted); conflicting postings counted.
    let mut posted_ref: Vec<u32> = vec![0; slots];
    let mut conflicting_refs = 0u64;
    let mut out_of_range = 0u64;
    let mut alt_ids_by_word: BTreeMap<u32, Vec<u8>> = BTreeMap::new();
    for &w in &word_ids {
        let w_us = w as usize;
        if w_us > expect {
            gaps += 1;
            gap_bytes += (w_us - expect) as u64;
        } else if w_us < expect {
            overlaps += 1;
        }
        let (alts, end) = ew.postings_at(w)?;
        expect = end;
        alt_ids_by_word.insert(w, alts.iter().map(|a| a.alt_id).collect());
        for a in &alts {
            n_alts += 1;
            n1 += a.t1.len() as u64;
            n2 += a.t2.len() as u64;
            n3 += a.t3.len() as u64;
            t1_bit31 += a.t1.iter().filter(|h| h.ref_id >> 31 != 0).count() as u64;
            t2_flag += a.t2.iter().filter(|h| h.flag).count() as u64;
            t3_flag += a.t3.iter().filter(|h| h.flag).count() as u64;
            let refs = a.t1.iter().map(|h| (h.entry_id, h.ref_id & 0x7FFF_FFFF));
            let refs = refs.chain(a.t2.iter().map(|h| (h.entry_id, h.ref_id)));
            let refs = refs.chain(a.t3.iter().map(|h| (h.entry_id, h.ref_id)));
            for (e, r) in refs {
                let Some(slot) = posted_ref.get_mut(e as usize) else {
                    out_of_range += 1;
                    continue;
                };
                if *slot == 0 {
                    *slot = r.max(1);
                } else if *slot != r.max(1) {
                    conflicting_refs += 1;
                }
            }
        }
    }
    let tail = eb.len() as i64 - expect as i64;
    let alt_mismatch = words
        .iter()
        .filter(|w| {
            w.has_postings()
                && !alt_ids_by_word
                    .get(&w.word_id)
                    .is_some_and(|a| a.contains(&w.alt_id))
        })
        .count();
    let idxew_stats = json!({"version": ew.version, "flags": ew.flags, "file_len": eb.len(),
        "blocks": word_ids.len(), "alts": n_alts, "t1": n1, "t2": n2, "t3": n3,
        "t1_refid_bit31": t1_bit31, "t2_flag": t2_flag, "t3_flag": t3_flag,
        "gaps_between_blocks": gaps, "gap_bytes": gap_bytes, "overlapping_blocks": overlaps,
        "bytes_after_last_block": tail, "entry_ids_out_of_range": out_of_range,
        "entry_ids_with_conflicting_refids": conflicting_refs,
        "keys_whose_alt_has_no_hits_in_block": alt_mismatch});
    drop(eb);
    drop(words);

    // IdXE: live records via postings, then the sequential walk.
    let mut live = 0u64;
    let mut parse_failures = 0u64;
    let mut ref_mismatch = 0u64;
    let mut types: BTreeMap<u8, u64> = BTreeMap::new();
    let mut article_ids: HashSet<i32> = HashSet::new();
    let mut live_ref_ids: HashSet<i32> = HashSet::new();
    let mut live_set = vec![false; slots];
    for (e, &r) in posted_ref.iter().enumerate() {
        if r == 0 {
            continue;
        }
        live += 1;
        live_set[e] = true;
        match idxe.record(e as u32) {
            Ok(rec) => {
                if (rec.ref_id as u32).max(1) != r {
                    ref_mismatch += 1;
                }
                *types.entry(rec.content_type()).or_default() += 1;
                live_ref_ids.insert(rec.ref_id);
                let p = rec.ref_id / 10_000_000;
                if p == 76 {
                    article_ids.insert(rec.ref_id);
                }
            }
            Err(_) => parse_failures += 1,
        }
    }
    let mut walk = idxe.walk();
    let (mut walked, mut walked_live) = (0u64, 0u64);
    let mut unposted = Vec::new();
    for r in walk.by_ref() {
        walked += 1;
        if live_set[r.entry_id as usize] {
            walked_live += 1;
        } else if unposted.len() < 20 {
            unposted.push(
                json!({"entry_id": r.entry_id, "ref_id": r.ref_id, "type": r.type_byte,
                "text": r.display_text}),
            );
        }
    }
    let idxe_stats = json!({"version": idxe.header.version, "file_len": idxe_bytes.len(),
        "first_entry_id": idxe.first_entry_id(), "live_records": live, "live_parse_failures": parse_failures,
        "posting_refid_mismatches": ref_mismatch, "distinct_ref_ids": live_ref_ids.len(),
        "distinct_761_762_ids": article_ids.len(), "content_types": types,
        "walk_records": walked, "walk_unparseable_slots": walk.failures, "walk_last_record_end": walk.last_end, "walk_records_live": walked_live,
        "walk_records_unposted": walked - walked_live, "walk_unposted_samples": unposted,
        "live_records_missed_by_walk": live - walked_live});

    let m = CharMap::parse(&read(&files.idxcm)?)?;
    let cm = json!({"culture": m.culture, "answers": m.answers, "entries": m.entries.len()});

    Ok(
        json!({"files": {"pfx": files.pfx, "idxe": files.idxe, "idxw": files.idxw, "idxew": files.idxew,
        "idxcm": files.idxcm}, "pfx": pfx_stats, "idxw": idxw_stats, "idxew": idxew_stats, "idxe": idxe_stats,
        "idxcm": cm}),
    )
}
