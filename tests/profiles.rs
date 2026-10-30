use std::path::PathBuf;

use linelight::engine::colorize;
use linelight::profile::ProfileSet;
use linelight::style::Palette;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn strip_sgr(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == 0x1b && bytes.get(i + 1) == Some(&b'[') {
            i += 2;
            while i < bytes.len() && bytes[i] != b'm' {
                i += 1;
            }
            i += 1;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    out
}

fn profiles() -> ProfileSet {
    ProfileSet::discover(&[])
}

#[test]
fn every_embedded_profile_compiles() {
    let set = profiles();
    assert!(set.errors.is_empty(), "{}", set.errors.join("\n"));
    let palette = Palette::default();
    for id in [
        "df", "free", "ps", "ss", "ping", "dig", "sensors", "ls", "diff", "lsblk", "log",
    ] {
        let raw = set
            .entries
            .get(id)
            .unwrap_or_else(|| panic!("missing {}", id));
        raw.compile(&palette).unwrap();
    }
}

#[test]
fn fixtures_preserve_text_and_add_style() {
    let set = profiles();
    let palette = Palette::default();
    for id in [
        "df", "free", "ps", "ss", "ping", "dig", "sensors", "ls", "diff", "lsblk", "log",
    ] {
        let raw = set.entries.get(id).unwrap();
        let profile = raw.compile(&palette).unwrap();
        let input = std::fs::read(root().join(format!("tests/fixtures/{}.in", id))).unwrap();
        let text = std::str::from_utf8(&input).unwrap();
        let mut saw_style = false;
        for line in text.lines() {
            let mut out = Vec::new();
            colorize(&profile, &palette, line, &mut out);
            assert_eq!(
                strip_sgr(&out),
                line.as_bytes(),
                "profile {} changed text",
                id
            );
            if out.windows(2).any(|window| window == b"\x1b[") {
                saw_style = true;
            }
        }
        assert!(saw_style, "profile {} produced no styles", id);
    }
}

#[test]
fn specific_semantics_are_applied() {
    let set = profiles();
    let palette = Palette::default();
    let cases = [
        ("diff", "+new", b"\x1b[0;32m".as_slice()),
        ("log", "level=ERROR", b"\x1b[0;1;31m".as_slice()),
        ("df", "99%", b"\x1b[0;1;31m".as_slice()),
        ("dig", "status: NOERROR", b"\x1b[0;32m".as_slice()),
        ("ping", "Request timeout", b"\x1b[0;1;31m".as_slice()),
    ];
    for (id, line, expected) in cases {
        let profile = set.entries.get(id).unwrap().compile(&palette).unwrap();
        let mut out = Vec::new();
        colorize(&profile, &palette, line, &mut out);
        assert!(
            out.windows(expected.len()).any(|window| window == expected),
            "profile {} did not apply expected style to {:?}: {:?}",
            id,
            line,
            String::from_utf8_lossy(&out)
        );
    }
}

#[test]
fn embedded_set_is_only_the_admitted_candidates() {
    let set = profiles();
    let mut ids: Vec<&str> = set.entries.keys().map(String::as_str).collect();
    ids.sort();
    assert_eq!(
        ids,
        vec!["df", "diff", "dig", "free", "log", "ls", "lsblk", "ping", "ps", "sensors", "ss",]
    );
    for native in [
        "journalctl",
        "systemctl",
        "dnf",
        "mvn",
        "ip",
        "gcc",
        "fdisk",
        "iostat",
        "sar",
    ] {
        assert!(
            !set.entries.contains_key(native),
            "native-default profile {}",
            native
        );
    }
}
