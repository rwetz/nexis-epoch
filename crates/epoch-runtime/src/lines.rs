//! Turning a child's output bytes into display lines, and a user's argument
//! string into an argument list.

use std::io::Read;

/// Longest line kept; the rest is cut and marked with `…`.
pub(crate) const MAX_LINE_BYTES: usize = 16 * 1024;

/// Read `reader` to the end, calling `emit` with each line.
///
/// - Lines end at `\n` (a preceding `\r` is dropped).
/// - A lone `\r` returns to the start of the line, as in a terminal: a
///   progress bar redrawn a thousand times becomes the one line it ends
///   as, not a thousand lines.
/// - ANSI escape sequences (colours, cursor moves) are removed.
/// - Invalid UTF-8 is replaced, never fatal.
pub(crate) fn read_lines<R: Read>(mut reader: R, mut emit: impl FnMut(String)) {
    let mut buf = [0u8; 8192];
    let mut line: Vec<u8> = Vec::new();
    let mut cut = false;
    let mut carriage = false;
    loop {
        let n = match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        };
        for &b in &buf[..n] {
            match b {
                b'\n' => {
                    emit(finish(&mut line, &mut cut));
                    carriage = false;
                }
                b'\r' => carriage = true,
                _ => {
                    if carriage {
                        line.clear();
                        cut = false;
                        carriage = false;
                    }
                    if line.len() < MAX_LINE_BYTES {
                        line.push(b);
                    } else {
                        cut = true;
                    }
                }
            }
        }
    }
    if !line.is_empty() {
        emit(finish(&mut line, &mut cut));
    }
}

fn finish(line: &mut Vec<u8>, cut: &mut bool) -> String {
    let mut text = strip_ansi(&String::from_utf8_lossy(line));
    if *cut {
        text.push('…');
    }
    line.clear();
    *cut = false;
    text
}

/// Remove CSI (`ESC [ … final`) and OSC (`ESC ] … BEL|ESC \`) sequences and
/// other two-byte escapes.
fn strip_ansi(s: &str) -> String {
    if !s.contains('\u{1b}') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('[') => {
                // Parameters and intermediates, then one final byte @..~.
                for c in chars.by_ref() {
                    if ('@'..='~').contains(&c) {
                        break;
                    }
                }
            }
            Some(']') => {
                while let Some(c) = chars.next() {
                    if c == '\u{7}' {
                        break;
                    }
                    if c == '\u{1b}' && chars.peek() == Some(&'\\') {
                        chars.next();
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// Split a user's argument string into a list: whitespace separates,
/// single or double quotes group (and are removed), `\"` inside double
/// quotes is a literal quote. Backslashes are otherwise literal, so Windows
/// paths need no escaping. Nothing is expanded: no variables, globs or
/// pipes, because nothing runs through a shell.
pub fn split_args(input: &str) -> Result<Vec<String>, String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut in_arg = false;
    let mut quote: Option<char> = None;
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        match quote {
            Some(q) => {
                if c == q {
                    quote = None;
                } else if q == '"' && c == '\\' && chars.peek() == Some(&'"') {
                    current.push('"');
                    chars.next();
                } else {
                    current.push(c);
                }
            }
            None if c == '"' || c == '\'' => {
                quote = Some(c);
                in_arg = true;
            }
            None if c.is_whitespace() => {
                if in_arg {
                    args.push(std::mem::take(&mut current));
                    in_arg = false;
                }
            }
            None => {
                current.push(c);
                in_arg = true;
            }
        }
    }
    if let Some(q) = quote {
        return Err(format!("unclosed {q} quote"));
    }
    if in_arg {
        args.push(current);
    }
    Ok(args)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(input: &[u8]) -> Vec<String> {
        let mut out = Vec::new();
        read_lines(input, |l| out.push(l));
        out
    }

    #[test]
    fn splits_on_newlines_and_keeps_a_last_partial_line() {
        assert_eq!(lines(b"a\nb\r\n\nc"), ["a", "b", "", "c"]);
        assert!(lines(b"").is_empty());
    }

    #[test]
    fn carriage_returns_redraw_the_line() {
        assert_eq!(lines(b"10%\r50%\r100%\ndone\n"), ["100%", "done"]);
    }

    #[test]
    fn strips_colours_and_replaces_bad_utf8() {
        assert_eq!(lines(b"\x1b[1;31merror\x1b[0m: x\n"), ["error: x"]);
        assert_eq!(lines(b"\x1b]0;title\x07ok\n"), ["ok"]);
        assert_eq!(lines(b"caf\xe9\n"), ["caf\u{fffd}"]);
    }

    #[test]
    fn cuts_very_long_lines() {
        let long = vec![b'x'; MAX_LINE_BYTES + 10];
        let out = lines(&long);
        assert_eq!(out.len(), 1);
        assert!(out[0].ends_with('…'));
        assert_eq!(out[0].chars().count(), MAX_LINE_BYTES + 1);
    }

    #[test]
    fn splits_arguments_like_a_careful_shell() {
        assert_eq!(
            split_args(r#"--lr 0.1  --name "my run" 'a b' C:\data\x.csv"#).unwrap(),
            ["--lr", "0.1", "--name", "my run", "a b", r"C:\data\x.csv"]
        );
        assert_eq!(split_args(r#""say \"hi\"""#).unwrap(), [r#"say "hi""#]);
        assert_eq!(split_args(r#"--empty """#).unwrap(), ["--empty", ""]);
        assert!(split_args("   ").unwrap().is_empty());
        assert!(split_args("'open").is_err());
        assert_eq!(
            split_args("$HOME *.py | rm").unwrap(),
            ["$HOME", "*.py", "|", "rm"]
        );
    }
}
