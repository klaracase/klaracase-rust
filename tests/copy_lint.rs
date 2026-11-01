//! Copy lint: banned wording must not appear in the README, CHANGELOG, manifest, examples or source.

use std::fs;
use std::path::{Path, PathBuf};

const ALLOWED_SENTENCE: &str = "it is not a claim that the citation is fabricated.";

/// Substring patterns, matched case-insensitively.
const SUBSTRINGS: [&str; 7] = [
    "hallucinat",
    "halluzin",
    "fabricat",
    "fabrizier",
    "official source",
    "offizielle quelle",
    "amtlich",
];

/// Removes the comment markers at the start of a line, so a wrapped sentence reads as one.
fn strip_comment_marks(text: &str) -> String {
    text.lines()
        .map(|line| {
            let line = line.trim_start();
            for mark in ["//!", "///", "//", "*", "#"] {
                if let Some(rest) = line.strip_prefix(mark) {
                    return rest.trim_start();
                }
            }
            line
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn contains_word(haystack: &str, word: &str) -> bool {
    haystack.match_indices(word).any(|(start, _)| {
        let before = haystack[..start].chars().next_back();
        let after = haystack[start + word.len()..].chars().next();
        !before.is_some_and(char::is_alphanumeric) && !after.is_some_and(char::is_alphanumeric)
    })
}

/// Returns the forbidden patterns found in the text, after removing the allowed sentence.
fn find_forbidden(text: &str) -> Vec<&'static str> {
    let flat = strip_comment_marks(text)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    let cleaned = flat.replace(ALLOWED_SENTENCE, "");
    let mut hits: Vec<&'static str> = SUBSTRINGS
        .iter()
        .copied()
        .filter(|pattern| cleaned.contains(pattern))
        .collect();
    if contains_word(&cleaned, "fake") {
        hits.push("fake");
    }
    hits
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("read dir") {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            collect(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn no_forbidden_wording_in_docs_manifest_and_source() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = vec![
        root.join("README.md"),
        root.join("CHANGELOG.md"),
        root.join("Cargo.toml"),
    ];
    collect(&root.join("src"), &mut files);
    collect(&root.join("examples"), &mut files);
    let mut hits = Vec::new();
    for file in files {
        let text = fs::read_to_string(&file).expect("read file");
        for pattern in find_forbidden(&text) {
            hits.push(format!("{}: {pattern}", file.display()));
        }
    }
    assert!(hits.is_empty(), "forbidden wording: {hits:#?}");
}

#[test]
fn every_forbidden_pattern_is_flagged() {
    let samples = [
        "This stops hallucinations.",
        "Das verhindert Halluzinationen.",
        "The citation is Fabricated.",
        "Das ist fabriziert.",
        "A fake case.",
        "The official source of law.",
        "Die offizielle Quelle.",
        "Der amtliche Leitsatz.",
    ];
    for sample in samples {
        assert!(!find_forbidden(sample).is_empty(), "not flagged: {sample}");
    }
    assert!(find_forbidden("fake time in a test").len() == 1);
    assert!(find_forbidden("fakeish is not the word").is_empty());
}

#[test]
fn the_required_disclaimer_is_allowed_and_nothing_around_it() {
    let sentence = "//! A `not_found` verdict means the citation was not found in the corpus; it is not a\n//! claim that the citation is fabricated.";
    assert!(find_forbidden(sentence).is_empty());
    assert!(!find_forbidden(&format!("{sentence} It is fabricated.")).is_empty());
}
