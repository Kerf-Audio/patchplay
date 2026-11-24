//! Splits one `.kerf` line into words. Shared by the parser and by `diagnose`.

#[derive(Clone, Copy)]
pub(super) struct Token<'a> {
    pub(super) text: &'a str,
    pub(super) quoted: bool,
    /// Byte offsets into the trimmed line, quotes included.
    pub(super) start: usize,
    pub(super) end: usize,
}

impl Token<'_> {
    /// Byte offset of `text` in the trimmed line: past the opening quote of a
    /// quoted word. Spans over part of `text` are measured from here.
    pub(super) fn body(&self) -> usize {
        self.start + usize::from(self.quoted)
    }
}

pub(super) fn tokenize(line: &str) -> Result<Vec<Token<'_>>, String> {
    let mut out = Vec::new();
    let mut rest = line.trim_start();
    while !rest.is_empty() {
        let offset = line.len() - rest.len();
        if let Some(after) = rest.strip_prefix('"') {
            let end = after.find('"').ok_or("unclosed quote")?;
            out.push(Token {
                text: &after[..end],
                quoted: true,
                start: offset,
                end: offset + end + 2,
            });
            rest = &after[end + 1..];
            if !rest.is_empty() && !rest.starts_with(char::is_whitespace) {
                return Err("expected a space after the closing quote".into());
            }
        } else {
            let word_end = rest.find(char::is_whitespace).unwrap_or(rest.len());
            // `key="a value with spaces"` is one token, quotes kept.
            let end = match rest[..word_end].find("=\"") {
                Some(eq) => {
                    let close = rest[eq + 2..].find('"').ok_or("unclosed quote")?;
                    let end = eq + 2 + close + 1;
                    if !rest[end..].is_empty() && !rest[end..].starts_with(char::is_whitespace) {
                        return Err("expected a space after the closing quote".into());
                    }
                    end
                }
                None if rest[..word_end].contains('"') => {
                    return Err(format!("stray quote in `{}`", &rest[..word_end]));
                }
                None => word_end,
            };
            out.push(Token {
                text: &rest[..end],
                quoted: false,
                start: offset,
                end: offset + end,
            });
            rest = &rest[end..];
        }
        rest = rest.trim_start();
    }
    Ok(out)
}
