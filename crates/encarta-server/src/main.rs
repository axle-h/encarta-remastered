//! encarta-server [--out DIR] [--static DIR] [--extracted DIR] [--listen ADDR]
//!
//! Environment: ENCARTA_OUT, ENCARTA_STATIC, ENCARTA_EXTRACTED, ENCARTA_LISTEN (flags win).

use anyhow::{Context, Result, bail};
use encarta_server::{AppState, Config, router};
use std::path::PathBuf;
use std::sync::Arc;

const USAGE: &str = "usage: encarta-server [--out DIR] [--static DIR] [--extracted DIR] [--listen ADDR]
  --out DIR        converter output (env ENCARTA_OUT, default out)
  --static DIR     built web client (env ENCARTA_STATIC, default web/dist)
  --extracted DIR  unpacked disc for the media fallback (env ENCARTA_EXTRACTED, default off)
  --listen ADDR    listen address (alias --bind; env ENCARTA_LISTEN, default 0.0.0.0:8080)";

fn env(k: &str) -> Option<String> {
    std::env::var(k).ok().filter(|v| !v.is_empty())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let term = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut s) => {
                s.recv().await;
            }
            Err(_) => std::future::pending::<()>().await,
        }
    };
    #[cfg(not(unix))]
    let term = std::future::pending::<()>();
    tokio::select! {
        _ = ctrl_c => {},
        _ = term => {},
    }
    println!("shutting down");
}

#[tokio::main]
async fn main() -> Result<()> {
    let mut out = env("ENCARTA_OUT").unwrap_or_else(|| "out".into());
    let mut static_dir = env("ENCARTA_STATIC").unwrap_or_else(|| "web/dist".into());
    let mut extracted = env("ENCARTA_EXTRACTED");
    let mut listen = env("ENCARTA_LISTEN").unwrap_or_else(|| "0.0.0.0:8080".into());
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        let mut val = || args.next().with_context(|| format!("{a} needs a value\n{USAGE}"));
        match a.as_str() {
            "--out" => out = val()?,
            "--static" => static_dir = val()?,
            "--extracted" => extracted = Some(val()?),
            "--listen" | "--bind" => listen = val()?,
            "-h" | "--help" => {
                println!("{USAGE}");
                return Ok(());
            }
            _ => bail!("unexpected argument {a}\n{USAGE}"),
        }
    }
    let mut cfg = Config::new(PathBuf::from(out));
    cfg.static_dir = Some(PathBuf::from(static_dir));
    cfg.extracted = extracted.map(PathBuf::from);
    let t = std::time::Instant::now();
    let st = tokio::task::spawn_blocking(move || AppState::open(&cfg)).await?;
    println!("opened in {:.0} ms", t.elapsed().as_secs_f64() * 1e3);
    let app = router(Arc::new(st));
    let listener = tokio::net::TcpListener::bind(&listen).await.with_context(|| format!("binding {listen}"))?;
    println!("listening on http://{}", listener.local_addr()?);
    axum::serve(listener, app).with_graceful_shutdown(shutdown_signal()).await?;
    Ok(())
}
