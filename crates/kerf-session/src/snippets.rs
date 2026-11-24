//! The snippet library: ready-made parts to paste into a patch. The source is
//! one `.kerf` file per snippet in the repo's `snippets/` folder, embedded at
//! build time. The in-app docs, `docs/SNIPPETS.md`, `docs/snippets/*.md` and the
//! tests all read it through here, so there is no second copy to keep in step.
//!
//! A snippet file starts with `# key: value` lines (`title`, `about`, any
//! number of `port` and `feed`), then the `.kerf` lines of one part and the
//! cables that connect it to a `clock` and an `out` module.

use std::collections::BTreeSet;
use std::fmt::Write;
use std::sync::LazyLock;

/// A category: its folder name, its title, and a line on how its snippets behave.
pub struct Category {
    pub dir: &'static str,
    pub title: &'static str,
    pub blurb: &'static str,
}

pub const CATEGORIES: [Category; 9] = [
    Category {
        dir: "start",
        title: "Start",
        blurb: "Every other snippet cables to a module called `clock` and a module called `out`. Paste this first, or name your own clock and output the same.",
    },
    Category {
        dir: "drums",
        title: "Drums",
        blurb: "Each drum snippet is one part with its own rhythm lane, so drums layer without clashing. The kick, hats and snare are dead straight; only a few lanes use chance.",
    },
    Category {
        dir: "bass",
        title: "Bass",
        blurb: "Bass lines play notes of A minor, two octaves down. Change `root` and `scale` on the Fret to change the key.",
    },
    Category {
        dir: "lead",
        title: "Lead",
        blurb: "Lead lines play notes of A minor up in the middle of the range, and rest now and then because their `chance` is below 1.",
    },
    Category {
        dir: "pads",
        title: "Pads and chords",
        blurb: "Slow, sustained sounds. They follow the beat clock, so the notes change once a beat or more slowly.",
    },
    Category {
        dir: "notes",
        title: "Notes and chords",
        blurb: "Melodies that are not written note by note: a chord maker feeding an arpeggiator, and a tune drawn on a piano roll. They follow the beat clock; the arpeggio plays A minor.",
    },
    Category {
        dir: "percussion",
        title: "Percussion and texture",
        blurb: "Extra rhythm and background sound: shakers, toms, a struck bar, crackle and noise, plus a dub siren, a radio and a sampled grain cloud.",
    },
    Category {
        dir: "modulation",
        title: "Modulation",
        blurb: "Each snippet holds a slow source of movement and a held saw it moves, so it makes sound on its own. Patch the CV output into any other module to move that instead. Depth on each source scales how far it goes.",
    },
    Category {
        dir: "effects",
        title: "Effects",
        blurb: "Each effect has an input you feed with a sound and an output already cabled to `out`. Echoes, reverb, grains and FM are wet only: keep the source's own cable to `out` for the dry sound. For the filter, drive and ducking, take the source's cable to `out` away so you hear only the processed sound.",
    },
];

macro_rules! sources {
    ($($path:literal),* $(,)?) => {
        [$(($path, include_str!(concat!("../../../snippets/", $path, ".kerf")))),*]
    };
}

/// Every snippet file. Adding a snippet means adding its path here; a test
/// fails if a file in `snippets/` is missing from this list.
pub(crate) const SOURCES: [(&str, &str); 39] = sources![
    "start/clock-out",
    "drums/kick",
    "drums/hats",
    "drums/snare",
    "drums/clap",
    "drums/kit",
    "bass/acid",
    "bass/sub",
    "bass/pluck",
    "lead/saw",
    "lead/wavetable",
    "lead/supersaw",
    "pads/drift",
    "pads/wavetable",
    "pads/stab",
    "notes/chord-arp",
    "notes/drawn",
    "percussion/shaker",
    "percussion/toms",
    "percussion/mallet",
    "percussion/crackle",
    "percussion/wash",
    "percussion/siren",
    "percussion/radio",
    "percussion/grains",
    "modulation/rocker",
    "modulation/tread",
    "modulation/warp",
    "modulation/chatter",
    "effects/tape-echo",
    "effects/reverb",
    "effects/dub-chain",
    "effects/filter-sweep",
    "effects/drive",
    "effects/duck",
    "effects/sidechain",
    "effects/grain-cloud",
    "effects/dado",
    "effects/splice",
];

#[derive(Debug, Clone)]
pub struct Snippet {
    /// `category/name`, the file's path under `snippets/` without `.kerf`.
    pub id: &'static str,
    pub category: &'static str,
    pub title: String,
    pub about: String,
    /// Jacks worth connecting to, one line each: `module.jack (kind): what it is`.
    pub ports: Vec<String>,
    /// Audio inputs an effect is fed through. Empty for snippets that make their own sound.
    pub feed: Vec<String>,
    /// Modules the snippet's cables reach that it does not define (`clock`, `out`).
    pub needs: Vec<String>,
    /// The `.kerf` lines without the metadata.
    pub body: String,
    /// What gets pasted: a comment header (title, needs, ports) and then the body.
    pub code: String,
}

/// Adds `label text` as a comment, wrapped to 78 columns with a hanging indent.
fn comment(out: &mut String, label: &str, text: &str) {
    const INDENT: &str = "#   ";
    let mut line = format!("# {label}");
    for word in text.split_whitespace() {
        if line.len() + 1 + word.len() > 78 && line.len() > INDENT.len() {
            out.push_str(&line);
            out.push('\n');
            line = INDENT.to_owned();
        } else if !line.ends_with(' ') && line != "#" {
            line.push(' ');
        }
        line.push_str(word);
    }
    out.push_str(line.trim_end());
    out.push('\n');
}

fn needs_of(body: &str) -> Vec<String> {
    let mut defined = BTreeSet::new();
    let mut used = BTreeSet::new();
    for line in body.lines() {
        let words: Vec<&str> = line.split_whitespace().collect();
        match words.as_slice() {
            ["module", id, ..] => {
                defined.insert((*id).to_owned());
            }
            ["cable", from, "->", to] => {
                for jack in [from, to] {
                    if let Some((module, _)) = jack.split_once('.') {
                        used.insert(module.to_owned());
                    }
                }
            }
            _ => {}
        }
    }
    used.difference(&defined).cloned().collect()
}

fn parse_snippet(id: &'static str, source: &str) -> Snippet {
    let category = id.split_once('/').map_or(id, |(c, _)| c);
    let (mut title, mut about) = (None, None);
    let (mut ports, mut feed) = (Vec::new(), Vec::new());
    let mut rest = source;
    while let Some((line, tail)) = rest.split_once('\n') {
        let Some((key, value)) = line.strip_prefix("# ").and_then(|l| l.split_once(": ")) else {
            break;
        };
        match key {
            "title" => title = Some(value.to_owned()),
            "about" => about = Some(value.to_owned()),
            "port" => ports.push(value.to_owned()),
            "feed" => feed.extend(value.split(", ").map(str::to_owned)),
            _ => break,
        }
        rest = tail;
    }
    let title = title.unwrap_or_else(|| panic!("snippet {id} has no `# title:`"));
    let about = about.unwrap_or_else(|| panic!("snippet {id} has no `# about:`"));
    let body = rest.trim().to_owned() + "\n";
    let needs = needs_of(&body);

    let mut code = String::new();
    comment(&mut code, &title, "");
    comment(&mut code, "", &about);
    if !needs.is_empty() {
        comment(
            &mut code,
            "Needs:",
            &format!("{}. See the Clock and output snippet.", needs.join(", ")),
        );
    }
    if !feed.is_empty() {
        comment(&mut code, "Feed it audio:", &feed.join(", "));
    }
    for p in &ports {
        comment(&mut code, "Port:", p);
    }
    code.push('\n');
    code.push_str(&body);
    Snippet {
        id,
        category,
        title,
        about,
        ports,
        feed,
        needs,
        body,
        code,
    }
}

/// Every snippet, in the order of [`CATEGORIES`] and then the source list.
pub fn snippets() -> &'static [Snippet] {
    static ALL: LazyLock<Vec<Snippet>> = LazyLock::new(|| {
        let mut all: Vec<Snippet> = SOURCES
            .iter()
            .map(|(id, src)| parse_snippet(id, src))
            .collect();
        all.sort_by_key(|s| CATEGORIES.iter().position(|c| c.dir == s.category));
        all
    });
    &ALL
}

/// How the snippets are used; the first lines of the docs panel's Snippets section.
pub const INTRO: &str = "Each snippet is one `part`: its modules, plus the cables that connect it to a clock and the master output, as plain `.kerf` lines.

- In the app, **Add to patch** appends the snippet to the end of the code editor's draft. It does not apply anything: read it, change it, then apply the draft when you are ready.
- Every module id starts with the snippet's own prefix (`kick_`, `acid_`, `echo_`), so several snippets can sit in one patch without clashing.
- Sound snippets cable to a module called `clock` (a Tally) and one called `out` (a Finish). The Start snippet makes both; paste it first.
- A comment header on each snippet lists what it needs, what to feed an effect, and the jacks worth patching elsewhere.
- Set the tempo and key in the patch header. The snippets follow the tempo and play in A minor until you change their Fret.";

/// The markdown for one category, from its blurb down. It starts below a `##` heading.
pub fn category_markdown(category: &Category) -> String {
    let mut md = format!("{}\n", category.blurb);
    for s in snippets().iter().filter(|s| s.category == category.dir) {
        let _ = write!(
            md,
            "\n### {}\n\n{}\n\n```kerf snippet\n{}```\n",
            s.title, s.about, s.code
        );
    }
    md
}

/// The generated doc files, as (path under `docs/`, content): an index and one
/// page per category. A test compares them with the files on disk.
pub fn doc_files() -> Vec<(String, String)> {
    let mut index = String::from(
        "# Snippets\n\nParts to paste into the code editor: drums, bass, lead, pads, chords and melodies, percussion, modulation and effects. This page and the pages it links to are generated from the `.kerf` files in `snippets/`, so edit those rather than the pages.\n\n",
    );
    index.push_str(INTRO);
    index.push_str("\n\nSee [WRITING-PATCHES.md](WRITING-PATCHES.md) for how to write a patch from scratch and [KERF-FORMAT.md](KERF-FORMAT.md) for the full format.\n\n## Pages\n\n");
    let mut files = Vec::new();
    for c in &CATEGORIES {
        let titles: Vec<&str> = snippets()
            .iter()
            .filter(|s| s.category == c.dir)
            .map(|s| s.title.as_str())
            .collect();
        let _ = writeln!(
            index,
            "- [{}](snippets/{}.md): {}",
            c.title,
            c.dir,
            titles.join(", ")
        );
        files.push((
            format!("snippets/{}.md", c.dir),
            format!("# {}\n\n{}", c.title, category_markdown(c)),
        ));
    }
    files.insert(0, ("SNIPPETS.md".to_owned(), index));
    files
}
