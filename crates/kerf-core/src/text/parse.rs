use std::str::FromStr;

use super::token::{Token, tokenize};
use crate::{
    Cable, JackRef, Key, Module, ModuleId, ParamValue, Part, PartId, Patch, Scene, SceneValue,
    StyleRef,
};

/// A syntax or range error. `line` and `col` are 1-based; the span covers the
/// characters `col..end_col` of the line (`end_col` is one past the last).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("line {line}, column {col}: {message}")]
pub struct ParseError {
    pub line: usize,
    pub col: usize,
    pub end_col: usize,
    pub message: String,
}

/// A failure on one line: what is wrong, and where when a token is to blame
/// (otherwise the whole line).
struct Fail {
    message: String,
    span: Option<(usize, usize)>,
}

impl Fail {
    fn at(t: &Token, message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            span: Some((t.start, t.end)),
        }
    }
}

impl From<String> for Fail {
    fn from(message: String) -> Self {
        Self {
            message,
            span: None,
        }
    }
}

impl From<&str> for Fail {
    fn from(message: &str) -> Self {
        message.to_owned().into()
    }
}

type R<T> = Result<T, Fail>;

/// Parses `.kerf` text. Checks syntax, ranges and duplicates; kinds, params
/// and jacks are checked against the catalogue when a session opens the patch.
pub fn parse(src: &str) -> Result<Patch, ParseError> {
    let (patch, mut errors) = parse_all(src);
    if errors.is_empty() {
        Ok(patch)
    } else {
        Err(errors.swap_remove(0))
    }
}

/// Parses as far as possible, collecting one error per bad line. The patch is
/// only meaningful when the list is empty.
pub fn parse_all(src: &str) -> (Patch, Vec<ParseError>) {
    let mut st = State::default();
    let mut errors = Vec::new();
    let mut last_line = 0;
    for (i, raw) in strip_bom(src).lines().enumerate() {
        last_line = i + 1;
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let indent = raw.len() - raw.trim_start().len();
        let indented = indent > 0;
        if let Err(f) = st.line(trimmed, indented) {
            errors.push(error_at(raw, indent, trimmed, i + 1, f));
            if !st.header {
                break;
            }
        }
    }
    if !st.header && errors.is_empty() {
        errors.push(ParseError {
            line: last_line.max(1),
            col: 1,
            end_col: 2,
            message: "missing `kerf 1` header".into(),
        });
    }
    (st.patch, errors)
}

/// Text saved by some editors starts with a byte order mark; it is not part of
/// the first line.
pub(super) fn strip_bom(src: &str) -> &str {
    src.strip_prefix('\u{feff}').unwrap_or(src)
}

fn error_at(raw: &str, indent: usize, trimmed: &str, line: usize, f: Fail) -> ParseError {
    let (start, end) = f.span.unwrap_or((0, trimmed.len()));
    let cols = |bytes: usize| raw[..indent].chars().count() + trimmed[..bytes].chars().count() + 1;
    ParseError {
        line,
        col: cols(start),
        end_col: cols(end).max(cols(start) + 1),
        message: f.message,
    }
}

#[derive(Default)]
struct State {
    patch: Patch,
    header: bool,
    tempo: bool,
    current_part: Option<PartId>,
}

impl State {
    fn line(&mut self, line: &str, indented: bool) -> R<()> {
        let tokens = tokenize(line)?;
        let (head, args) = tokens.split_first().expect("non-empty line has a token");
        if head.quoted {
            return Err(Fail::at(head, "a directive is written without quotes"));
        }
        if !self.header {
            if head.text != "kerf" {
                return Err(Fail::at(head, "expected the `kerf 1` header first"));
            }
            let [v] = args else {
                return Err("expected `kerf <version>`".into());
            };
            if v.text != Patch::FORMAT_VERSION.to_string() {
                return Err(Fail::at(
                    v,
                    format!(
                        "unsupported format version `{}`; this build reads {}",
                        v.text,
                        Patch::FORMAT_VERSION
                    ),
                ));
            }
            self.header = true;
            return Ok(());
        }
        if indented && head.text != "module" {
            return Err(Fail::at(
                head,
                format!("only `module` lines are indented, found `{}`", head.text),
            ));
        }
        match head.text {
            "tempo" => self.tempo(args),
            "key" => self.key(line),
            "style" => self.style(args),
            "tags" => self.tags(args),
            "part" => self.part(args),
            "module" => self.module(args, indented),
            "cable" => self.cable(args),
            "scene" => self.scene(args),
            "kerf" => Err(Fail::at(head, "duplicate `kerf` header")),
            other => Err(Fail::at(head, format!("unknown directive `{other}`"))),
        }
    }

    fn tempo(&mut self, args: &[Token]) -> R<()> {
        if std::mem::replace(&mut self.tempo, true) {
            return Err("`tempo` given twice".into());
        }
        let [t] = args else {
            return Err("expected `tempo <bpm>`".into());
        };
        let bpm: f32 = number(t)?;
        if !Patch::TEMPO_RANGE.contains(&bpm) {
            return Err(Fail::at(
                t,
                format!("tempo {bpm} is outside {:?} bpm", Patch::TEMPO_RANGE),
            ));
        }
        self.patch.tempo = bpm;
        Ok(())
    }

    fn key(&mut self, line: &str) -> R<()> {
        if self.patch.key.is_some() {
            return Err("`key` given twice".into());
        }
        let key = line["key".len()..].trim();
        if key.is_empty() {
            return Err("expected `key <name>`, e.g. `key A minor`".into());
        }
        if !Patch::valid_key(key) {
            return Err("a key must not contain double quotes".into());
        }
        self.patch.key = Some(key.to_owned());
        Ok(())
    }

    fn style(&mut self, args: &[Token]) -> R<()> {
        if self.patch.style.is_some() {
            return Err("`style` given twice".into());
        }
        let usage = "expected `style <name> seed <n>`";
        let [name, seed_kw, seed] = args else {
            return Err(usage.into());
        };
        if name.quoted || seed_kw.text != "seed" {
            return Err(usage.into());
        }
        let seed = seed.text.parse().map_err(|_| {
            Fail::at(seed, format!("`{}` is not a seed (whole number)", seed.text))
        })?;
        self.patch.style = Some(StyleRef {
            name: name.text.to_owned(),
            seed,
        });
        Ok(())
    }

    fn tags(&mut self, args: &[Token]) -> R<()> {
        if !self.patch.tags.is_empty() {
            return Err("`tags` given twice".into());
        }
        if args.is_empty() {
            return Err("expected `tags <word> ...`, e.g. `tags four-four`".into());
        }
        for t in args {
            if t.quoted || !Patch::valid_tag(t.text) {
                return Err(Fail::at(
                    t,
                    format!(
                        "`{}` is not a tag: use lowercase letters, digits and `-`, starting with a letter",
                        t.text
                    ),
                ));
            }
            if !self.patch.tags.iter().any(|x| x == t.text) {
                self.patch.tags.push(t.text.to_owned());
            }
        }
        Ok(())
    }

    fn part(&mut self, args: &[Token]) -> R<()> {
        let usage = "expected `part <id> \"<name>\" colour #rrggbb [lock] [mute]`";
        let [id, name, colour_kw, colour, flags @ ..] = args else {
            return Err(usage.into());
        };
        // Keep following modules attached to this part even if the line is
        // bad, so one mistake does not bury the rest of the file in errors.
        if let Ok(pid) = id.text.parse::<PartId>() {
            self.current_part = Some(pid);
        }
        if !name.quoted || colour_kw.text != "colour" {
            return Err(usage.into());
        }
        let id: PartId = parsed(id)?;
        if self.patch.part(&id).is_some() {
            return Err(Fail::at(
                &args[0],
                format!("part `{id}` defined twice"),
            ));
        }
        if !Part::valid_name(name.text) {
            return Err(Fail::at(name, "part name must not be empty"));
        }
        let (mut locked, mut muted, mut level) = (false, false, None);
        for f in flags {
            match f.text.split_once('=') {
                None if f.text == "lock" && !locked => locked = true,
                None if f.text == "mute" && !muted => muted = true,
                Some(("level", v)) if level.is_none() => {
                    level = Some(unit_str(v).map_err(|m| Fail::at(f, m))?);
                }
                _ => {
                    return Err(Fail::at(
                        f,
                        format!("unexpected `{}` on part line", f.text),
                    ));
                }
            }
        }
        self.patch.parts.push(Part {
            id: id.clone(),
            name: name.text.to_owned(),
            colour: parsed(colour)?,
            locked,
            muted,
            level: level.unwrap_or(Part::unity()),
        });
        self.current_part = Some(id);
        Ok(())
    }

    fn module(&mut self, args: &[Token], indented: bool) -> R<()> {
        let [id_t, kind, rest @ ..] = args else {
            return Err("expected `module <id> <Kind> [key=value ...]`".into());
        };
        let part = match (indented, &self.current_part) {
            (true, Some(p)) => Some(p.clone()),
            (true, None) => {
                return Err(Fail::at(id_t, "indented module with no part above it"));
            }
            (false, None) => None,
            (false, Some(_)) => {
                return Err(Fail::at(
                    id_t,
                    "modules without a part must come before the first part",
                ));
            }
        };
        let id: ModuleId = parsed(id_t)?;
        if self.patch.module(&id).is_some() {
            return Err(Fail::at(id_t, format!("module `{id}` defined twice")));
        }
        if kind.text.is_empty() || !kind.text.chars().all(|c| c.is_ascii_alphanumeric()) {
            return Err(Fail::at(
                kind,
                format!("`{}` is not a module kind", kind.text),
            ));
        }
        let mut params: Vec<ParamValue> = Vec::new();
        let mut locks: Option<(&Token, &str)> = None;
        let mut locked = false;
        let mut sample: Option<String> = None;
        for t in rest {
            match t.text.split_once('=') {
                None if t.text == "lock" && !locked => locked = true,
                None => {
                    return Err(Fail::at(
                        t,
                        format!("unexpected `{}`; expected `key=value`", t.text),
                    ));
                }
                Some(("lock", keys)) if locks.is_none() => locks = Some((t, keys)),
                Some(("sample", path)) if sample.is_none() => {
                    let path = path
                        .strip_prefix('"')
                        .and_then(|p| p.strip_suffix('"'))
                        .ok_or_else(|| {
                            Fail::at(t, "expected `sample=\"<path>\"` with the path in double quotes")
                        })?;
                    if !Module::valid_sample_path(path) {
                        return Err(Fail::at(t, "sample path must not be empty"));
                    }
                    sample = Some(path.to_owned());
                }
                Some(("sample", _)) => return Err(Fail::at(t, "`sample` set twice")),
                Some((k, v)) => {
                    let key: Key = k.parse().map_err(|e: crate::IdError| Fail::at(t, e.to_string()))?;
                    if params.iter().any(|p| p.key == key) {
                        return Err(Fail::at(t, format!("`{key}` set twice")));
                    }
                    params.push(ParamValue {
                        key,
                        value: unit_str(v).map_err(|m| Fail::at(t, m))?,
                        locked: false,
                    });
                }
            }
        }
        if let Some((t, keys)) = locks {
            for k in keys.split(',') {
                let p = params
                    .iter_mut()
                    .find(|p| p.key.as_str() == k)
                    .ok_or_else(|| {
                        Fail::at(t, format!("`lock={k}` names a param with no value on this line"))
                    })?;
                p.locked = true;
            }
        }
        self.patch.modules.push(Module {
            id,
            kind: kind.text.to_owned(),
            part,
            params,
            locked,
            sample,
        });
        Ok(())
    }

    fn cable(&mut self, args: &[Token]) -> R<()> {
        let usage = "expected `cable <module.jack> -> <module.jack>`";
        let [from, arrow, to] = args else {
            return Err(usage.into());
        };
        if arrow.text != "->" {
            return Err(Fail::at(arrow, usage));
        }
        let cable = Cable {
            from: parsed::<JackRef>(from)?,
            to: parsed::<JackRef>(to)?,
        };
        if self.patch.cables.contains(&cable) {
            return Err(format!("cable {} -> {} given twice", cable.from, cable.to).into());
        }
        self.patch.cables.push(cable);
        Ok(())
    }

    fn scene(&mut self, args: &[Token]) -> R<()> {
        let [slot, values @ ..] = args else {
            return Err("expected `scene <A|B|C|D> [module.param=value ...]`".into());
        };
        let scene: Scene = parsed(slot)?;
        if self.patch.scenes.contains_key(&scene) {
            return Err(Fail::at(slot, format!("scene {scene} given twice")));
        }
        let values = values
            .iter()
            .map(|t| {
                let (target, v) = t.text.split_once('=').ok_or_else(|| {
                    Fail::at(
                        t,
                        format!("expected `module.param=value`, found `{}`", t.text),
                    )
                })?;
                let r: JackRef = target
                    .parse()
                    .map_err(|e: crate::IdError| Fail::at(t, e.to_string()))?;
                Ok(SceneValue {
                    module: r.module,
                    param: r.jack,
                    value: unit_str(v).map_err(|m| Fail::at(t, m))?,
                })
            })
            .collect::<R<Vec<_>>>()?;
        self.patch.scenes.insert(scene, values);
        Ok(())
    }
}

fn parsed<T: FromStr>(t: &Token) -> R<T>
where
    T::Err: std::fmt::Display,
{
    t.text.parse().map_err(|e: T::Err| Fail::at(t, e.to_string()))
}

fn number(t: &Token) -> R<f32> {
    number_str(t.text).map_err(|m| Fail::at(t, m))
}

fn number_str(s: &str) -> Result<f32, String> {
    match s.parse::<f32>() {
        Ok(v) if v.is_finite() => Ok(v),
        _ => Err(format!("`{s}` is not a number")),
    }
}

fn unit_str(s: &str) -> Result<f32, String> {
    let v = number_str(s)?;
    if (0.0..=1.0).contains(&v) {
        Ok(v)
    } else {
        Err(format!("value {v} is outside 0..=1"))
    }
}
