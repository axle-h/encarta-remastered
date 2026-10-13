//! One job per unique source file (by SHA-256): store the original and make whatever derivatives
//! its kind calls for (docs/media.md §2). Jobs are independent and run on a bounded pool.

use super::ffmpeg::{self, AudioStream, Tools};
use super::manifest::DerivRow;
use super::rules::{ColourKey, Kind};
use super::sniff::{self, Format};
use super::store::{self, Store};
use super::{sami, still};
use anyhow::{Context, Result, anyhow, bail};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct Source {
    pub sha: String,
    /// A file on disk with these bytes (the first member that has them).
    pub path: PathBuf,
    pub size: u64,
    pub fmt: Format,
    pub kind: Kind,
    pub key: Option<ColourKey>,
    /// Extra facts for the original's row (e.g. the `.iax` root attributes).
    pub meta: Option<serde_json::Value>,
}

/// One output a source should have.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spec {
    pub format: String,
    pub suffix: String,
    pub recipe: String,
}

impl Source {
    /// Relative path of an output of this source.
    pub fn rel(&self, spec: &Spec) -> String {
        if spec.format == "original" {
            Store::original_rel(&self.sha, self.fmt.ext())
        } else {
            Store::derivative_rel(&self.sha, &spec.suffix)
        }
    }

    /// Rough cost, used to start the slowest jobs first.
    pub fn cost(&self) -> u64 {
        match self.kind {
            Kind::Video => 1000 + self.size / 1000,
            Kind::Audio | Kind::Swa => 50 + self.size / 20_000,
            Kind::Image if self.fmt == Format::Jpeg => 10 + self.size / 50_000,
            _ => 1,
        }
    }
}

fn spec(format: &str, suffix: &str, recipe: impl Into<String>) -> Spec {
    Spec { format: format.into(), suffix: suffix.into(), recipe: recipe.into() }
}

/// Every output a source should have, given the available encoders. Recipes name the encoder
/// and settings; changing one makes the next run redo that derivative.
pub fn wanted(src: &Source, tools: &Tools) -> Vec<Spec> {
    let mut v = vec![spec("original", "", "copy")];
    match src.kind {
        Kind::Image => {
            if src.fmt == Format::Bmp {
                v.push(spec("png", "png", format!("png-best key={:?} keyrule-2", src.key)));
            }
            if let Some(w) = tools.webp {
                if src.fmt == Format::Jpeg {
                    v.push(spec("webp", "webp", format!("{w} q80 m4")));
                } else {
                    let anim = tools.webp_anim.unwrap_or(w);
                    v.push(spec("webp", "webp", format!("{w}/{anim} lossless m6 key={:?} keyrule-2", src.key)));
                }
            }
            if let (Some(a), Format::Jpeg) = (tools.avif, src.fmt) {
                v.push(spec("avif", "avif", format!("{a} crf30 cpu6")));
            }
        }
        Kind::Cursor => v.push(spec("png", "png", "png-best cursor")),
        Kind::Audio | Kind::Swa => {
            if src.kind == Kind::Swa {
                v.push(spec("mp3", "mp3", "swa-strip-2"));
            }
            if let Some(o) = tools.opus {
                v.push(spec("opus", "opus", format!("{o} vbr rate-rule-1 asetpts")));
            }
            if let Some(a) = tools.aac {
                v.push(spec("aac", "m4a", format!("{a} rate-rule-1 asetpts mp4 faststart")));
            }
        }
        Kind::Video => {
            if let (Some(h), Some(a)) = (tools.h264, tools.aac) {
                v.push(spec("mp4", "mp4", format!("{h} slow crf23 yuv420p + {a} rate-rule-1 async faststart")));
            }
            if let (Some(p), Some(o)) = (tools.vp9, tools.opus) {
                v.push(spec("webm", "webm", format!("{p} crf34 good cpu2 + {o} rate-rule-1 async")));
            }
            if let Some(w) = tools.webp {
                v.push(spec("poster", "poster.webp", format!("{w} q80 at-1s")));
            }
        }
        Kind::Captions => v.push(spec("vtt", "vtt", "sami-webvtt-2")),
        Kind::Mp3 | Kind::Midi | Kind::Flash | Kind::Interactive | Kind::PlayerConfig | Kind::Director => {}
    }
    v
}

/// Opus and AAC bit rates (kbit/s) for a source stream ("rate-rule-1").
pub fn audio_bitrates(a: &AudioStream) -> (u32, u32) {
    let stereo = a.channels >= 2;
    match (a.sample_rate <= 16_000, stereo) {
        (true, false) => (24, 48),
        (true, true) => (32, 64),
        (false, false) => (40, 64),
        (false, true) => (80, 128),
    }
}

pub struct Outcome {
    pub rows: Vec<DerivRow>,
    pub failures: Vec<(String, String)>,
}

struct Ctx<'a> {
    src: &'a Source,
    store: &'a Store,
    rows: Vec<DerivRow>,
    failures: Vec<(String, String)>,
}

impl Ctx<'_> {
    /// Moves a finished temp file into place and records its row.
    fn finish(&mut self, spec: &Spec, tmp: &Path, dims: Option<(u32, u32)>, duration_ms: Option<u64>, meta: Option<serde_json::Value>) -> Result<()> {
        let bytes = std::fs::metadata(tmp)?.len();
        if bytes == 0 {
            let _ = std::fs::remove_file(tmp);
            bail!("encoder wrote an empty file");
        }
        // Audio/video outputs: record their own measured length (container headers on the disc
        // can overstate it), which also checks that the output demuxes.
        let mut meta = meta;
        let mut duration_ms = duration_ms;
        if PROBED_FORMATS.contains(&spec.format.as_str()) {
            let p = ffmpeg::probe(tmp).context("probing the output")?;
            duration_ms = p.duration_ms.or(duration_ms);
            let m = meta.get_or_insert_with(|| json!({}));
            m["probed"] = json!(true);
        }
        let out_sha = store::sha256_file(tmp)?;
        let rel = self.src.rel(spec);
        self.store.commit(tmp, &rel)?;
        self.rows.push(DerivRow {
            sha256: self.src.sha.clone(),
            format: spec.format.clone(),
            path: rel,
            width: dims.map(|d| d.0),
            height: dims.map(|d| d.1),
            duration_ms,
            bytes,
            out_sha256: out_sha,
            recipe: spec.recipe.clone(),
            meta: meta.map(|m| m.to_string()),
        });
        Ok(())
    }

    fn fail(&mut self, spec: &Spec, e: impl std::fmt::Display) {
        self.failures.push((spec.format.clone(), e.to_string()));
    }

    /// Stores the original bytes and records its row.
    fn original(&mut self, spec: &Spec, bytes: &[u8], dims: Option<(u32, u32)>, duration_ms: Option<u64>, meta: Option<serde_json::Value>) {
        let r = (|| {
            let tmp = self.store.temp_path(self.src.fmt.ext());
            std::fs::write(&tmp, bytes)?;
            let mut meta = meta.unwrap_or_else(|| json!({}));
            if let (Some(m), Some(extra)) = (meta.as_object_mut(), self.src.meta.as_ref().and_then(|v| v.as_object())) {
                m.extend(extra.clone());
            }
            let meta = (meta.as_object().is_some_and(|m| !m.is_empty())).then_some(meta);
            self.finish(spec, &tmp, dims, duration_ms, meta)
        })();
        if let Err(e) = r {
            self.fail(spec, e);
        }
    }
}

/// Outputs whose duration is measured with ffprobe.
pub const PROBED_FORMATS: &[&str] = &["opus", "aac", "mp3", "mp4", "webm"];

const IMAGE_TIMEOUT: Duration = Duration::from_secs(180);
const AUDIO_TIMEOUT: Duration = Duration::from_secs(600);
const VIDEO_TIMEOUT: Duration = Duration::from_secs(3600);

fn s(v: &str) -> String {
    v.to_string()
}

/// Runs one ffmpeg command with several outputs; on failure, retries each output alone so the
/// error lands on the right derivative.
fn ffmpeg_outputs(
    ctx: &mut Ctx,
    input_args: &[String],
    outputs: Vec<(Spec, Vec<String>, PathBuf)>,
    timeout: Duration,
) -> Vec<(Spec, PathBuf, Vec<String>)> {
    let mut args = input_args.to_vec();
    for (_, o, tmp) in &outputs {
        args.extend(o.iter().cloned());
        args.push(tmp.to_string_lossy().into_owned());
    }
    match ffmpeg::ffmpeg(&args, timeout) {
        Ok(out) => {
            let warnings = ffmpeg::warning_lines(&out.stderr);
            outputs.into_iter().map(|(s, _, t)| (s, t, warnings.clone())).collect()
        }
        Err(_) if outputs.len() > 1 => {
            let mut ok = Vec::new();
            for (s, o, tmp) in outputs {
                let mut a = input_args.to_vec();
                a.extend(o);
                a.push(tmp.to_string_lossy().into_owned());
                match ffmpeg::ffmpeg(&a, timeout) {
                    Ok(out) => ok.push((s, tmp, ffmpeg::warning_lines(&out.stderr))),
                    Err(e) => {
                        let _ = std::fs::remove_file(&tmp);
                        ctx.fail(&s, format!("ffmpeg: {e}"));
                    }
                }
            }
            ok
        }
        Err(e) => {
            for (s, _, tmp) in outputs {
                let _ = std::fs::remove_file(&tmp);
                ctx.fail(&s, format!("ffmpeg: {e}"));
            }
            Vec::new()
        }
    }
}

fn input(path: &Path) -> Vec<String> {
    vec![s("-i"), path.to_string_lossy().into_owned()]
}

fn warn_meta(w: &[String]) -> Option<serde_json::Value> {
    (!w.is_empty()).then(|| json!({ "ffmpeg_warnings": w }))
}

/// Runs a job for the given outputs (the ones not already done).
pub fn run(src: &Source, todo: &[Spec], store: &Store, tools: &Tools) -> Outcome {
    let mut ctx = Ctx { src, store, rows: Vec::new(), failures: Vec::new() };
    let r = match src.kind {
        Kind::Image => image_job(&mut ctx, todo, tools),
        Kind::Cursor => cursor_job(&mut ctx, todo),
        Kind::Audio | Kind::Swa | Kind::Mp3 => audio_job(&mut ctx, todo, tools),
        Kind::Video => video_job(&mut ctx, todo, tools),
        Kind::Captions => captions_job(&mut ctx, todo),
        _ => plain_job(&mut ctx, todo),
    };
    if let Err(e) = r {
        // A failure before any output (unreadable or undecodable source) fails every output.
        for spec in todo {
            if !ctx.rows.iter().any(|r| r.format == spec.format) && !ctx.failures.iter().any(|f| f.0 == spec.format) {
                ctx.fail(spec, format!("{e:#}"));
            }
        }
    }
    Outcome { rows: ctx.rows, failures: ctx.failures }
}

fn read(src: &Source) -> Result<Vec<u8>> {
    let b = std::fs::read(&src.path).with_context(|| format!("reading {}", src.path.display()))?;
    if store::sha256_bytes(&b) != src.sha {
        bail!("{} changed since the survey", src.path.display());
    }
    Ok(b)
}

/// Stores the original if it is in `todo` (always first, so a source that later fails to decode
/// is still kept), and returns the remaining outputs.
fn take_original<'a>(
    ctx: &mut Ctx,
    todo: &'a [Spec],
    bytes: &[u8],
    dims: Option<(u32, u32)>,
    duration_ms: Option<u64>,
    meta: Option<serde_json::Value>,
) -> Vec<&'a Spec> {
    let mut rest = Vec::new();
    for spec in todo {
        if spec.format == "original" {
            ctx.original(spec, bytes, dims, duration_ms, meta.clone());
        } else {
            rest.push(spec);
        }
    }
    rest
}

fn fail_all(ctx: &mut Ctx, specs: &[&Spec], e: &anyhow::Error) {
    for spec in specs {
        ctx.fail(spec, format!("{e:#}"));
    }
}

fn plain_job(ctx: &mut Ctx, todo: &[Spec]) -> Result<()> {
    let bytes = read(ctx.src)?;
    let rest = take_original(ctx, todo, &bytes, None, None, None);
    fail_all(ctx, &rest, &anyhow!("no derivative is defined for this kind"));
    Ok(())
}

fn image_job(ctx: &mut Ctx, todo: &[Spec], tools: &Tools) -> Result<()> {
    let src = ctx.src;
    let bytes = read(src)?;
    let info = still::info(&bytes, src.fmt).context("reading image header");
    // BMP family: PNG first (it is also the WebP input).
    let png = (src.fmt == Format::Bmp).then(|| still::bmp_to_png(&bytes, src.key).context("BMP → PNG"));
    let mut meta = json!({});
    match &info {
        Ok(i) if src.fmt == Format::Gif => meta = json!({ "frames": i.frames, "transparent": i.transparent }),
        Ok(_) => {}
        Err(e) => meta = json!({ "decode_error": format!("{e:#}") }),
    }
    if let Some(Ok(p)) = &png {
        meta["png_alpha"] = json!(p.alpha);
        meta["keyed_pixels"] = json!(p.keyed_pixels);
        if let Some(k) = src.key {
            meta["key"] = json!(format!("{k:?}").to_lowercase());
        }
    }
    let dims = info.as_ref().ok().map(|i| (i.width, i.height));
    let rest = take_original(ctx, todo, &bytes, dims, None, Some(meta));
    let info = match info {
        Ok(i) => i,
        Err(e) => {
            fail_all(ctx, &rest, &e);
            return Ok(());
        }
    };
    let png = match png.transpose() {
        Ok(p) => p,
        Err(e) => {
            fail_all(ctx, &rest, &e);
            return Ok(());
        }
    };
    let mut from_source = Vec::new();
    let mut from_png = Vec::new();
    for spec in rest {
        match spec.format.as_str() {
            "png" => {
                let Some(p) = &png else {
                    ctx.fail(spec, "PNG output is only defined for BMP sources");
                    continue;
                };
                let tmp = ctx.store.temp_path("png");
                std::fs::write(&tmp, &p.bytes)?;
                let m = json!({ "alpha": p.alpha, "keyed_pixels": p.keyed_pixels });
                if let Err(e) = ctx.finish(spec, &tmp, Some((p.width, p.height)), None, Some(m)) {
                    ctx.fail(spec, e);
                }
            }
            "webp" => {
                if info.width > still::WEBP_MAX || info.height > still::WEBP_MAX {
                    ctx.fail(spec, format!("not applicable: {}×{} exceeds the WebP limit", info.width, info.height));
                    continue;
                }
                let animated = info.frames > 1;
                let encoder = if animated { tools.webp_anim.or(tools.webp) } else { tools.webp };
                let out = (spec.clone(), still::webp_args(encoder.unwrap_or("libwebp"), src.fmt != Format::Jpeg, animated), ctx.store.temp_path("webp"));
                if png.is_some() { from_png.push(out) } else { from_source.push(out) }
            }
            "avif" => from_source.push((spec.clone(), still::avif_args(tools.avif.unwrap_or("libaom-av1")), ctx.store.temp_path("avif"))),
            other => ctx.fail(spec, format!("unknown image output {other}")),
        }
    }
    let mut png_input = None;
    if let (Some(p), false) = (&png, from_png.is_empty()) {
        let tmp = ctx.store.temp_path("png");
        std::fs::write(&tmp, &p.bytes)?;
        png_input = Some(tmp);
    }
    for (group, path) in [(from_source, Some(src.path.clone())), (from_png, png_input.clone())] {
        let (false, Some(path)) = (group.is_empty(), path) else { continue };
        let mut in_args = Vec::new();
        if src.fmt == Format::Gif && info.frames > 1 {
            in_args.extend([s("-ignore_loop"), s("1")]);
        }
        in_args.extend(input(&path));
        for (spec, tmp, warnings) in ffmpeg_outputs(ctx, &in_args, group, IMAGE_TIMEOUT) {
            let mut m = json!({});
            if spec.format == "webp" && info.frames > 1 {
                m["animated"] = json!(true);
            }
            if !warnings.is_empty() {
                m["ffmpeg_warnings"] = json!(warnings);
            }
            let m = m.as_object().is_some_and(|o| !o.is_empty()).then_some(m);
            if let Err(e) = ctx.finish(&spec, &tmp, Some((info.width, info.height)), None, m) {
                ctx.fail(&spec, e);
            }
        }
    }
    if let Some(t) = png_input {
        let _ = std::fs::remove_file(t);
    }
    Ok(())
}

fn cursor_job(ctx: &mut Ctx, todo: &[Spec]) -> Result<()> {
    let bytes = read(ctx.src)?;
    let decoded = still::cursor_to_png(&bytes);
    let (dims, meta) = match &decoded {
        Ok((png, hotspot)) => (Some((png.width, png.height)), json!({ "hotspot": [hotspot.0, hotspot.1] })),
        Err(e) => (None, json!({ "decode_error": format!("{e:#}") })),
    };
    let rest = take_original(ctx, todo, &bytes, dims, None, Some(meta.clone()));
    match decoded {
        Ok((png, _)) => {
            for spec in rest {
                let tmp = ctx.store.temp_path("png");
                std::fs::write(&tmp, &png.bytes)?;
                if let Err(e) = ctx.finish(spec, &tmp, dims, None, Some(meta.clone())) {
                    ctx.fail(spec, e);
                }
            }
        }
        Err(e) => fail_all(ctx, &rest, &e),
    }
    Ok(())
}

fn audio_out_args(kind: &str, encoder: &str, a: &AudioStream) -> Vec<String> {
    let (opus_k, aac_k) = audio_bitrates(a);
    let mut v = vec![s("-c:a"), s(encoder)];
    if a.channels > 2 {
        v.extend([s("-ac"), s("2")]);
    }
    if kind == "opus" {
        v.extend([s("-b:a"), format!("{opus_k}k"), s("-vbr"), s("on")]);
    } else {
        v.extend([s("-b:a"), format!("{aac_k}k")]);
    }
    v
}

fn audio_job(ctx: &mut Ctx, todo: &[Spec], tools: &Tools) -> Result<()> {
    let src = ctx.src;
    let bytes = read(src)?;
    // SWA: the audio is the MPEG stream after the header.
    // SWA: `director::swa_file_to_mp3` checks and strips the standard 0x144-byte header. It
    // rejects other header lengths (four IAF `.MP3` files have 0x164); for those our own rule
    // (header length field + 4, frame sync must follow) applies.
    let mut stripper = None;
    let swa = if src.kind == Kind::Swa {
        match encarta_formats::director::swa_file_to_mp3(&bytes) {
            Ok(mp3) => {
                stripper = Some("director::swa_file_to_mp3");
                Ok(bytes.len() - mp3.len())
            }
            Err(lib_err) => sniff::swa_audio_offset(&bytes)
                .inspect(|_| stripper = Some("media::sniff::swa_audio_offset"))
                .ok_or_else(|| anyhow!("SWA header does not end at an MPEG frame ({lib_err:#})")),
        }
    } else {
        Ok(0)
    };
    let mut tmp_input = None;
    let probe = match &swa {
        Ok(off) if src.kind == Kind::Swa => {
            let tmp = ctx.store.temp_path("mp3");
            std::fs::write(&tmp, &bytes[*off..])?;
            let p = ffmpeg::probe(&tmp).context("ffprobe");
            tmp_input = Some(tmp);
            p
        }
        Ok(_) => ffmpeg::probe(&src.path).context("ffprobe"),
        Err(e) => Err(anyhow!("{e}")),
    };
    let input_path = tmp_input.clone().unwrap_or_else(|| src.path.clone());
    let mut meta = json!({});
    let (stream, duration) = match &probe {
        Ok(p) => {
            let a = p.best_audio().cloned();
            meta["audio_streams"] = json!(p.audio.len());
            if let Some(a) = &a {
                meta["codec"] = json!(a.codec);
                meta["sample_rate"] = json!(a.sample_rate);
                meta["channels"] = json!(a.channels);
            }
            (a, p.duration_ms)
        }
        Err(e) => {
            meta["probe_error"] = json!(format!("{e:#}"));
            (None, None)
        }
    };
    if let (Kind::Swa, Ok(off)) = (src.kind, &swa) {
        meta["swa_header_bytes"] = json!(off);
        meta["swa_stripper"] = json!(stripper);
    }
    let rest = take_original(ctx, todo, &bytes, None, duration, Some(meta));
    let Some(a) = stream else {
        let e = match probe {
            Err(e) => e,
            Ok(_) => anyhow!("no audio stream"),
        };
        fail_all(ctx, &rest, &e);
        if let Some(t) = tmp_input {
            let _ = std::fs::remove_file(t);
        }
        return Ok(());
    };
    let mut ff = Vec::new();
    for spec in rest {
        match spec.format.as_str() {
            "mp3" => {
                let out = ctx.store.temp_path("mp3");
                std::fs::copy(&input_path, &out)?;
                let m = json!({ "stripped_header_bytes": swa.as_ref().ok(), "stripper": stripper });
                if let Err(e) = ctx.finish(spec, &out, None, duration, Some(m)) {
                    ctx.fail(spec, e);
                }
            }
            "opus" | "aac" => {
                let opus = spec.format == "opus";
                let enc = if opus { tools.opus.unwrap_or("libopus") } else { tools.aac.unwrap_or("aac") };
                // ASF packet timestamps jitter by a millisecond; the decoded audio is continuous,
                // so timestamps are regenerated from the sample count.
                let mut args = vec![s("-map"), format!("0:{}", a.index), s("-map_metadata"), s("-1"), s("-vn")];
                args.extend([s("-af"), s("asetpts=N/SR/TB")]);
                args.extend(audio_out_args(&spec.format, enc, &a));
                if opus {
                    args.extend([s("-f"), s("opus")]);
                } else {
                    args.extend([s("-movflags"), s("+faststart"), s("-f"), s("mp4")]);
                }
                ff.push((spec.clone(), args, ctx.store.temp_path(&spec.suffix)));
            }
            other => ctx.fail(spec, format!("unknown audio output {other}")),
        }
    }
    if !ff.is_empty() {
        let (opus_k, aac_k) = audio_bitrates(&a);
        for (spec, tmp, warnings) in ffmpeg_outputs(ctx, &input(&input_path), ff, AUDIO_TIMEOUT) {
            let mut m = json!({ "kbps": if spec.format == "opus" { opus_k } else { aac_k } });
            if !warnings.is_empty() {
                m["ffmpeg_warnings"] = json!(warnings);
            }
            if let Err(e) = ctx.finish(&spec, &tmp, None, duration, Some(m)) {
                ctx.fail(&spec, e);
            }
        }
    }
    if let Some(t) = tmp_input {
        let _ = std::fs::remove_file(t);
    }
    Ok(())
}

fn video_job(ctx: &mut Ctx, todo: &[Spec], tools: &Tools) -> Result<()> {
    let src = ctx.src;
    let bytes = read(src)?;
    let probe = ffmpeg::probe(&src.path)
        .context("ffprobe")
        .and_then(|p| p.video.clone().map(|v| (p, v)).ok_or_else(|| anyhow!("no video stream")));
    let mut meta = json!({});
    let (dims, duration) = match &probe {
        Ok((p, v)) => {
            meta["video_codec"] = json!(v.codec);
            if let Some(a) = p.best_audio() {
                meta["audio_codec"] = json!(a.codec);
                meta["sample_rate"] = json!(a.sample_rate);
                meta["channels"] = json!(a.channels);
            }
            (Some((v.width, v.height)), p.duration_ms)
        }
        Err(e) => {
            meta["probe_error"] = json!(format!("{e:#}"));
            (None, None)
        }
    };
    let rest = take_original(ctx, todo, &bytes, dims, duration, Some(meta));
    drop(bytes);
    let (probe, v) = match probe {
        Ok(x) => x,
        Err(e) => {
            fail_all(ctx, &rest, &e);
            return Ok(());
        }
    };
    let a = probe.best_audio().cloned();
    let even = s("scale=trunc(iw/2)*2:trunc(ih/2)*2");
    for spec in rest {
        let mut args = Vec::new();
        match spec.format.as_str() {
            "mp4" | "webm" => {
                args.extend(input(&src.path));
                args.extend([s("-map"), format!("0:{}", v.index), s("-map_metadata"), s("-1")]);
                if let Some(a) = &a {
                    // Keep A/V sync while smoothing ASF timestamp jitter.
                    args.extend([s("-map"), format!("0:{}", a.index), s("-af"), s("aresample=async=1")]);
                }
                args.extend([s("-vf"), even.clone()]);
                if spec.format == "mp4" {
                    let h264 = tools.h264.unwrap_or("libx264");
                    args.extend(["-c:v", h264, "-preset", "slow", "-crf", "23", "-pix_fmt", "yuv420p", "-threads", "2"].map(s));
                    if let Some(a) = &a {
                        args.extend(audio_out_args("aac", tools.aac.unwrap_or("aac"), a));
                    }
                    args.extend(["-movflags", "+faststart", "-f", "mp4"].map(s));
                } else {
                    let vp9 = tools.vp9.unwrap_or("libvpx-vp9");
                    args.extend(["-c:v", vp9, "-crf", "34", "-b:v", "0", "-deadline", "good", "-cpu-used", "2"].map(s));
                    args.extend(["-row-mt", "1", "-threads", "2"].map(s));
                    if let Some(a) = &a {
                        args.extend(audio_out_args("opus", tools.opus.unwrap_or("libopus"), a));
                    }
                    args.extend(["-f", "webm"].map(s));
                }
            }
            "poster" => {
                let at = duration.map_or(0, |d| (d / 2).min(1000));
                args.extend([s("-ss"), format!("{}.{:03}", at / 1000, at % 1000)]);
                args.extend(input(&src.path));
                let webp = tools.webp.unwrap_or("libwebp");
                args.extend([s("-map"), format!("0:{}", v.index)]);
                args.extend(["-frames:v", "1", "-c:v", webp, "-quality", "80", "-f", "webp"].map(s));
            }
            other => {
                ctx.fail(spec, format!("unknown video output {other}"));
                continue;
            }
        }
        let tmp = ctx.store.temp_path(spec.suffix.rsplit('.').next().unwrap_or("bin"));
        args.push(tmp.to_string_lossy().into_owned());
        match ffmpeg::ffmpeg(&args, VIDEO_TIMEOUT) {
            Ok(out) => {
                let w = ffmpeg::warning_lines(&out.stderr);
                let dur = if spec.format == "poster" { None } else { duration };
                if let Err(e) = ctx.finish(spec, &tmp, Some((v.width, v.height)), dur, warn_meta(&w)) {
                    ctx.fail(spec, e);
                }
            }
            Err(e) => {
                let _ = std::fs::remove_file(&tmp);
                ctx.fail(spec, format!("ffmpeg: {e}"));
            }
        }
    }
    Ok(())
}

fn captions_job(ctx: &mut Ctx, todo: &[Spec]) -> Result<()> {
    let bytes = read(ctx.src)?;
    let parsed = sami::parse(&sami::decode(&bytes));
    let tracks = sami::to_webvtt(&parsed);
    let meta = json!({
        "classes": parsed.classes.keys().collect::<Vec<_>>(),
        "syncs": parsed.syncs.len(),
        "length_ms": parsed.length_ms,
    });
    let rest = take_original(ctx, todo, &bytes, None, parsed.length_ms, Some(meta));
    for spec in rest {
        if tracks.is_empty() {
            ctx.fail(spec, "no caption text in any class");
            continue;
        }
        // The first class is `vtt`; any further class gets its own `vtt-<class>` row.
        for (i, t) in tracks.iter().enumerate() {
            let class = t.class.to_ascii_lowercase();
            let sp = if i == 0 {
                spec.clone()
            } else {
                Spec { format: format!("vtt-{class}"), suffix: format!("{class}.vtt"), recipe: spec.recipe.clone() }
            };
            let tmp = ctx.store.temp_path("vtt");
            std::fs::write(&tmp, &t.vtt)?;
            let m = json!({ "class": t.class, "lang": t.lang, "label": t.label, "cues": t.cues });
            if let Err(e) = ctx.finish(&sp, &tmp, None, parsed.length_ms, Some(m)) {
                ctx.fail(&sp, e);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn src(kind: Kind, fmt: Format) -> Source {
        Source { sha: "ab".repeat(32), path: PathBuf::new(), size: 1, fmt, kind, key: None, meta: None }
    }

    fn tools() -> Tools {
        Tools {
            version: String::new(),
            opus: Some("libopus"),
            aac: Some("aac"),
            h264: Some("libx264"),
            vp9: Some("libvpx-vp9"),
            webp: Some("libwebp"),
            webp_anim: Some("libwebp_anim"),
            avif: Some("libaom-av1"),
        }
    }

    fn formats(k: Kind, f: Format) -> Vec<String> {
        wanted(&src(k, f), &tools()).into_iter().map(|s| s.format).collect()
    }

    #[test]
    fn outputs_per_kind() {
        assert_eq!(formats(Kind::Image, Format::Jpeg), ["original", "webp", "avif"]);
        assert_eq!(formats(Kind::Image, Format::Bmp), ["original", "png", "webp"]);
        assert_eq!(formats(Kind::Image, Format::Gif), ["original", "webp"]);
        assert_eq!(formats(Kind::Swa, Format::Swa), ["original", "mp3", "opus", "aac"]);
        assert_eq!(formats(Kind::Video, Format::Asf), ["original", "mp4", "webm", "poster"]);
        assert_eq!(formats(Kind::Captions, Format::Sami), ["original", "vtt"]);
        assert_eq!(formats(Kind::Flash, Format::Swf), ["original"]);
        let s = src(Kind::Image, Format::Jpeg);
        let w = wanted(&s, &tools());
        assert_eq!(s.rel(&w[0]), format!("o/ab/{}.jpg", s.sha));
        assert_eq!(s.rel(&w[1]), format!("d/ab/{}.webp", s.sha));
    }

    #[test]
    fn bitrate_rule() {
        let a = |sr, ch| AudioStream { index: 0, codec: String::new(), sample_rate: sr, channels: ch, bit_rate: 0 };
        assert_eq!(audio_bitrates(&a(11025, 1)), (24, 48));
        assert_eq!(audio_bitrates(&a(22050, 1)), (40, 64));
        assert_eq!(audio_bitrates(&a(44100, 2)), (80, 128));
    }
}
