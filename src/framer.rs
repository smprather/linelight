use std::io::{self, BufRead, Write};

use crate::engine::colorize;
use crate::error::{Error, Result};
use crate::profile::CompiledProfile;
use crate::style::Palette;

/// Filter a byte stream. When `colour` is false, copy exactly. When true,
/// records containing control bytes, invalid UTF-8, or ANSI are passed
/// through unchanged; only ordinary UTF-8 records are styled.
pub fn filter<R: BufRead, W: Write>(
    reader: &mut R,
    writer: &mut W,
    profile: &CompiledProfile,
    palette: &Palette,
    colour: bool,
    max_record: usize,
    partial_passthrough: bool,
) -> Result<()> {
    if !colour {
        return match io::copy(reader, writer) {
            Ok(_) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::BrokenPipe => Ok(()),
            Err(error) => Err(Error::from(error)),
        };
    }

    let max_record = max_record.max(1);
    let mut record = Vec::new();
    let mut bypass = false;
    let mut bypass_buf = Vec::new();
    let mut pending_cr = false;

    loop {
        let chunk = {
            let available = reader.fill_buf()?;
            if available.is_empty() {
                break;
            }
            let chunk = available.to_vec();
            reader.consume(chunk.len());
            chunk
        };

        let mut index = 0;
        while index < chunk.len() {
            let byte = chunk[index];

            if bypass {
                bypass_buf.push(byte);
                if byte == b'\n' {
                    if !put(writer, &bypass_buf)? {
                        return Ok(());
                    }
                    bypass_buf.clear();
                    bypass = false;
                    if !flush(writer)? {
                        return Ok(());
                    }
                } else if bypass_buf.len() >= 8192 {
                    if !put(writer, &bypass_buf)? {
                        return Ok(());
                    }
                    bypass_buf.clear();
                }
                index += 1;
                continue;
            }

            if pending_cr {
                pending_cr = false;
                if byte == b'\n' {
                    if !emit_record(writer, &mut record, b"\r\n", profile, palette)? {
                        return Ok(());
                    }
                    index += 1;
                    continue;
                }
                record.push(b'\r');
                if !put(writer, &record)? {
                    return Ok(());
                }
                record.clear();
                bypass = true;
                if !flush(writer)? {
                    return Ok(());
                }
                // Handle this byte in bypass mode below.
                continue;
            }

            match byte {
                b'\n' => {
                    if !emit_record(writer, &mut record, b"\n", profile, palette)? {
                        return Ok(());
                    }
                }
                b'\r' => {
                    if index + 1 < chunk.len() {
                        if chunk[index + 1] == b'\n' {
                            if !emit_record(writer, &mut record, b"\r\n", profile, palette)? {
                                return Ok(());
                            }
                            index += 2;
                            continue;
                        }
                        record.push(b'\r');
                        if !put(writer, &record)? {
                            return Ok(());
                        }
                        record.clear();
                        bypass = true;
                        if !flush(writer)? {
                            return Ok(());
                        }
                    } else {
                        pending_cr = true;
                    }
                }
                b if is_control(b) => {
                    record.push(b);
                    if !put(writer, &record)? {
                        return Ok(());
                    }
                    record.clear();
                    bypass = true;
                    if !flush(writer)? {
                        return Ok(());
                    }
                }
                b => {
                    record.push(b);
                    if record.len() >= max_record {
                        if !put(writer, &record)? {
                            return Ok(());
                        }
                        record.clear();
                        bypass = true;
                    }
                }
            }
            index += 1;
        }

        if partial_passthrough {
            if !bypass {
                if pending_cr {
                    record.push(b'\r');
                    if !put(writer, &record)? {
                        return Ok(());
                    }
                    record.clear();
                    pending_cr = false;
                    bypass = true;
                    if !flush(writer)? {
                        return Ok(());
                    }
                } else if !record.is_empty() {
                    if !put(writer, &record)? {
                        return Ok(());
                    }
                    record.clear();
                    bypass = true;
                    if !flush(writer)? {
                        return Ok(());
                    }
                }
            }
            if bypass && !bypass_buf.is_empty() {
                if !put(writer, &bypass_buf)? {
                    return Ok(());
                }
                bypass_buf.clear();
                if !flush(writer)? {
                    return Ok(());
                }
            }
        }
    }

    if pending_cr {
        record.push(b'\r');
        if !put(writer, &record)? {
            return Ok(());
        }
        record.clear();
    } else if !record.is_empty() && !emit_record(writer, &mut record, b"", profile, palette)? {
        return Ok(());
    }
    if !bypass_buf.is_empty() && !put(writer, &bypass_buf)? {
        return Ok(());
    }
    let _ = flush(writer)?;
    Ok(())
}

/// Returns false on EPIPE, true on success.
fn put<W: Write>(writer: &mut W, bytes: &[u8]) -> Result<bool> {
    match writer.write_all(bytes) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => Ok(false),
        Err(error) => Err(Error::from(error)),
    }
}

fn flush<W: Write>(writer: &mut W) -> Result<bool> {
    match writer.flush() {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => Ok(false),
        Err(error) => Err(Error::from(error)),
    }
}

fn emit_record<W: Write>(
    writer: &mut W,
    record: &mut Vec<u8>,
    terminator: &[u8],
    profile: &CompiledProfile,
    palette: &Palette,
) -> Result<bool> {
    if !record.is_empty() {
        let mut output = Vec::with_capacity(record.len() + 16);
        if let Ok(text) = std::str::from_utf8(record) {
            if !text.chars().any(|ch| ch.is_control() && ch != '\t') {
                colorize(profile, palette, text, &mut output);
            } else {
                output.extend_from_slice(record);
            }
        } else {
            output.extend_from_slice(record);
        }
        output.extend_from_slice(terminator);
        let ok = put(writer, &output)?;
        record.clear();
        return Ok(ok);
    }
    let ok = put(writer, terminator)?;
    record.clear();
    Ok(ok)
}

fn is_control(byte: u8) -> bool {
    (byte < 0x20 && byte != b'\t' && byte != b'\n' && byte != b'\r') || byte == 0x7f
}

#[cfg(test)]
mod tests {
    use std::io::{BufRead, Cursor};

    use super::filter;
    use crate::profile::{parse_profile_str, Source};
    use crate::style::Palette;

    fn compiled() -> crate::profile::CompiledProfile {
        parse_profile_str(
            Source::Embedded,
            "schema = 1\nid = \"demo\"\n[[rule]]\nregex = 'x'\nstyle = \"error\"\n",
        )
        .unwrap()
        .compile(&Palette::default())
        .unwrap()
    }

    fn run(input: &[u8], colour: bool, max: usize) -> Vec<u8> {
        let profile = compiled();
        let palette = Palette::default();
        let mut reader = Cursor::new(input);
        let mut out = Vec::new();
        filter(
            &mut reader,
            &mut out,
            &profile,
            &palette,
            colour,
            max,
            false,
        )
        .unwrap();
        out
    }

    #[test]
    fn styles_plain_record() {
        assert_eq!(run(b"x\n", true, 64), b"\x1b[0;1;31mx\x1b[0m\n");
    }

    #[test]
    fn preserves_crlf_and_missing_final_newline() {
        assert_eq!(run(b"x\r\n", true, 64), b"\x1b[0;1;31mx\x1b[0m\r\n");
        assert_eq!(run(b"x", true, 64), b"\x1b[0;1;31mx\x1b[0m");
    }

    #[test]
    fn bypasses_ansi_nul_and_invalid_utf8() {
        for input in [
            &b"\x1b[31mred\x1b[0m\n"[..],
            &b"a\0b\n"[..],
            &b"abc\xffdef\n"[..],
        ] {
            assert_eq!(run(input, true, 64), input);
        }
    }

    #[test]
    fn bypasses_carriage_return_progress() {
        assert_eq!(run(b"10%\r20%\n", true, 64), b"10%\r20%\n");
    }

    #[test]
    fn oversized_record_streams_unchanged() {
        let mut input = vec![b'x'; 100];
        input.push(b'\n');
        assert_eq!(run(&input, true, 16), input);
    }

    #[test]
    fn colour_off_is_byte_exact() {
        let input = b"x\r\n\x1b[31mred\x1b[0m\n\xff";
        assert_eq!(run(input, false, 64), input);
    }

    #[derive(Default)]
    struct Recorder {
        bytes: Vec<u8>,
        flushes: Vec<usize>,
    }

    impl std::io::Write for Recorder {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            self.flushes.push(self.bytes.len());
            Ok(())
        }
    }

    struct Chunked {
        chunks: Vec<Vec<u8>>,
        index: usize,
        pos: usize,
    }

    impl Chunked {
        fn new(chunks: Vec<Vec<u8>>) -> Self {
            Self {
                chunks,
                index: 0,
                pos: 0,
            }
        }
    }

    impl std::io::Read for Chunked {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            let n = {
                let available = self.fill_buf()?;
                let n = available.len().min(buf.len());
                buf[..n].copy_from_slice(&available[..n]);
                n
            };
            self.consume(n);
            Ok(n)
        }
    }

    impl std::io::BufRead for Chunked {
        fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
            if self.index >= self.chunks.len() {
                return Ok(&[]);
            }
            Ok(&self.chunks[self.index][self.pos..])
        }

        fn consume(&mut self, amount: usize) {
            self.pos += amount;
            if self.pos >= self.chunks[self.index].len() {
                self.index += 1;
                self.pos = 0;
            }
        }
    }

    #[test]
    fn flushes_progress_marker_before_lf() {
        let profile = compiled();
        let palette = Palette::default();
        let input = b"progress\rnext";
        let mut reader = Cursor::new(input);
        let mut writer = Recorder::default();
        filter(
            &mut reader,
            &mut writer,
            &profile,
            &palette,
            true,
            64,
            false,
        )
        .unwrap();
        assert_eq!(writer.bytes, input);
        assert_eq!(writer.flushes.first().copied(), Some(b"progress\r".len()));
    }

    #[test]
    fn partial_passthrough_flushes_before_next_chunk() {
        let profile = compiled();
        let palette = Palette::default();
        let mut reader = Chunked::new(vec![b"hel".to_vec(), b"lo\n".to_vec()]);
        let mut writer = Recorder::default();
        filter(&mut reader, &mut writer, &profile, &palette, true, 64, true).unwrap();
        assert_eq!(writer.bytes, b"hello\n");
        assert_eq!(writer.flushes.first().copied(), Some(3));
    }

    #[test]
    fn flushes_progress_marker_across_chunks() {
        let profile = compiled();
        let palette = Palette::default();
        let mut reader = Chunked::new(vec![b"progress\r".to_vec(), b"next".to_vec()]);
        let mut writer = Recorder::default();
        filter(
            &mut reader,
            &mut writer,
            &profile,
            &palette,
            true,
            64,
            false,
        )
        .unwrap();
        assert_eq!(writer.bytes, b"progress\rnext");
        assert_eq!(writer.flushes.first().copied(), Some(b"progress\r".len()));
    }
}
