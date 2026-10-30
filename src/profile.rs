use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use regex::{Regex, RegexBuilder};
use serde::Deserialize;

use crate::error::{Error, Result};
use crate::profiles::EMBEDDED;
use crate::style::{Palette, SEMANTIC};

#[derive(Clone, Debug)]
pub enum Source {
    Embedded,
    File(PathBuf),
}

impl fmt::Display for Source {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Source::Embedded => f.write_str("embedded"),
            Source::File(path) => write!(f, "{}", path.display()),
        }
    }
}

#[derive(Clone, Debug)]
pub struct RawProfile {
    pub id: String,
    pub name: String,
    pub description: String,
    pub source: Source,
    rules: Vec<RuleFile>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProfileFile {
    schema: u32,
    id: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(rename = "rule")]
    rules: Vec<RuleFile>,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
struct RuleFile {
    regex: String,
    #[serde(default)]
    style: Option<String>,
    #[serde(default)]
    groups: BTreeMap<String, String>,
}

#[derive(Clone, Debug)]
pub struct CompiledProfile {
    pub id: String,
    pub name: String,
    pub description: String,
    pub source: Source,
    pub rules: Vec<CompiledRule>,
}

#[derive(Clone, Debug)]
pub struct CompiledRule {
    pub regex: Regex,
    pub style: Option<usize>,
    pub groups: Vec<(String, usize)>,
}

pub struct ProfileSet {
    pub entries: BTreeMap<String, RawProfile>,
    pub errors: Vec<String>,
}

impl ProfileSet {
    pub fn discover(extra_dirs: &[PathBuf]) -> Self {
        let mut entries = BTreeMap::new();
        let mut errors = Vec::new();

        for (id, text) in EMBEDDED {
            match parse_profile_str(Source::Embedded, text) {
                Ok(profile) => {
                    if profile.id != *id {
                        errors.push(format!(
                            "embedded profile `{}` declares id `{}`",
                            id, profile.id
                        ));
                    } else {
                        entries.insert(profile.id.clone(), profile);
                    }
                }
                Err(error) => errors.push(format!("embedded profile `{}`: {}", id, error.message)),
            }
        }

        // Lower-precedence extra directories first; the caller lists
        // highest-precedence directories first.
        for dir in extra_dirs.iter().rev() {
            load_dir(dir, &mut entries, &mut errors);
        }
        if let Some(user) = user_profile_dir() {
            load_dir(&user, &mut entries, &mut errors);
        }

        Self { entries, errors }
    }

    pub fn available(&self) -> Vec<&str> {
        self.entries.keys().map(String::as_str).collect()
    }
}

pub fn resolve(id_or_path: &str, extra_dirs: &[PathBuf]) -> Result<(RawProfile, Vec<String>)> {
    let path = Path::new(id_or_path);
    if id_or_path.contains('/') || id_or_path.ends_with(".toml") {
        let text = std::fs::read_to_string(path)
            .map_err(|e| Error::new(format!("{}: {}", path.display(), e)))?;
        let profile = parse_profile_str(Source::File(path.to_path_buf()), &text)?;
        return Ok((profile, Vec::new()));
    }

    let set = ProfileSet::discover(extra_dirs);
    if let Some(profile) = set.entries.get(id_or_path) {
        return Ok((profile.clone(), set.errors));
    }
    let available = set.available().join(", ");
    let mut message = format!("unknown profile `{}`; available: {}", id_or_path, available);
    if !set.errors.is_empty() {
        message.push_str("\nprofile load errors:\n");
        message.push_str(&set.errors.join("\n"));
    }
    Err(Error::new(message))
}

pub fn user_profile_dir() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| home.map(|home| home.join(".config")))?;
    Some(config.join("linelight/profiles"))
}

fn load_dir(dir: &Path, entries: &mut BTreeMap<String, RawProfile>, errors: &mut Vec<String>) {
    let Ok(read) = std::fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<PathBuf> = read
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("toml"))
        .collect();
    paths.sort();
    for path in paths {
        let stem = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or_default()
            .to_string();
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) => {
                errors.push(format!("{}: {}", path.display(), error));
                if !stem.is_empty() {
                    entries.remove(&stem);
                }
                continue;
            }
        };
        match parse_profile_str(Source::File(path.clone()), &text) {
            Ok(profile) if profile.id == stem => {
                entries.insert(profile.id.clone(), profile);
            }
            Ok(profile) => {
                errors.push(format!(
                    "{}: file stem `{}` does not match profile id `{}`",
                    path.display(),
                    stem,
                    profile.id
                ));
                if !stem.is_empty() {
                    entries.remove(&stem);
                }
            }
            Err(error) => {
                errors.push(format!("{}: {}", path.display(), error.message));
                if !stem.is_empty() {
                    entries.remove(&stem);
                }
            }
        }
    }
}

pub fn parse_profile_file(path: &Path) -> Result<RawProfile> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| Error::new(format!("{}: {}", path.display(), e)))?;
    parse_profile_str(Source::File(path.to_path_buf()), &text)
}

pub fn parse_profile_str(source: Source, text: &str) -> Result<RawProfile> {
    let file: ProfileFile =
        toml::from_str(text).map_err(|e| Error::new(format!("{}: {}", source, e)))?;
    if file.schema != 1 {
        return Err(Error::new(format!(
            "{}: unsupported schema {}; expected 1",
            source, file.schema
        )));
    }
    if !valid_id(&file.id) {
        return Err(Error::new(format!(
            "{}: invalid profile id `{}` (expected lowercase letters, digits, hyphens)",
            source, file.id
        )));
    }
    if file.rules.is_empty() {
        return Err(Error::new(format!("{}: profile has no rules", source)));
    }
    for (index, rule) in file.rules.iter().enumerate() {
        if rule.regex.is_empty() {
            return Err(Error::new(format!(
                "{}: rule {} has an empty regexp",
                source,
                index + 1
            )));
        }
        if rule.style.is_none() && rule.groups.is_empty() {
            return Err(Error::new(format!(
                "{}: rule {} has neither style nor groups",
                source,
                index + 1
            )));
        }
        if let Some(style) = &rule.style {
            if !SEMANTIC.contains(&style.as_str()) {
                return Err(Error::new(format!(
                    "{}: rule {} references unknown semantic style `{}`",
                    source,
                    index + 1,
                    style
                )));
            }
        }
        for (group, style) in &rule.groups {
            if group.is_empty() {
                return Err(Error::new(format!(
                    "{}: rule {} has an empty group name",
                    source,
                    index + 1
                )));
            }
            if !SEMANTIC.contains(&style.as_str()) {
                return Err(Error::new(format!(
                    "{}: rule {} group `{}` references unknown semantic style `{}`",
                    source,
                    index + 1,
                    group,
                    style
                )));
            }
        }
    }
    Ok(RawProfile {
        id: file.id,
        name: file.name.unwrap_or_else(|| "unnamed".to_string()),
        description: file.description.unwrap_or_default(),
        source,
        rules: file.rules,
    })
}

impl RawProfile {
    pub fn compile(&self, palette: &Palette) -> Result<CompiledProfile> {
        let mut rules = Vec::with_capacity(self.rules.len());
        for (index, rule) in self.rules.iter().enumerate() {
            let regex = RegexBuilder::new(&rule.regex)
                .size_limit(10 * 1024 * 1024)
                .build()
                .map_err(|e| {
                    Error::new(format!(
                        "{}: rule {}: invalid regexp: {}",
                        self.source,
                        index + 1,
                        e
                    ))
                })?;
            let style = rule
                .style
                .as_ref()
                .map(|name| {
                    palette.style(name).ok_or_else(|| {
                        Error::new(format!(
                            "{}: rule {}: unknown palette style `{}`",
                            self.source,
                            index + 1,
                            name
                        ))
                    })
                })
                .transpose()?;
            let mut groups = Vec::new();
            for (name, style) in &rule.groups {
                if regex.capture_names().flatten().all(|group| group != name) {
                    return Err(Error::new(format!(
                        "{}: rule {}: group `{}` does not exist",
                        self.source,
                        index + 1,
                        name
                    )));
                }
                let id = palette.style(style).ok_or_else(|| {
                    Error::new(format!(
                        "{}: rule {}: unknown palette style `{}`",
                        self.source,
                        index + 1,
                        style
                    ))
                })?;
                groups.push((name.clone(), id));
            }
            groups.sort_by_key(|(name, _)| {
                regex
                    .capture_names()
                    .position(|group| group == Some(name.as_str()))
                    .unwrap_or(usize::MAX)
            });
            rules.push(CompiledRule {
                regex,
                style,
                groups,
            });
        }
        Ok(CompiledProfile {
            id: self.id.clone(),
            name: self.name.clone(),
            description: self.description.clone(),
            source: self.source.clone(),
            rules,
        })
    }
}

fn valid_id(id: &str) -> bool {
    let mut chars = id.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_lowercase() || first.is_ascii_digit())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

#[cfg(test)]
mod tests {
    use super::{parse_profile_str, Source};
    use crate::style::Palette;

    const SIMPLE: &str = r#"
schema = 1
id = "demo"
name = "Demo"
[[rule]]
regex = '(?P<word>hello)'
groups = { word = "identifier" }
"#;

    #[test]
    fn parses_and_compiles() {
        let raw = parse_profile_str(Source::Embedded, SIMPLE).unwrap();
        raw.compile(&Palette::default()).unwrap();
    }

    #[test]
    fn rejects_unknown_field() {
        let text = SIMPLE.replace("name = \"Demo\"", "name = \"Demo\"\nextra = 1");
        assert!(parse_profile_str(Source::Embedded, &text).is_err());
    }

    #[test]
    fn rejects_duplicate_keys() {
        let text = SIMPLE.replace("schema = 1", "schema = 1\nschema = 1");
        assert!(parse_profile_str(Source::Embedded, &text).is_err());
    }

    #[test]
    fn rejects_invalid_regex() {
        let text = SIMPLE.replace("regex = '(?P<word>hello)'", "regex = '('");
        let raw = parse_profile_str(Source::Embedded, &text).unwrap();
        assert!(raw.compile(&Palette::default()).is_err());
    }

    #[test]
    fn rejects_unknown_style() {
        let text = SIMPLE.replace("identifier", "chartreuse");
        assert!(parse_profile_str(Source::Embedded, &text).is_err());
    }

    #[test]
    fn rejects_missing_group() {
        let text = SIMPLE.replace(
            "groups = { word = \"identifier\" }",
            "groups = { missing = \"identifier\" }",
        );
        let raw = parse_profile_str(Source::Embedded, &text).unwrap();
        assert!(raw.compile(&Palette::default()).is_err());
    }
}
