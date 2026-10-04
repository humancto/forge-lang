//! Base-protocol framing for the Debug Adapter Protocol.
//!
//! DAP uses the same wire format as LSP: a block of `Name: value` headers
//! terminated by an empty line (`\r\n`), followed by exactly
//! `Content-Length` bytes of JSON. All reads go through a single
//! `BufRead`, so buffered bytes are never lost between header and body
//! (and stdin is never re-locked mid-message).

use std::io::{self, BufRead, Write};

/// Upper bound for a single message body; protects against a corrupted
/// header allocating gigabytes.
const MAX_CONTENT_LENGTH: usize = 64 * 1024 * 1024;

/// Read one framed message body.
///
/// Returns `Ok(None)` on a clean EOF (before any header byte of a new
/// message). Header names are matched case-insensitively and unknown headers
/// (e.g. `Content-Type`) are ignored. Accepts both `\r\n` and bare `\n` line
/// endings for robustness with hand-written clients.
pub(crate) fn read_message(reader: &mut impl BufRead) -> io::Result<Option<Vec<u8>>> {
    let mut content_length: Option<usize> = None;
    let mut saw_header = false;
    let mut line = String::new();
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            if saw_header {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "EOF inside message headers",
                ));
            }
            return Ok(None);
        }
        let header = line.trim_end_matches(['\r', '\n']);
        if header.is_empty() {
            if saw_header {
                break;
            }
            // Stray blank line between messages: tolerate it.
            continue;
        }
        saw_header = true;
        let Some((name, value)) = header.split_once(':') else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("malformed header: {:?}", header),
            ));
        };
        if name.trim().eq_ignore_ascii_case("Content-Length") {
            let len = value.trim().parse::<usize>().map_err(|e| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("bad Content-Length {:?}: {}", value.trim(), e),
                )
            })?;
            content_length = Some(len);
        }
    }
    let len = content_length
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing Content-Length"))?;
    if len > MAX_CONTENT_LENGTH {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Content-Length {} exceeds limit", len),
        ));
    }
    let mut body = vec![0u8; len];
    reader.read_exact(&mut body)?;
    Ok(Some(body))
}

/// Write one framed message and flush.
pub(crate) fn write_message(writer: &mut impl Write, body: &str) -> io::Result<()> {
    write!(writer, "Content-Length: {}\r\n\r\n", body.len())?;
    writer.write_all(body.as_bytes())?;
    writer.flush()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn read_all(input: &[u8]) -> Vec<io::Result<Option<Vec<u8>>>> {
        let mut cursor = Cursor::new(input.to_vec());
        let mut out = Vec::new();
        loop {
            let msg = read_message(&mut cursor);
            let stop = !matches!(msg, Ok(Some(_)));
            out.push(msg);
            if stop {
                return out;
            }
        }
    }

    #[test]
    fn reads_back_to_back_messages() {
        let mut buf = Vec::new();
        write_message(&mut buf, r#"{"seq":1}"#).unwrap();
        write_message(&mut buf, r#"{"seq":2}"#).unwrap();
        let msgs = read_all(&buf);
        assert_eq!(msgs.len(), 3);
        assert_eq!(
            msgs[0].as_ref().unwrap().as_deref(),
            Some(&br#"{"seq":1}"#[..])
        );
        assert_eq!(
            msgs[1].as_ref().unwrap().as_deref(),
            Some(&br#"{"seq":2}"#[..])
        );
        assert!(matches!(msgs[2], Ok(None)));
    }

    #[test]
    fn ignores_content_type_and_header_case() {
        let input =
            b"content-length: 2\r\nContent-Type: application/vscode-jsonrpc; charset=utf-8\r\n\r\n{}";
        let mut cursor = Cursor::new(&input[..]);
        assert_eq!(read_message(&mut cursor).unwrap().unwrap(), b"{}");
    }

    #[test]
    fn body_is_read_by_byte_length_not_lines() {
        // Body contains a newline and multi-byte UTF-8.
        let body = "{\"s\":\"é\n\"}";
        let mut buf = Vec::new();
        write_message(&mut buf, body).unwrap();
        let mut cursor = Cursor::new(buf);
        assert_eq!(read_message(&mut cursor).unwrap().unwrap(), body.as_bytes());
    }

    #[test]
    fn clean_eof_is_none() {
        let mut cursor = Cursor::new(&b""[..]);
        assert!(read_message(&mut cursor).unwrap().is_none());
    }

    #[test]
    fn truncated_body_is_error() {
        let mut cursor = Cursor::new(&b"Content-Length: 10\r\n\r\n{}"[..]);
        assert!(read_message(&mut cursor).is_err());
    }

    #[test]
    fn missing_content_length_is_error() {
        let mut cursor = Cursor::new(&b"Content-Type: x\r\n\r\n{}"[..]);
        assert!(read_message(&mut cursor).is_err());
    }
}
