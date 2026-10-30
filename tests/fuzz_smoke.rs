use std::io::Cursor;

use linelight::framer::filter;
use linelight::profile::{parse_profile_str, Source};
use linelight::style::Palette;

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

#[test]
fn random_records_preserve_text() {
    let profile = parse_profile_str(
        Source::Embedded,
        "schema = 1\nid = \"demo\"\n[[rule]]\nregex = 'x+'\nstyle = \"error\"\n",
    )
    .unwrap()
    .compile(&Palette::default())
    .unwrap();
    let palette = Palette::default();

    let mut state = 0x1234_5678u64;
    for _ in 0..2000 {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        let len = (state as usize >> 33) % 200;
        let mut input = Vec::with_capacity(len);
        for _ in 0..len {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            let byte = (state as u8) & 0x7f; // no ESC, so no original SGR
            input.push(byte);
        }
        let mut reader = Cursor::new(&input);
        let mut output = Vec::new();
        filter(&mut reader, &mut output, &profile, &palette, true, 64).unwrap();
        assert_eq!(
            strip_sgr(&output),
            input,
            "input changed: {:?} -> {:?}",
            input,
            output
        );

        let mut reader = Cursor::new(&input);
        let mut output = Vec::new();
        filter(&mut reader, &mut output, &profile, &palette, false, 64).unwrap();
        assert_eq!(output, input);
    }
}
