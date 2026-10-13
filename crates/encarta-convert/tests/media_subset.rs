//! Runs `encarta-convert media` on a small subset of the disc (via `MEDIA_ONLY`) and checks the
//! manifest and report. Reads `$ENCARTA_EXTRACTED` (default `<workspace>/extracted`); skipped
//! when the extracted disc or ffmpeg is absent. No disc bytes are stored in the repository.

use std::path::PathBuf;
use std::process::Command;

fn extracted() -> PathBuf {
    std::env::var_os("ENCARTA_EXTRACTED")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../extracted"))
}

const SUBSET: &str = "EE/ENCARTA/SWALLDSK.EIT,MSWORLD.EIT/dfg/1,MDDLX01.EIT/baggage/00067cfc,7015/04257.EIT,\
SWDLX01.EIT/dswMedia/IAF/a/000a3efc,SWDLX01.EIT/baggage/000a3efc,PRON.EIT/baggage/186151814,ISO/CONTENT/DSV/00045123";

fn run(out: &PathBuf) -> serde_json::Value {
    let status = Command::new(env!("CARGO_BIN_EXE_encarta-convert"))
        .args(["--extracted"])
        .arg(extracted())
        .arg("--out")
        .arg(out)
        .arg("media")
        .env("MEDIA_ONLY", SUBSET)
        .env("MEDIA_JOBS", "4")
        .status()
        .expect("running encarta-convert");
    assert!(status.success());
    let report = std::fs::read(out.join("report/media.json")).unwrap();
    serde_json::from_slice(&report).unwrap()
}

#[test]
fn media_subset() {
    if !extracted().join("eit/CONTENT/MSWORLD.EIT").is_dir() {
        eprintln!("skipping: no extracted disc at {}", extracted().display());
        return;
    }
    if Command::new("ffmpeg").arg("-version").output().is_err() {
        eprintln!("skipping: ffmpeg not installed");
        return;
    }
    let out = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("media-subset-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&out);

    let r = run(&out);
    let t = &r["totals"];
    assert_eq!(r["partial"], true);
    assert_eq!(t["unexplained"], 0, "unexplained files: {}", r["unexplained"]);
    assert_eq!(t["failed_outputs"], 0, "failures: {}", r["failures"]);
    assert!(t["jobs_run"].as_u64().unwrap() > 50);

    let db = rusqlite::Connection::open(out.join("media/manifest.sqlite")).unwrap();
    let count = |sql: &str| -> i64 { db.query_row(sql, [], |row| row.get(0)).unwrap() };
    // Every media file has its original stored, under its content hash.
    assert_eq!(
        count("SELECT COUNT(*) FROM file f LEFT JOIN derivative d ON d.sha256 = f.sha256 AND d.format = 'original' WHERE d.path IS NULL"),
        0
    );
    // SAMI → WebVTT.
    let vtt: String = db
        .query_row(
            "SELECT d.path FROM file f JOIN derivative d ON d.sha256 = f.sha256 AND d.format = 'vtt' WHERE f.name = '00067cfc.smi'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let text = std::fs::read_to_string(out.join("media").join(vtt)).unwrap();
    assert!(text.starts_with("WEBVTT\n"));
    assert!(text.contains(" --> "));
    // SWA audio (including the `.MP3`-named ones in IAF folders) gets a stripped MP3 and Opus.
    assert!(count("SELECT COUNT(*) FROM file f JOIN derivative d ON d.sha256 = f.sha256 AND d.format = 'mp3' WHERE f.format = 'swa'") >= 2);
    assert_eq!(
        count("SELECT COUNT(*) FROM file f LEFT JOIN derivative d ON d.sha256 = f.sha256 AND d.format = 'opus' WHERE f.kind = 'audio' AND f.format IN ('wav', 'asf', 'swa') AND d.path IS NULL"),
        0
    );
    // Acropolis: 18 BMP textures carry magenta key pixels (tours-3d.md §5.5).
    assert_eq!(
        count("SELECT COUNT(DISTINCT f.sha256) FROM file f JOIN derivative d ON d.sha256 = f.sha256 AND d.format = 'png' WHERE f.container LIKE '%/04257.EIT' AND d.meta LIKE '%\"alpha\":true%'"),
        18
    );
    // Flags: the green background becomes alpha.
    assert!(count("SELECT COUNT(*) FROM file f JOIN derivative d ON d.sha256 = f.sha256 AND d.format = 'png' WHERE f.member_path LIKE 'dfg/%' AND d.meta LIKE '%\"alpha\":true%'") > 50);
    // The video gets MP4, WebM and a poster.
    assert_eq!(count("SELECT COUNT(*) FROM file f JOIN derivative d ON d.sha256 = f.sha256 WHERE f.kind = 'video' AND d.format IN ('mp4', 'webm', 'poster')"), 3);
    // `.iax` references resolve into the interactive's IAF folder.
    assert!(count("SELECT COUNT(*) FROM iax_ref WHERE resolution = 'iaf-folder'") > 5);
    assert_eq!(count("SELECT COUNT(*) FROM iax_ref WHERE sha256 IS NULL"), 0);
    // Catalog keys resolve to the stored files.
    assert!(count("SELECT COUNT(*) FROM catalog_key WHERE sha256 IS NOT NULL") > 20);
    drop(db);

    // A second run finds everything in place.
    let again = run(&out);
    assert_eq!(again["totals"]["jobs_run"], 0);
    let _ = std::fs::remove_dir_all(&out);
}
