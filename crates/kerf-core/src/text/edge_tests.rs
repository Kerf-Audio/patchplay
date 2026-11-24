//! Awkward input to the parser and to `diagnose`: byte order marks, quoted
//! words with multi-byte text, and lines the parser never reached.

use super::*;
use crate::descriptor::{Band, JackKind, ModuleDescriptor, jack, param};
use crate::{Catalogue, Patch};

static CHISEL: ModuleDescriptor = ModuleDescriptor {
    kind: "Chisel",
    band: Band::Voices,
    hp: 4,
    summary: "",
    params: &[param("tune", "Tune", 0.5, "")],
    inputs: &[jack("voct", "V/oct", JackKind::Pitch, "")],
    outputs: &[jack("out", "Out", JackKind::Audio, "")],
};

static ALL: [&ModuleDescriptor; 1] = [&CHISEL];

struct Stub;

impl Catalogue for Stub {
    fn get(&self, kind: &str) -> Option<&'static ModuleDescriptor> {
        (kind == "Chisel").then_some(&CHISEL)
    }

    fn all(&self) -> &'static [&'static ModuleDescriptor] {
        &ALL
    }
}

#[test]
fn diagnose_measures_spans_inside_quoted_words_from_the_word_not_the_quote() {
    // The bad header stops the parser, so every later line reaches `diagnose`
    // unparsed; each one has a multi-byte character right after a quote.
    let src = "kerf 2\n\
               module a Chisel \"éé=1\"\n\
               cable \"é.x\" -> a.voct\n\
               cable a.out -> \"a.é\"\n\
               scene A \"a.é=1\"\n\
               scene B \"é.x=1\"\n";
    let spans: Vec<_> = diagnose(&Stub, src)
        .iter()
        .filter(|d| d.line > 1)
        .map(|d| (d.line, d.col, d.end_col))
        .collect();
    assert_eq!(spans, [(2, 18, 20), (3, 8, 9), (4, 19, 21), (5, 12, 13), (6, 10, 11)]);
}

#[test]
fn a_byte_order_mark_is_not_part_of_the_header() {
    assert!(parse("\u{feff}kerf 1\ntempo 100\n").is_ok());
    let d = diagnose(&Stub, "\u{feff}kerf 1\nmodule a Chisel zzz=1\n");
    assert_eq!(d.len(), 1, "{d:?}");
    assert_eq!((d[0].line, d[0].col, d[0].end_col), (2, 17, 20));
}

#[test]
fn a_directive_cannot_be_quoted() {
    let e = parse("kerf 1\n\"key\" A minor\n").unwrap_err();
    assert_eq!((e.line, e.col), (2, 1));
    assert!(e.message.contains("without quotes"), "{}", e.message);
}

#[test]
fn a_key_with_quotes_is_refused_so_the_text_round_trips() {
    let e = parse("kerf 1\nkey Bb \"dorian\"\n").unwrap_err();
    assert_eq!(e.line, 2);
    assert!(e.message.contains("double quotes"), "{}", e.message);
}

#[test]
fn key_and_tag_rules() {
    assert!(Patch::valid_key("A minor"));
    for bad in ["", " A", "A ", "A \"minor\"", "A\nminor"] {
        assert!(!Patch::valid_key(bad), "{bad:?}");
    }
    assert!(Patch::valid_tag("four-four"));
    for bad in ["", "4four", "Four", "a_b", "é"] {
        assert!(!Patch::valid_tag(bad), "{bad:?}");
    }
}
