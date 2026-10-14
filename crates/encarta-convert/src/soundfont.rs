//! `encarta-convert soundfont [--sf2 FILE]`: writes the General MIDI soundfont the web client's MIDI
//! synth plays (docs/media.md §8) to `out/soundfont/`. Not disc content: the source is a freely
//! licensed SF2, by default Fluid R3 GM (MIT) at `/usr/share/soundfonts/FluidR3_GM.sf2`
//! (Fedora `fluid-soundfont-gm`); `ENCARTA_SF2` or `--sf2` picks another.
//!
//! Output: `index.json` (name, copyright, licence, presets), one `presets/{bank}-{program}.json` per
//! General MIDI preset (bank 0 melodic, bank 128 drum kits) with its flattened regions, and one
//! lossless `samples/{id}.flac` per sample those presets use (loop points stay sample-exact).

mod sf2;

use crate::Paths;
use crate::media::ffmpeg;
use anyhow::{Context, Result};
use rayon::prelude::*;
use serde_json::{Map, Value, json};
use sf2::{Region, SampleHeader, SoundFont, op};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

const DEFAULT_SF2: &str = "/usr/share/soundfonts/FluidR3_GM.sf2";
/// The licence file shipped next to the default soundfont by its Fedora package.
const DEFAULT_LICENSE: &str = "/usr/share/doc/fluid-soundfont-common/COPYING";
/// Banks written: General MIDI melodic and percussion.
const BANKS: [u16; 2] = [0, 128];

pub fn run(paths: &Paths, sf2_path: Option<PathBuf>) -> Result<()> {
    let src = sf2_path.or_else(|| std::env::var_os("ENCARTA_SF2").map(PathBuf::from)).unwrap_or_else(|| DEFAULT_SF2.into());
    let bytes = std::fs::read(&src).with_context(|| format!("reading soundfont {} (set --sf2 or ENCARTA_SF2)", src.display()))?;
    let sf = SoundFont::parse(&bytes).with_context(|| format!("parsing {}", src.display()))?;
    let dir = paths.out.join("soundfont");
    let source = json!({ "file": src.file_name().map(|n| n.to_string_lossy().into_owned()), "bytes": bytes.len() });

    // A different source soundfont invalidates the stored samples.
    if let Ok(old) = std::fs::read(dir.join("index.json")) {
        let old: Value = serde_json::from_slice(&old).unwrap_or(Value::Null);
        if old.get("source") != Some(&source) {
            let _ = std::fs::remove_dir_all(&dir);
        }
    }
    std::fs::create_dir_all(dir.join("presets"))?;
    std::fs::create_dir_all(dir.join("samples/.tmp"))?;

    let mut presets: Vec<&sf2::Preset> = sf.presets.iter().filter(|p| BANKS.contains(&p.bank) && p.program < 128).collect();
    presets.sort_by_key(|p| (p.bank, p.program));
    presets.dedup_by_key(|p| (p.bank, p.program));

    let usable = |s: &SampleHeader| s.rate > 0 && s.len() >= 2;
    let mut used = BTreeMap::new();
    let mut index_presets = Vec::new();
    let mut region_count = 0usize;
    for p in &presets {
        let regions: Vec<Region> = sf.regions(p).into_iter().filter(|r| usable(&sf.samples[r.sample])).collect();
        let mut samples = Map::new();
        for r in &regions {
            let s = &sf.samples[r.sample];
            used.insert(r.sample, s);
            samples.insert(r.sample.to_string(), json!({ "rate": s.rate, "length": s.len() }));
        }
        let file = format!("presets/{}-{}.json", p.bank, p.program);
        let doc = json!({
            "bank": p.bank,
            "program": p.program,
            "name": p.name,
            "regions": regions.iter().map(|r| region_json(r, &sf.samples[r.sample])).collect::<Vec<_>>(),
            "samples": samples,
        });
        std::fs::write(dir.join(&file), serde_json::to_vec(&doc)?).with_context(|| format!("writing {file}"))?;
        region_count += regions.len();
        index_presets.push(json!({ "bank": p.bank, "program": p.program, "name": p.name, "regions": regions.len() }));
    }

    let raw_bytes: u64 = used.values().map(|s| s.len() as u64 * 2).sum();
    let samples_dir = dir.join("samples");
    let failures: Vec<String> = used
        .par_iter()
        .filter_map(|(&id, s)| encode_sample(&sf, id, s, &samples_dir).err().map(|e| format!("sample {id} ({}): {e:#}", s.name)))
        .collect();
    let _ = std::fs::remove_dir_all(samples_dir.join(".tmp"));
    let flac_bytes: u64 = used.keys().filter_map(|id| std::fs::metadata(samples_dir.join(format!("{id}.flac"))).ok()).map(|m| m.len()).sum();

    let license = std::fs::read_to_string(DEFAULT_LICENSE).ok().filter(|_| src == Path::new(DEFAULT_SF2));
    if let Some(text) = &license {
        std::fs::write(dir.join("LICENSE.txt"), text)?;
    }
    let index = json!({
        "name": sf.info("INAM"),
        "copyright": sf.info("ICOP"),
        "comment": sf.info("ICMT"),
        "license": license.as_ref().map(|_| "LICENSE.txt"),
        "source": source,
        "presets": index_presets,
    });
    std::fs::write(dir.join("index.json"), serde_json::to_vec_pretty(&index)?)?;

    let report = json!({
        "source": src.display().to_string(),
        "name": sf.info("INAM"),
        "presets": presets.len(),
        "regions": region_count,
        "samples": used.len(),
        "sample_bytes_pcm16": raw_bytes,
        "sample_bytes_flac": flac_bytes,
        "failures": failures,
    });
    let report_dir = paths.out.join("report");
    std::fs::create_dir_all(&report_dir)?;
    std::fs::write(report_dir.join("soundfont.json"), serde_json::to_vec_pretty(&report)?)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    anyhow::ensure!(failures.is_empty(), "{} samples failed", failures.len());
    Ok(())
}

/// One region as the client reads it (docs/media.md §8). Times stay in timecents and levels in
/// centibels so the client can apply key scaling and the default modulators exactly.
fn region_json(r: &Region, s: &SampleHeader) -> Value {
    let g = |n: usize| r.get(n);
    let len = s.len() as i64;
    let coarse = |fine: usize, coarse: usize| g(fine) as i64 + 32768 * g(coarse) as i64;
    let mut o = Map::new();
    o.insert("keys".into(), json!([r.keys.lo, r.keys.hi]));
    o.insert("vels".into(), json!([r.vels.lo, r.vels.hi]));
    o.insert("sample".into(), json!(r.sample));
    let root = if g(op::ROOT_KEY) >= 0 { g(op::ROOT_KEY) } else if s.original_pitch <= 127 { s.original_pitch as i32 } else { 60 };
    o.insert("root".into(), json!(root));
    let tune = g(op::COARSE_TUNE) * 100 + g(op::FINE_TUNE) + s.pitch_correction as i32;
    if tune != 0 {
        o.insert("tune".into(), json!(tune));
    }
    if g(op::SCALE_TUNING) != 100 {
        o.insert("scale".into(), json!(g(op::SCALE_TUNING)));
    }
    let offset = coarse(op::START_OFFSET, op::START_COARSE).clamp(0, len - 1);
    if offset != 0 {
        o.insert("offset".into(), json!(offset));
    }
    let end = (len + coarse(op::END_OFFSET, op::END_COARSE)).clamp(offset + 1, len);
    if end != len {
        o.insert("end".into(), json!(end));
    }
    let mode = g(op::SAMPLE_MODES) & 3;
    let loop_start = (s.loop_start as i64 - s.start as i64 + coarse(op::LOOP_START_OFFSET, op::LOOP_START_COARSE)).clamp(0, len);
    let loop_end = (s.loop_end as i64 - s.start as i64 + coarse(op::LOOP_END_OFFSET, op::LOOP_END_COARSE)).clamp(0, len);
    if (mode == 1 || mode == 3) && loop_end > loop_start {
        o.insert("loop".into(), json!(mode));
        o.insert("loopStart".into(), json!(loop_start));
        o.insert("loopEnd".into(), json!(loop_end));
    }
    let mut put = |k: &str, v: i32, default: i32| {
        if v != default {
            o.insert(k.into(), json!(v));
        }
    };
    put("attenuation", g(op::ATTENUATION), 0);
    put("pan", g(op::PAN).clamp(-500, 500), 0);
    put("reverb", g(op::REVERB_SEND), 0);
    put("chorus", g(op::CHORUS_SEND), 0);
    put("exclusive", g(op::EXCLUSIVE_CLASS), 0);
    put("filterQ", g(op::FILTER_Q), 0);
    let env = |first: usize| (first..first + 6).map(g).collect::<Vec<_>>();
    o.insert("volEnv".into(), json!(env(op::DELAY_VOL_ENV)));
    if g(op::KEY_TO_VOL_ENV_HOLD) != 0 || g(op::KEY_TO_VOL_ENV_DECAY) != 0 {
        o.insert("volEnvKey".into(), json!([g(op::KEY_TO_VOL_ENV_HOLD), g(op::KEY_TO_VOL_ENV_DECAY)]));
    }
    let to_filter = g(op::MOD_ENV_TO_FILTER_FC);
    let to_pitch = g(op::MOD_ENV_TO_PITCH);
    if to_filter != 0 || to_pitch != 0 {
        o.insert("modEnv".into(), json!(env(op::DELAY_MOD_ENV)));
        debug_assert_eq!(op::RELEASE_MOD_ENV, op::DELAY_MOD_ENV + 5);
        if g(op::KEY_TO_MOD_ENV_HOLD) != 0 || g(op::KEY_TO_MOD_ENV_DECAY) != 0 {
            o.insert("modEnvKey".into(), json!([g(op::KEY_TO_MOD_ENV_HOLD), g(op::KEY_TO_MOD_ENV_DECAY)]));
        }
        if to_filter != 0 {
            o.insert("modEnvToFilter".into(), json!(to_filter));
        }
        if to_pitch != 0 {
            o.insert("modEnvToPitch".into(), json!(to_pitch));
        }
    }
    let fc = g(op::FILTER_FC).clamp(1500, 13500);
    if fc < 13500 || to_filter != 0 {
        o.insert("filterFc".into(), json!(fc));
    }
    // Also written when only frequency or delay differ: the mod wheel drives the same LFO.
    if g(op::VIB_LFO_TO_PITCH) != 0 || g(op::FREQ_VIB_LFO) != 0 || g(op::DELAY_VIB_LFO) != -12000 {
        o.insert("vibrato".into(), json!([g(op::VIB_LFO_TO_PITCH), g(op::FREQ_VIB_LFO), g(op::DELAY_VIB_LFO)]));
    }
    Value::Object(o)
}

/// Writes `samples/{id}.flac` (16-bit mono at the sample's own rate) unless it already exists.
fn encode_sample(sf: &SoundFont, id: usize, s: &SampleHeader, dir: &Path) -> Result<()> {
    let out = dir.join(format!("{id}.flac"));
    if out.is_file() {
        return Ok(());
    }
    let data = sf.sample_data(s)?;
    let wav = dir.join(format!(".tmp/{id}.wav"));
    std::fs::write(&wav, wav_bytes(data, s.rate))?;
    let tmp = dir.join(format!(".tmp/{id}.flac"));
    let args: Vec<String> = ["-i", &wav.to_string_lossy(), "-c:a", "flac", "-compression_level", "8", &tmp.to_string_lossy()].iter().map(|s| s.to_string()).collect();
    ffmpeg::ffmpeg(&args, Duration::from_secs(60))?;
    let _ = std::fs::remove_file(&wav);
    std::fs::rename(&tmp, &out)?;
    Ok(())
}

/// A canonical 16-bit mono PCM WAV.
fn wav_bytes(pcm: &[u8], rate: u32) -> Vec<u8> {
    let mut v = Vec::with_capacity(44 + pcm.len());
    v.extend(b"RIFF");
    v.extend((36 + pcm.len() as u32).to_le_bytes());
    v.extend(b"WAVEfmt ");
    v.extend(16u32.to_le_bytes());
    v.extend(1u16.to_le_bytes()); // PCM
    v.extend(1u16.to_le_bytes()); // mono
    v.extend(rate.to_le_bytes());
    v.extend((rate * 2).to_le_bytes());
    v.extend(2u16.to_le_bytes());
    v.extend(16u16.to_le_bytes());
    v.extend(b"data");
    v.extend((pcm.len() as u32).to_le_bytes());
    v.extend(pcm);
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn region_json_resolves_pitch_and_loop() {
        let b = sf2::tests::synthetic();
        let sf = SoundFont::parse(&b).unwrap();
        let regions = sf.regions(&sf.presets[0]);
        let low = region_json(&regions[0], &sf.samples[0]);
        // Root from the sample header, tune from its pitch correction, no loop (mode 0).
        assert_eq!(low["root"], 48);
        assert_eq!(low["tune"], -3);
        assert!(low.get("loop").is_none());
        assert_eq!(low["attenuation"], 20);
        assert_eq!(low["volEnv"], json!([-12000, -12000, -12000, -12000, 0, -1200]));
        assert!(low.get("filterFc").is_none());
        let high = region_json(&regions[1], &sf.samples[1]);
        // Root key override; loop points relative to the sample's start (156 − 146).
        assert_eq!(high["root"], 72);
        assert_eq!(high["loop"], 1);
        assert_eq!(high["loopStart"], 10);
        assert_eq!(high["loopEnd"], 90);
    }

    #[test]
    fn wav_header() {
        let w = wav_bytes(&[1, 0, 2, 0], 22050);
        assert_eq!(w.len(), 48);
        assert_eq!(&w[0..4], b"RIFF");
        assert_eq!(u32::from_le_bytes(w[24..28].try_into().unwrap()), 22050);
        assert_eq!(u32::from_le_bytes(w[40..44].try_into().unwrap()), 4);
    }
}
