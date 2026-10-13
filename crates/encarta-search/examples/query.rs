//! `cargo run --example query -- out/search <search|suggest|spell> <query> [kind]`
use encarta_search::{SearchIndex, SearchRequest};

fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let t = std::time::Instant::now();
    let idx = SearchIndex::open(a[0].as_ref())?;
    eprintln!("open {:?}", t.elapsed());
    for _ in 0..2 {
        let t = std::time::Instant::now();
        let out = match a[1].as_str() {
            "search" => serde_json::to_string_pretty(&idx.search(&SearchRequest {
                q: a[2].clone(),
                kind: a.get(3).cloned(),
                limit: 10,
                snippets: true,
                ..Default::default()
            })?)?,
            "explain" => idx.explain(&a[2], a[3].parse()?)?,
            "suggest" => serde_json::to_string_pretty(&idx.suggest(&a[2], 10, None, None))?,
            _ => serde_json::to_string_pretty(&idx.spell(&a[2], &[], None, 5))?,
        };
        eprintln!("{} {:?}", a[1], t.elapsed());
        if std::env::var_os("QUIET").is_none() {
            println!("{out}");
        }
    }
    Ok(())
}
