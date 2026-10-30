use std::collections::BTreeMap;
use std::path::Path;

use serde::Deserialize;

use crate::error::{Error, Result};

pub const SEMANTIC: &[&str] = &[
    "heading",
    "muted",
    "accent",
    "identifier",
    "path",
    "number",
    "success",
    "warning",
    "error",
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Style {
    /// A complete SGR sequence, or empty for the default style.
    pub sgr: Vec<u8>,
}

impl Style {
    pub fn parse(value: &str) -> Result<Self> {
        let mut params = Vec::new();
        for token in value.split_whitespace() {
            match token {
                "default" | "none" => {}
                "bold" => params.push("1"),
                "dim" | "dark" => params.push("2"),
                "italic" => params.push("3"),
                "underline" => params.push("4"),
                "blink" => params.push("5"),
                "rapidblink" => params.push("6"),
                "reverse" => params.push("7"),
                "concealed" => params.push("8"),
                "strikethrough" => params.push("9"),
                "black" => params.push("30"),
                "red" => params.push("31"),
                "green" => params.push("32"),
                "yellow" => params.push("33"),
                "blue" => params.push("34"),
                "magenta" => params.push("35"),
                "cyan" => params.push("36"),
                "white" => params.push("37"),
                "bright_black" => params.extend(["30", "90"]),
                "bright_red" => params.extend(["31", "91"]),
                "bright_green" => params.extend(["32", "92"]),
                "bright_yellow" => params.extend(["33", "93"]),
                "bright_blue" => params.extend(["34", "94"]),
                "bright_magenta" => params.extend(["35", "95"]),
                "bright_cyan" => params.extend(["36", "96"]),
                "bright_white" => params.extend(["37", "97"]),
                "on_black" => params.push("40"),
                "on_red" => params.push("41"),
                "on_green" => params.push("42"),
                "on_yellow" => params.push("43"),
                "on_blue" => params.push("44"),
                "on_magenta" => params.push("45"),
                "on_cyan" => params.push("46"),
                "on_white" => params.push("47"),
                "on_bright_black" => params.extend(["40", "100"]),
                "on_bright_red" => params.extend(["41", "101"]),
                "on_bright_green" => params.extend(["42", "102"]),
                "on_bright_yellow" => params.extend(["43", "103"]),
                "on_bright_blue" => params.extend(["44", "104"]),
                "on_bright_magenta" => params.extend(["45", "105"]),
                "on_bright_cyan" => params.extend(["46", "106"]),
                "on_bright_white" => params.extend(["47", "107"]),
                other => return Err(Error::new(format!("unknown style token `{}`", other))),
            }
        }
        if params.is_empty() {
            return Ok(Self { sgr: Vec::new() });
        }
        let mut sgr = Vec::with_capacity(8 + params.len() * 3);
        sgr.extend_from_slice(b"\x1b[0;");
        sgr.extend_from_slice(params.join(";").as_bytes());
        sgr.push(b'm');
        Ok(Self { sgr })
    }

    pub fn is_default(&self) -> bool {
        self.sgr.is_empty()
    }
}

#[derive(Clone, Debug)]
pub struct Palette {
    styles: Vec<Style>,
    index: BTreeMap<String, usize>,
}

impl Default for Palette {
    fn default() -> Self {
        let defaults = [
            ("heading", "bold"),
            ("muted", "dim"),
            ("accent", "bold cyan"),
            ("identifier", "cyan"),
            ("path", "blue"),
            ("number", "magenta"),
            ("success", "green"),
            ("warning", "yellow"),
            ("error", "bold red"),
        ];
        let mut styles = vec![Style { sgr: Vec::new() }];
        let mut index = BTreeMap::new();
        for (name, value) in defaults {
            index.insert(name.to_string(), styles.len());
            styles.push(Style::parse(value).expect("valid built-in style"));
        }
        Self { styles, index }
    }
}

impl Palette {
    pub fn load(explicit: Option<&Path>) -> Result<Self> {
        let mut palette = Self::default();
        let path = match explicit {
            Some(path) => Some(path.to_path_buf()),
            None => user_palette_path(),
        };
        let Some(path) = path else {
            return Ok(palette);
        };
        if !path.is_file() {
            if explicit.is_some() {
                return Err(Error::new(format!(
                    "palette file [{}] not found",
                    path.display()
                )));
            }
            return Ok(palette);
        }
        let text = std::fs::read_to_string(&path)
            .map_err(|e| Error::new(format!("{}: {}", path.display(), e)))?;
        let file: PaletteFile =
            toml::from_str(&text).map_err(|e| Error::new(format!("{}: {}", path.display(), e)))?;
        for (name, value) in file.palette {
            let Some(&slot) = palette.index.get(&name) else {
                return Err(Error::new(format!(
                    "{}: unknown semantic style `{}`",
                    path.display(),
                    name
                )));
            };
            palette.styles[slot] = Style::parse(&value)
                .map_err(|e| Error::new(format!("{}: {}: {}", path.display(), name, e.message)))?;
        }
        Ok(palette)
    }

    pub fn style(&self, name: &str) -> Option<usize> {
        self.index.get(name).copied()
    }

    pub fn get(&self, id: usize) -> &Style {
        &self.styles[id]
    }

    pub fn name(&self, id: usize) -> &str {
        self.index
            .iter()
            .find(|(_, value)| **value == id)
            .map(|(name, _)| name.as_str())
            .unwrap_or("default")
    }

    pub fn entries(&self) -> impl Iterator<Item = (&str, &Style)> {
        self.index
            .iter()
            .map(move |(name, id)| (name.as_str(), &self.styles[*id]))
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PaletteFile {
    #[serde(default)]
    palette: BTreeMap<String, String>,
}

fn user_palette_path() -> Option<std::path::PathBuf> {
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| home.map(|home| home.join(".config")))?;
    Some(config.join("linelight/palette.toml"))
}

#[cfg(test)]
mod tests {
    use super::{Palette, Style};

    #[test]
    fn style_parser_rejects_unknown() {
        assert!(Style::parse("bold chartreuse").is_err());
        assert!(Style::parse("\\033[31m").is_err());
    }

    #[test]
    fn named_style_combines() {
        let style = Style::parse("bold cyan").unwrap();
        assert_eq!(style.sgr, b"\x1b[0;1;36m");
    }

    #[test]
    fn default_palette_has_all_semantics() {
        let palette = Palette::default();
        for name in super::SEMANTIC {
            assert!(palette.style(name).is_some(), "missing {}", name);
        }
    }
}
