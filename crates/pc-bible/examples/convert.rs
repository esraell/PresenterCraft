//! Makes a PresenterCraft Bible file from one-verse-per-line text as eBible.org publishes it
//! (the `_vpl.txt` file in a translation's VPL zip).
//!
//! cargo run -p pc-bible --example convert -- IN.txt OUT.pcbible id=kjv "name=King James Version" \
//!     short=KJV language=en "terms=Public domain" credit= "source=eBible.org eng-kjv2006" numbering=english
//!
//! Add `tidy=kjv` for the King James text, whose source marks paragraphs with a pilcrow and
//! the translators' added words with square brackets; the marks are dropped and the words kept.

use pc_bible::{Bible, Info, Numbering};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [input, output, settings @ ..] = args.as_slice() else {
        return eprintln!("usage: convert IN.txt OUT.pcbible key=value ...");
    };
    let get = |key: &str| {
        settings
            .iter()
            .find_map(|s| s.strip_prefix(key).and_then(|rest| rest.strip_prefix('=')))
            .unwrap_or("")
            .to_owned()
    };
    let Some(numbering) = Numbering::from_name(&get("numbering")) else {
        return eprintln!("numbering= must be english, synodal or original");
    };
    let info = Info {
        id: get("id"),
        name: get("name"),
        short: get("short"),
        language: get("language"),
        terms: get("terms"),
        credit: get("credit"),
        source: get("source"),
        numbering,
        loose: get("fit") == "loose",
    };
    if info.id.is_empty() || info.name.is_empty() {
        return eprintln!("id= and name= are needed");
    }
    let text = match std::fs::read_to_string(input) {
        Ok(text) => text,
        Err(e) => return eprintln!("Could not read {input}: {e}"),
    };
    let kjv = get("tidy") == "kjv";
    let bible = Bible::from_verse_lines(info, &text, |words| {
        let words = if kjv { words.replace(['[', ']', '¶'], "") } else { words.to_owned() };
        // Single spaces only, none at the ends.
        words.split_whitespace().collect::<Vec<_>>().join(" ")
    });
    let bytes = bible.to_bytes();
    match std::fs::write(output, &bytes) {
        Ok(()) => println!("{output}: {} verses, {} bytes", bible.verse_count(), bytes.len()),
        Err(e) => eprintln!("Could not write {output}: {e}"),
    }
}
