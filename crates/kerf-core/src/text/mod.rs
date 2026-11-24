//! The `.kerf` text format: one directive per line, readable and diffable.
//!
//! ```text
//! kerf 1
//! tempo 118
//! key A minor
//! style dub-chamber seed 4821
//! tags four-four
//!
//! module clock1 Tally tempo=0.5 swing=0 divide=0.5
//!
//! part bass "Bass" colour #3d63c9 lock
//!   module seq1 Dovetail steps_a=0.5 interlock=0.4
//!   module osc1 Chisel tune=0.25 shape=0.35 lock=tune
//!
//! cable clock1.clock -> seq1.clock
//! cable seq1.pitch -> osc1.voct
//!
//! scene A osc1.shape=0.35
//! ```
//!
//! Indented `module` lines belong to the part above; unindented ones (before
//! the first part) belong to no part. `#` starts a comment line. Params are
//! normalised `0..=1`. `parse` checks syntax and ranges only; whether a kind,
//! param or jack exists is checked against the catalogue by the session, and
//! `diagnose` reports both kinds of problem with line and column for editors.

mod check;
mod parse;
mod print;
mod token;

#[cfg(test)]
mod edge_tests;

pub use check::{Diagnostic, Severity, diagnose};
pub use parse::{ParseError, parse, parse_all};
pub use print::print;

/// Keys that mean something of their own on a `module` line (`lock`,
/// `lock=a,b`, `sample="…"`), so no module may use one as a param or jack
/// key. The catalogue tests check every descriptor against this list; add a
/// word here whenever the parser gives a module-line key a special meaning.
pub const RESERVED_MODULE_KEYS: [&str; 2] = ["lock", "sample"];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Cable, Module, ParamValue, Part, Patch, Scene, SceneValue, StyleRef};

    fn module(
        id: &str,
        kind: &str,
        part: Option<&str>,
        params: &[(&str, f32, bool)],
        locked: bool,
    ) -> Module {
        Module {
            id: id.parse().unwrap(),
            kind: kind.into(),
            part: part.map(|p| p.parse().unwrap()),
            params: params
                .iter()
                .map(|&(k, value, locked)| ParamValue {
                    key: k.parse().unwrap(),
                    value,
                    locked,
                })
                .collect(),
            locked,
            sample: None,
        }
    }

    fn sample() -> Patch {
        let mut p = Patch {
            tempo: 118.5,
            key: Some("A minor".into()),
            style: Some(StyleRef {
                name: "dub-chamber".into(),
                seed: 4821,
            }),
            tags: vec!["four-four".into(), "samples".into()],
            ..Patch::default()
        };
        p.parts = vec![
            Part {
                id: "drums".parse().unwrap(),
                name: "Drums".into(),
                colour: "#d9622b".parse().unwrap(),
                locked: true,
                muted: false,
                level: 1.0,
            },
            Part {
                id: "bass".parse().unwrap(),
                name: "Low End".into(),
                colour: "#3d63c9".parse().unwrap(),
                locked: false,
                muted: true,
                level: 0.8,
            },
        ];
        p.modules = vec![
            module(
                "clock1",
                "Tally",
                None,
                &[("tempo", 0.5, false), ("swing", 0.0, false)],
                false,
            ),
            module(
                "kick1",
                "Mortise",
                Some("drums"),
                &[("punch", 1.0, false)],
                true,
            ),
            module(
                "osc1",
                "Chisel",
                Some("bass"),
                &[("tune", 0.25, true), ("shape", 1.0 / 3.0, false)],
                false,
            ),
            module("flt1", "Plane", Some("bass"), &[], false),
            Module {
                sample: Some("kits/my drums/loop one.wav".into()),
                ..module(
                    "vnr1",
                    "Veneer",
                    Some("bass"),
                    &[("slice", 0.25, true)],
                    false,
                )
            },
            Module {
                sample: Some("/Users/me/Music/break.flac".into()),
                ..module("vnr2", "Veneer", Some("bass"), &[], true)
            },
        ];
        p.cables = vec![
            Cable {
                from: "clock1.clock".parse().unwrap(),
                to: "kick1.trig".parse().unwrap(),
            },
            Cable {
                from: "osc1.saw".parse().unwrap(),
                to: "flt1.in".parse().unwrap(),
            },
        ];
        p.scenes.insert(
            Scene::A,
            vec![SceneValue {
                module: "osc1".parse().unwrap(),
                param: "shape".parse().unwrap(),
                value: 0.35,
            }],
        );
        p.scenes.insert(Scene::C, vec![]);
        p
    }

    #[test]
    fn print_then_parse_is_identity() {
        let p = sample();
        let text = print(&p);
        assert_eq!(parse(&text).unwrap(), p, "\n{text}");
        assert_eq!(print(&parse(&text).unwrap()), text);
    }

    #[test]
    fn sample_paths_round_trip_and_older_files_parse() {
        let text = print(&sample());
        assert!(
            text.contains(" slice=0.25 sample=\"kits/my drums/loop one.wav\" lock=slice"),
            "\n{text}"
        );
        assert!(
            text.contains("sample=\"/Users/me/Music/break.flac\" lock"),
            "\n{text}"
        );
        let old = parse("kerf 1\nmodule v1 Veneer slice=0.5\n").unwrap();
        assert_eq!(old.modules[0].sample, None);
        let spaced = parse("kerf 1\nmodule v1 Veneer sample=\"a b.wav\" slice=0.5 lock\n").unwrap();
        assert_eq!(spaced.modules[0].sample.as_deref(), Some("a b.wav"));
        assert!(spaced.modules[0].locked);
    }

    #[test]
    fn empty_patch_round_trips() {
        let p = Patch::default();
        assert_eq!(parse(&print(&p)).unwrap(), p);
    }

    #[test]
    fn tags_round_trip_and_stay_out_of_old_files() {
        let text = print(&sample());
        assert!(text.contains("\ntags four-four samples\n"), "\n{text}");
        assert!(!print(&Patch::default()).contains("tags"));
        let old = parse("kerf 1\ntempo 100\n").unwrap();
        assert!(old.tags.is_empty());
        let dup = parse("kerf 1\ntags crazy crazy abstract\n").unwrap();
        assert_eq!(dup.tags, ["crazy", "abstract"]);
        assert!(parse("kerf 1\ntags\n").is_err());
        assert!(parse("kerf 1\ntags Crazy\n").is_err());
        assert!(parse("kerf 1\ntags a\ntags b\n").is_err());
    }

    #[test]
    fn comments_blank_lines_and_spacing_are_ignored() {
        let src = "# a comment\n\nkerf 1\n  # indented comment\ntempo   90\n\npart a \"A b\"   colour #000000\n    module m1   Plane    cutoff=0.5\n";
        let p = parse(src).unwrap();
        assert_eq!(p.tempo, 90.0);
        assert_eq!(p.parts[0].name, "A b");
        assert_eq!(p.modules[0].part, Some("a".parse().unwrap()));
    }

    fn err_line(src: &str) -> (usize, String) {
        let e = parse(src).unwrap_err();
        (e.line, e.message)
    }

    #[test]
    fn errors_name_the_line() {
        let cases: &[(&str, usize, &str)] = &[
            ("tempo 120\n", 1, "header"),
            ("kerf 2\n", 1, "version"),
            ("kerf 1\n  module m1 Plane\n", 2, "part"),
            (
                "kerf 1\npart a \"A\" colour #000000\nmodule m1 Plane\n",
                3,
                "before the first part",
            ),
            ("kerf 1\nwobble 3\n", 2, "unknown"),
            ("kerf 1\nmodule m1 Plane cutoff=1.5\n", 2, "0..=1"),
            ("kerf 1\nmodule m1 Plane\nmodule m1 Chisel\n", 3, "m1"),
            ("kerf 1\npart a \"A colour #000000\n", 2, "quote"),
            ("kerf 1\ntempo 5\n", 2, "tempo"),
            ("kerf 1\ncable a.out b.in\n", 2, "->"),
            (
                "kerf 1\nmodule m1 Plane cutoff=0.1 cutoff=0.2\n",
                2,
                "twice",
            ),
            ("kerf 1\nmodule m1 Plane lock=res\n", 2, "res"),
            ("kerf 1\nscene A x.y=0.1\nscene A\n", 3, "scene A"),
            (
                "kerf 1\nmodule v1 Veneer sample=a.wav\n",
                2,
                "double quotes",
            ),
            ("kerf 1\nmodule v1 Veneer sample=\"\"\n", 2, "empty"),
            ("kerf 1\nmodule v1 Veneer sample=\"a.wav\n", 2, "quote"),
            (
                "kerf 1\nmodule v1 Veneer sample=\"a.wav\" sample=\"b.wav\"\n",
                2,
                "twice",
            ),
        ];
        for &(src, line, needle) in cases {
            let (got_line, msg) = err_line(src);
            assert_eq!(got_line, line, "{src:?}: {msg}");
            assert!(msg.contains(needle), "{src:?}: {msg:?} lacks {needle:?}");
        }
    }

    #[test]
    fn errors_point_at_the_token_to_blame() {
        // (source, line, col, end_col)
        let cases: &[(&str, usize, usize, usize)] = &[
            ("kerf 1\nwobble 3\n", 2, 1, 7),
            ("kerf 2\n", 1, 6, 7),
            ("kerf 1\nmodule m1 Plane cutoff=1.5\n", 2, 17, 27),
            ("kerf 1\nmodule m1 Plane\nmodule m1 Chisel\n", 3, 8, 10),
            ("kerf 1\ncable a.out => b.in\n", 2, 13, 15),
            ("kerf 1\ntempo 5\n", 2, 7, 8),
            ("kerf 1\npart a \"A\" colour #000000\n  wobble\n", 3, 3, 9),
            ("kerf 1\n  module m1 Plane x=y\n", 2, 10, 12),
        ];
        for &(src, line, col, end_col) in cases {
            let e = parse(src).unwrap_err();
            assert_eq!(
                (e.line, e.col, e.end_col),
                (line, col, end_col),
                "{src:?}: {e}"
            );
        }
        let e = parse("kerf 1\ntempo 5\n").unwrap_err();
        assert!(e.to_string().starts_with("line 2, column 7: "), "{e}");
    }

    #[test]
    fn parse_all_reports_every_bad_line_and_keeps_going() {
        let src = "kerf 1\ntempo 5\npart a \"A\" colour red\n  module m1 Plane\nwobble\ncable m1.out -> m1.in\n";
        let (patch, errors) = parse_all(src);
        let lines: Vec<_> = errors.iter().map(|e| e.line).collect();
        assert_eq!(lines, [2, 3, 5]);
        // The bad part line still owns the module under it.
        assert_eq!(patch.modules.len(), 1);
        assert_eq!(patch.cables.len(), 1);
        assert!(parse_all("tempo 120\nwobble\n").1.len() == 1);
    }
}
