use crate::profile::CompiledProfile;
use crate::style::Palette;

pub type StyleId = usize;

/// Colour one valid UTF-8 record. The returned bytes contain the original
/// text plus only the SGR sequences inserted at style-run boundaries.
pub fn colorize(profile: &CompiledProfile, palette: &Palette, line: &str, out: &mut Vec<u8>) {
    out.clear();
    if line.is_empty() {
        return;
    }

    let mut styles = vec![0usize; line.len()];
    for rule in &profile.rules {
        for captures in rule.regex.captures_iter(line) {
            if let Some(style) = rule.style {
                if let Some(matched) = captures.get(0) {
                    fill(&mut styles, matched.start(), matched.end(), style);
                }
            }
            for (name, style) in &rule.groups {
                if let Some(matched) = captures.name(name) {
                    fill(&mut styles, matched.start(), matched.end(), *style);
                }
            }
        }
    }

    let mut current: Option<StyleId> = None;
    let mut index = 0;
    while index < line.len() {
        let style = styles[index];
        let mut end = index + 1;
        while end < line.len() && styles[end] == style {
            end += 1;
        }
        if current != Some(style) {
            let next = palette.get(style);
            let previous = current.map(|id| palette.get(id));
            if next.sgr.is_empty() {
                if previous.is_some_and(|previous| !previous.sgr.is_empty()) {
                    out.extend_from_slice(b"\x1b[0m");
                }
            } else {
                out.extend_from_slice(&next.sgr);
            }
            current = Some(style);
        }
        out.extend_from_slice(&line.as_bytes()[index..end]);
        index = end;
    }
    if let Some(style) = current {
        if !palette.get(style).sgr.is_empty() {
            out.extend_from_slice(b"\x1b[0m");
        }
    }
}

fn fill(styles: &mut [StyleId], start: usize, end: usize, style: StyleId) {
    let end = end.min(styles.len());
    let start = start.min(end);
    styles[start..end].fill(style);
}

#[cfg(test)]
mod tests {
    use super::colorize;
    use crate::profile::{parse_profile_str, Source};
    use crate::style::Palette;

    fn compiled(text: &str) -> crate::profile::CompiledProfile {
        parse_profile_str(Source::Embedded, text)
            .unwrap()
            .compile(&Palette::default())
            .unwrap()
    }

    #[test]
    fn styles_whole_match() {
        let profile = compiled(
            "schema = 1\nid = \"demo\"\n[[rule]]\nregex = 'hello'\nstyle = \"identifier\"\n",
        );
        let palette = Palette::default();
        let mut out = Vec::new();
        colorize(&profile, &palette, "hello world", &mut out);
        assert_eq!(out, b"\x1b[0;36mhello\x1b[0m world");
    }

    #[test]
    fn named_groups_override_whole_match() {
        let profile = compiled(
            "schema = 1\nid = \"demo\"\n[[rule]]\nregex = '(?P<a>a)(?P<b>b)'\nstyle = \"heading\"\ngroups = { a = \"success\", b = \"error\" }\n",
        );
        let palette = Palette::default();
        let mut out = Vec::new();
        colorize(&profile, &palette, "ab", &mut out);
        assert_eq!(out, b"\x1b[0;32ma\x1b[0;1;31mb\x1b[0m");
    }

    #[test]
    fn later_rules_override_earlier_rules() {
        let profile = compiled(
            "schema = 1\nid = \"demo\"\n[[rule]]\nregex = 'foo'\nstyle = \"error\"\n[[rule]]\nregex = 'o'\nstyle = \"success\"\n",
        );
        let palette = Palette::default();
        let mut out = Vec::new();
        colorize(&profile, &palette, "foo", &mut out);
        assert_eq!(out, b"\x1b[0;1;31mf\x1b[0;32moo\x1b[0m");
    }

    #[test]
    fn empty_line_is_unchanged() {
        let profile =
            compiled("schema = 1\nid = \"demo\"\n[[rule]]\nregex = 'x'\nstyle = \"error\"\n");
        let mut out = Vec::new();
        colorize(&profile, &Palette::default(), "", &mut out);
        assert!(out.is_empty());
    }
}
