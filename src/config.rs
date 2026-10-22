//! Materialise and upgrade a single `config.toml` holding both query
//! prefixes and keyboard shortcuts.
//!
//! [`ConfigFile::ensure`] creates the file from a template, migrates the old
//! split `prefixes.toml`/`shortcuts.toml` files, and appends built-in entries
//! added by a newer version without touching the user's customisations. A file
//! that does not parse is left alone: the picker reports the error when it
//! loads, instead of overwriting the user's edits.

use std::path::{Path, PathBuf};

use crate::{parse_prefixes, parse_shortcuts};

/// Builder for one app's unified config file.
///
/// ```no_run
/// use r_modals::ConfigFile;
///
/// ConfigFile::at("/tmp/my-app/config.toml")
///     .template("[[prefix]]\nprefix = \">\"\nshell = \"true\"\n")
///     .migrate_from(["/tmp/my-app/prefixes.toml", "/tmp/my-app/shortcuts.toml"])
///     .ensure()
///     .unwrap();
/// ```
#[derive(Debug, Clone)]
pub struct ConfigFile {
    path: PathBuf,
    template: Option<String>,
    vars: Vec<(String, String)>,
    legacy: Vec<PathBuf>,
}

impl ConfigFile {
    /// Target path of the unified file.
    pub fn at(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            template: None,
            vars: Vec::new(),
            legacy: Vec::new(),
        }
    }

    /// Template used when the file does not exist yet (usually
    /// `include_str!("../config.toml")`).
    pub fn template(mut self, text: impl Into<String>) -> Self {
        self.template = Some(text.into());
        self
    }

    /// Replace `{name}` with `value` in the template.
    ///
    /// This is applied when the file is first created and to built-in blocks
    /// appended on upgrade. Runtime tokens such as `{query}` are left for the
    /// picker; callers typically set `exe` to a shell-quoted executable path.
    ///
    /// Values are escaped for a TOML basic string (`"…"`), so a Windows path
    /// such as `C:\Program Files\App.exe` or a path containing quotes cannot
    /// break out of the string and inject extra config entries. Values are
    /// inserted in a single pass: braces inside a value are not expanded
    /// again.
    pub fn var(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.vars.push((name.into(), value.into()));
        self
    }

    /// Old split files to fold into the new file when it does not exist yet.
    ///
    /// The legacy files are removed only after the unified file has been
    /// written successfully. They are left untouched when the target already
    /// exists.
    pub fn migrate_from<I, P>(mut self, paths: I) -> Self
    where
        I: IntoIterator<Item = P>,
        P: Into<PathBuf>,
    {
        self.legacy.extend(paths.into_iter().map(Into::into));
        self
    }

    /// The unified file's path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Current file contents (`io::ErrorKind::NotFound` when absent).
    pub fn read(&self) -> std::io::Result<String> {
        std::fs::read_to_string(&self.path)
    }

    /// Create the file if needed, then append missing built-in entries.
    ///
    /// Returns `true` when the file was written. A malformed existing file is
    /// deliberately left untouched and yields `Ok(false)`; the picker reports
    /// the parse error when it loads the file.
    pub fn ensure(&self) -> std::io::Result<bool> {
        let mut seeded = false;
        if !self.path.exists() {
            seeded = self.seed()?;
        }
        let Ok(content) = std::fs::read_to_string(&self.path) else {
            return Ok(false);
        };
        let mut content = content;
        let mut changed = false;

        // Handle each section independently: a broken shortcut must not stop
        // new prefix entries from being added (and vice versa).
        if let Some(template) = &self.template {
            if let Ok(existing) = parse_prefixes(&content) {
                let mut names: Vec<String> = existing.into_iter().map(|p| p.prefix).collect();
                for (name, block) in embedded_blocks(template, "[[prefix]]", |block| {
                    parse_prefixes(block)
                        .ok()
                        .and_then(|v| v.into_iter().next())
                        .map(|p| p.prefix)
                }) {
                    if names.contains(&name) {
                        continue;
                    }
                    append_block(&mut content, &self.substitute(&block));
                    names.push(name);
                    changed = true;
                }
            }
            if let Ok(existing) = parse_shortcuts(&content) {
                let mut names: Vec<String> = existing.into_iter().map(|s| s.key).collect();
                for (name, block) in embedded_blocks(template, "[[shortcut]]", |block| {
                    parse_shortcuts(block)
                        .ok()
                        .and_then(|v| v.into_iter().next())
                        .map(|s| s.key)
                }) {
                    if names.contains(&name) {
                        continue;
                    }
                    append_block(&mut content, &self.substitute(&block));
                    names.push(name);
                    changed = true;
                }
            }
        }

        if changed {
            crate::fsutil::write_atomic(&self.path, content.as_bytes())?;
        }
        Ok(changed || seeded)
    }

    /// Seed a missing file from the legacy files, or from the template.
    fn seed(&self) -> std::io::Result<bool> {
        let mut content = String::new();
        let mut migrated = Vec::new();
        for legacy in &self.legacy {
            let Ok(text) = std::fs::read_to_string(legacy) else {
                continue;
            };
            if text.trim().is_empty() {
                continue;
            }
            if !content.is_empty() && !content.ends_with('\n') {
                content.push('\n');
            }
            content.push_str(&text);
            migrated.push(legacy.clone());
        }

        if content.is_empty() {
            if let Some(template) = &self.template {
                content = self.substitute(template);
            }
        }
        if content.is_empty() {
            return Ok(false);
        }

        crate::fsutil::write_atomic(&self.path, content.as_bytes())?;
        for path in migrated {
            let _ = std::fs::remove_file(path);
        }
        Ok(true)
    }

    /// One-pass `{name}` substitution with TOML string escaping.
    ///
    /// A single pass (rather than `str::replace` per variable) means a value
    /// containing `{other}` is inserted literally instead of being expanded
    /// by a later variable. Unknown tokens, notably the runtime `{query}`,
    /// are preserved for the picker.
    fn substitute(&self, text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        let mut rest = text;
        while let Some(start) = rest.find('{') {
            out.push_str(&rest[..start]);
            let after = &rest[start + 1..];
            let Some(end) = after.find('}') else {
                // Unbalanced brace: keep the remainder verbatim.
                out.push_str(&rest[start..]);
                return out;
            };
            let name = &after[..end];
            match self.vars.iter().find(|(n, _)| n == name) {
                Some((_, value)) => out.push_str(&toml_escape(value)),
                None => {
                    out.push('{');
                    out.push_str(name);
                    out.push('}');
                }
            }
            rest = &after[end + 1..];
        }
        out.push_str(rest);
        out
    }
}

/// Escape a value for a TOML basic string. Prevents a variable (typically an
/// executable path) from terminating the surrounding `"…"` and injecting
/// additional tables or keys into the generated config.
fn toml_escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\u{:04X}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// Append a `[[section]]` block after a blank line, preserving the tail style.
fn append_block(content: &mut String, block: &str) {
    if !content.ends_with('\n') {
        content.push('\n');
    }
    content.push('\n');
    content.push_str(block.trim_end());
    content.push('\n');
}

/// Entry blocks of `table` in `template`, paired with the key returned by
/// `name_of`.
///
/// Only a line that starts a table (`[[…`) ends a body, so a comment that
/// mentions a section name is never mistaken for one.
fn embedded_blocks(
    template: &str,
    table: &str,
    name_of: impl Fn(&str) -> Option<String>,
) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut body: Option<String> = None;

    fn flush(
        body: &mut Option<String>,
        out: &mut Vec<(String, String)>,
        name_of: &impl Fn(&str) -> Option<String>,
    ) {
        let Some(text) = body.take() else {
            return;
        };
        if let Some(name) = name_of(&text) {
            out.push((name, text.trim_end().to_string()));
        }
    }

    for line in template.lines() {
        let start = line.trim_start();
        if start.starts_with("[[") {
            flush(&mut body, &mut out, &name_of);
            if start == table || start.starts_with(&format!("{table} ")) {
                body = Some(format!("{line}\n"));
            }
            continue;
        }
        if let Some(text) = body.as_mut() {
            text.push_str(line);
            text.push('\n');
        }
    }
    flush(&mut body, &mut out, &name_of);
    out
}

#[cfg(test)]
mod tests;
