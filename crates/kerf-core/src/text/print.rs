use std::fmt::Write;

use crate::{Module, Part, Patch};

/// Prints a patch as `.kerf` text. Floats use Rust's shortest round-trip
/// form, so parsing the output gives back the same values.
pub fn print(p: &Patch) -> String {
    let mut o = String::new();
    // Writing to a String cannot fail.
    let _ = writeln!(o, "kerf {}", Patch::FORMAT_VERSION);
    let _ = writeln!(o, "tempo {}", p.tempo);
    if let Some(key) = &p.key {
        let _ = writeln!(o, "key {key}");
    }
    if let Some(style) = &p.style {
        let _ = writeln!(o, "style {} seed {}", style.name, style.seed);
    }
    if !p.tags.is_empty() {
        let _ = writeln!(o, "tags {}", p.tags.join(" "));
    }

    let id_w = p
        .modules
        .iter()
        .map(|m| m.id.as_str().len())
        .max()
        .unwrap_or(0);
    let kind_w = p.modules.iter().map(|m| m.kind.len()).max().unwrap_or(0);
    let widths = (id_w, kind_w);

    let loose: Vec<&Module> = p.modules.iter().filter(|m| m.part.is_none()).collect();
    if !loose.is_empty() {
        o.push('\n');
        for m in loose {
            module_line(&mut o, m, "", widths);
        }
    }

    for part in &p.parts {
        let _ = write!(
            o,
            "\npart {} \"{}\" colour {}",
            part.id, part.name, part.colour
        );
        if part.level != Part::unity() {
            let _ = write!(o, " level={}", part.level);
        }
        if part.locked {
            o.push_str(" lock");
        }
        if part.muted {
            o.push_str(" mute");
        }
        o.push('\n');
        for m in p
            .modules
            .iter()
            .filter(|m| m.part.as_ref() == Some(&part.id))
        {
            module_line(&mut o, m, "  ", widths);
        }
    }

    if !p.cables.is_empty() {
        o.push('\n');
        for c in &p.cables {
            let _ = writeln!(o, "cable {} -> {}", c.from, c.to);
        }
    }

    if !p.scenes.is_empty() {
        o.push('\n');
        for (scene, values) in &p.scenes {
            let _ = write!(o, "scene {scene}");
            for v in values {
                let _ = write!(o, " {}.{}={}", v.module, v.param, v.value);
            }
            o.push('\n');
        }
    }
    o
}

fn module_line(o: &mut String, m: &Module, indent: &str, (id_w, kind_w): (usize, usize)) {
    let _ = write!(
        o,
        "{indent}module {:<id_w$} {:<kind_w$}",
        m.id.as_str(),
        m.kind
    );
    for p in &m.params {
        let _ = write!(o, " {}={}", p.key, p.value);
    }
    if let Some(path) = &m.sample {
        let _ = write!(o, " sample=\"{path}\"");
    }
    let mut locked = m.params.iter().filter(|p| p.locked).map(|p| p.key.as_str());
    if let Some(first) = locked.next() {
        let _ = write!(o, " lock={first}");
        for k in locked {
            let _ = write!(o, ",{k}");
        }
    }
    if m.locked {
        o.push_str(" lock");
    }
    // Padding leaves trailing spaces on modules without params.
    let trimmed = o.trim_end_matches(' ').len();
    o.truncate(trimmed);
    o.push('\n');
}
