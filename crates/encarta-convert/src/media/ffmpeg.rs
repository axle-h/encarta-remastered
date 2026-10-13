//! The system `ffmpeg`/`ffprobe`, run as subprocesses (docs/media.md §2).

use anyhow::{Context, Result, bail};
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Encoders picked at start-up from `ffmpeg -encoders`.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct Tools {
    pub version: String,
    pub opus: Option<&'static str>,
    pub aac: Option<&'static str>,
    pub h264: Option<&'static str>,
    pub vp9: Option<&'static str>,
    pub webp: Option<&'static str>,
    pub webp_anim: Option<&'static str>,
    pub avif: Option<&'static str>,
}

impl Tools {
    pub fn detect() -> Result<Self> {
        let out = Command::new("ffmpeg")
            .args(["-hide_banner", "-encoders"])
            .output()
            .context("running ffmpeg (is it installed?)")?;
        let list = String::from_utf8_lossy(&out.stdout);
        let has = |name: &str| list.lines().any(|l| l.split_whitespace().nth(1) == Some(name));
        let pick = |names: &[&'static str]| names.iter().copied().find(|n| has(n));
        let version = Command::new("ffmpeg")
            .arg("-version")
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).lines().next().unwrap_or("").to_string())
            .unwrap_or_default();
        Ok(Tools {
            version,
            opus: pick(&["libopus"]),
            aac: pick(&["libfdk_aac", "aac"]),
            h264: pick(&["libx264"]),
            vp9: pick(&["libvpx-vp9"]),
            webp: pick(&["libwebp"]),
            webp_anim: pick(&["libwebp_anim"]),
            avif: pick(&["libaom-av1", "libsvtav1"]),
        })
    }
}

pub struct Output {
    pub stderr: String,
}

/// Runs `ffmpeg -hide_banner -nostdin -y -loglevel <level> <args>` with a timeout.
pub fn ffmpeg(args: &[String], timeout: Duration) -> Result<Output> {
    let mut cmd = Command::new("ffmpeg");
    cmd.args(["-hide_banner", "-nostdin", "-y", "-loglevel", "warning"]).args(args);
    run(cmd, timeout).map(|(_, stderr)| Output { stderr })
}

fn run(mut cmd: Command, timeout: Duration) -> Result<(Vec<u8>, String)> {
    cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().context("spawning ffmpeg/ffprobe")?;
    let mut stdout = child.stdout.take().unwrap();
    let mut stderr = child.stderr.take().unwrap();
    let out_thread = std::thread::spawn(move || {
        let mut b = Vec::new();
        let _ = stdout.read_to_end(&mut b);
        b
    });
    let err_thread = std::thread::spawn(move || {
        let mut b = Vec::new();
        let _ = stderr.read_to_end(&mut b);
        b
    });
    let start = Instant::now();
    let mut nap = Duration::from_millis(2);
    let status = loop {
        if let Some(s) = child.try_wait()? {
            break s;
        }
        if start.elapsed() > timeout {
            let _ = child.kill();
            let _ = child.wait();
            bail!("timed out after {} s", timeout.as_secs());
        }
        std::thread::sleep(nap);
        nap = (nap * 2).min(Duration::from_millis(100));
    };
    let out = out_thread.join().unwrap_or_default();
    let err = String::from_utf8_lossy(&err_thread.join().unwrap_or_default()).into_owned();
    if !status.success() {
        let tail: String = err.lines().rev().take(6).collect::<Vec<_>>().into_iter().rev().collect::<Vec<_>>().join(" | ");
        bail!("exit {}: {tail}", status.code().unwrap_or(-1));
    }
    Ok((out, err))
}

/// What ffprobe says about a source.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Probe {
    pub duration_ms: Option<u64>,
    pub video: Option<VideoStream>,
    /// Audio streams in file order (some WMA files carry two bit-rate variants).
    pub audio: Vec<AudioStream>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VideoStream {
    pub index: u32,
    pub codec: String,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AudioStream {
    pub index: u32,
    pub codec: String,
    pub sample_rate: u32,
    pub channels: u32,
    pub bit_rate: u64,
}

impl Probe {
    /// The audio stream to transcode: most channels, then highest sample rate, then bit rate.
    pub fn best_audio(&self) -> Option<&AudioStream> {
        self.audio.iter().max_by_key(|a| (a.channels, a.sample_rate, a.bit_rate))
    }
}

pub fn probe(path: &Path) -> Result<Probe> {
    let mut cmd = Command::new("ffprobe");
    cmd.args(["-v", "error", "-print_format", "json", "-show_format", "-show_streams"]).arg(path);
    let (out, _) = run(cmd, Duration::from_secs(60))?;
    parse_probe(&out)
}

pub fn parse_probe(json: &[u8]) -> Result<Probe> {
    let v: serde_json::Value = serde_json::from_slice(json).context("parsing ffprobe JSON")?;
    let num = |x: &serde_json::Value| -> Option<f64> {
        x.as_str().and_then(|s| s.parse().ok()).or_else(|| x.as_f64())
    };
    let mut p = Probe {
        duration_ms: num(&v["format"]["duration"]).map(|d| (d * 1000.0).round() as u64),
        ..Probe::default()
    };
    for s in v["streams"].as_array().into_iter().flatten() {
        let index = s["index"].as_u64().unwrap_or(0) as u32;
        let codec = s["codec_name"].as_str().unwrap_or("").to_string();
        match s["codec_type"].as_str() {
            Some("video") if p.video.is_none() => {
                p.video = Some(VideoStream {
                    index,
                    codec,
                    width: s["width"].as_u64().unwrap_or(0) as u32,
                    height: s["height"].as_u64().unwrap_or(0) as u32,
                })
            }
            Some("audio") => p.audio.push(AudioStream {
                index,
                codec,
                sample_rate: num(&s["sample_rate"]).unwrap_or(0.0) as u32,
                channels: s["channels"].as_u64().unwrap_or(0) as u32,
                bit_rate: num(&s["bit_rate"]).unwrap_or(0.0) as u64,
            }),
            _ => {}
        }
    }
    Ok(p)
}

/// Counts decoder complaints in ffmpeg's stderr (at `-loglevel warning`).
/// Benign messages (layout guesses, bitrate-based duration estimates, repeat counters) are dropped.
pub fn warning_lines(stderr: &str) -> Vec<String> {
    const BENIGN: &[&str] = &[
        "Guessed Channel Layout",
        "Estimating duration from bitrate",
        "Last message repeated",
        "deprecated pixel format used",
        "Using libwebp for RGB-to-YUV conversion",
    ];
    stderr
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !BENIGN.iter().any(|b| l.contains(b)))
        .map(|l| l.to_string())
        .take(20)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_json() {
        let j = br#"{"streams":[
            {"index":0,"codec_name":"wmav2","codec_type":"audio","sample_rate":"48000","channels":2,"bit_rate":"64000"},
            {"index":1,"codec_name":"wmav2","codec_type":"audio","sample_rate":"16000","channels":2,"bit_rate":"16000"},
            {"index":2,"codec_name":"wmv1","codec_type":"video","width":320,"height":240}],
            "format":{"duration":"37.833000"}}"#;
        let p = parse_probe(j).unwrap();
        assert_eq!(p.duration_ms, Some(37833));
        assert_eq!(p.video.as_ref().map(|v| (v.width, v.height)), Some((320, 240)));
        assert_eq!(p.best_audio().map(|a| a.index), Some(0));
    }
}
