//! Decodes every Megalith lexicon on the disc and checks it against its header.
//! Reads `$ENCARTA_EXTRACTED` (default `<workspace>/extracted`); skipped when the data is absent.

use std::collections::HashSet;
use std::path::PathBuf;

use encarta_formats::lex;

fn extracted() -> PathBuf {
    std::env::var_os("ENCARTA_EXTRACTED")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../extracted"))
}

const FILES: &[&str] = &[
    "iso/EE/ENCARTA/EESPLA01.LEX",
    "iso/EE/KIDS/EESPLA01.LEX",
    "iso/EE/ENCARTA/EDICT/DICTA.LEX",
    "iso/EE/ENCARTA/EDICT/THESA.LEX",
    "iso/EE/ENCARTA/EDICT/BILAD.LEX",
    "iso/EE/ENCARTA/EDICT/BILAE.LEX",
    "iso/EE/ENCARTA/EDICT/BILAF.LEX",
    "iso/EE/ENCARTA/EDICT/BILAI.LEX",
    "iso/EE/ENCARTA/EDICT/BILDA.LEX",
    "iso/EE/ENCARTA/EDICT/BILEA.LEX",
    "iso/EE/ENCARTA/EDICT/BILFA.LEX",
    "iso/EE/ENCARTA/EDICT/BILIA.LEX",
];

#[test]
fn disc_lexicons() {
    let root = extracted();
    let mut seen = 0;
    for rel in FILES {
        let path = root.join(rel);
        let Ok(bytes) = std::fs::read(&path) else {
            eprintln!("skipping: {} not found", path.display());
            continue;
        };
        seen += 1;
        let lex = lex::parse(&bytes).unwrap_or_else(|e| panic!("{rel}: {e:#}"));
        let h = &lex.header;
        assert!(!h.lcids.is_empty(), "{rel}: no LCID");
        assert!(
            h.copyright.to_lowercase().starts_with("copyright"),
            "{rel}: {:?}",
            h.copyright
        );

        let stats = lex.scan().unwrap_or_else(|e| panic!("{rel}: {e:#}"));
        assert_eq!(stats.max_arcs, h.max_arcs as usize, "{rel}: max arcs");
        assert_eq!(
            stats.symbols_used,
            lex.symbols.len(),
            "{rel}: unused symbols"
        );
        assert_eq!(
            stats.shared_nodes_used,
            lex.nodes.len(),
            "{rel}: unused shared nodes"
        );

        let words = lex.words().unwrap_or_else(|e| panic!("{rel}: {e:#}"));
        assert_eq!(words.len(), h.word_count as usize, "{rel}: word count");
        let distinct: HashSet<&str> = words.iter().map(String::as_str).collect();
        assert_eq!(distinct.len(), words.len(), "{rel}: duplicate words");
        let longest = words
            .iter()
            .map(|w| w.encode_utf16().count())
            .max()
            .unwrap();
        assert_eq!(longest, h.max_word_len as usize, "{rel}: longest word");
        eprintln!("{rel}: {} words, {} nodes", words.len(), stats.nodes);
    }
    if seen == 0 {
        return;
    }
    // Spot checks against words known to be in the lists.
    let check = |rel: &str, expect: &[&str]| {
        let Ok(bytes) = std::fs::read(root.join(rel)) else {
            return;
        };
        let words: HashSet<String> = lex::parse(&bytes)
            .unwrap()
            .words()
            .unwrap()
            .into_iter()
            .collect();
        for w in expect {
            assert!(words.contains(*w), "{rel}: missing {w}");
        }
    };
    check(
        "iso/EE/ENCARTA/EDICT/BILAE.LEX",
        &["AGM", "ATM", "AIDS", "Ecuadorean", "Egyptian", "king-size"],
    );
    check(
        "iso/EE/ENCARTA/EESPLA01.LEX",
        &["blossoming", "frigate", "unearthed"],
    );
}
