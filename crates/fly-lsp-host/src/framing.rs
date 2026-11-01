//! LSP over stdio 的 Content-Length 帧编解码。
//!
//! 帧格式（LSP 规范）：若干 `Key: Value\r\n` 头 + 空行 + 恰好
//! `Content-Length` 字节的消息体。`Content-Length` 以**字节**计。

use std::io::{self, BufRead, Write};

/// 写入一条带帧的消息。
pub fn write_message(mut out: impl Write, message: &str) -> io::Result<()> {
    write!(out, "Content-Length: {}\r\n\r\n", message.len())?;
    out.write_all(message.as_bytes())?;
    out.flush()
}

/// 读取一条带帧的消息；EOF 返回 `None`；帧不完整/非法返回错误。
pub fn read_message(mut input: impl BufRead) -> io::Result<Option<String>> {
    let mut content_length: Option<usize> = None;
    loop {
        let mut line = String::new();
        let read = input.read_line(&mut line)?;
        if read == 0 {
            return Ok(None); // EOF
        }
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            break; // 头结束
        }
        if let Some(value) = trimmed
            .strip_prefix("Content-Length:")
            .or_else(|| trimmed.strip_prefix("content-length:"))
        {
            content_length =
                Some(value.trim().parse().map_err(|_| {
                    io::Error::new(io::ErrorKind::InvalidData, "非法 Content-Length")
                })?);
        }
        // 其他头（Content-Type 等）忽略
    }
    let length = content_length
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "缺少 Content-Length"))?;
    let mut body = vec![0u8; length];
    input.read_exact(&mut body)?;
    Ok(Some(String::from_utf8_lossy(&body).into_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufReader, Cursor};

    #[test]
    fn write_produces_spec_compliant_frame() {
        let mut buf = Vec::new();
        write_message(&mut buf, "{}").unwrap();
        assert_eq!(buf, b"Content-Length: 2\r\n\r\n{}");
    }

    #[test]
    fn content_length_counts_bytes_not_chars() {
        let mut buf = Vec::new();
        write_message(&mut buf, "中").unwrap();
        assert_eq!(buf, "Content-Length: 3\r\n\r\n中".as_bytes());
    }

    #[test]
    fn round_trip_single_message() {
        let mut data = Vec::new();
        write_message(&mut data, "{\"jsonrpc\":\"2.0\"}").unwrap();
        let mut reader = BufReader::new(Cursor::new(data));
        assert_eq!(
            read_message(&mut reader).unwrap().as_deref(),
            Some("{\"jsonrpc\":\"2.0\"}")
        );
        assert_eq!(read_message(&mut reader).unwrap(), None);
    }

    #[test]
    fn multiple_frames_in_one_stream() {
        let mut data = Vec::new();
        write_message(&mut data, "one").unwrap();
        write_message(&mut data, "two").unwrap();
        write_message(&mut data, "three").unwrap();
        let mut reader = BufReader::new(Cursor::new(data));
        assert_eq!(read_message(&mut reader).unwrap().as_deref(), Some("one"));
        assert_eq!(read_message(&mut reader).unwrap().as_deref(), Some("two"));
        assert_eq!(read_message(&mut reader).unwrap().as_deref(), Some("three"));
        assert_eq!(read_message(&mut reader).unwrap(), None);
    }

    #[test]
    fn tolerates_extra_headers_and_case_insensitive_length() {
        let data =
            b"Content-Type: application/vscode-jsonrpc; charset=utf-8\r\ncontent-length: 2\r\n\r\n{}";
        let mut reader = BufReader::new(Cursor::new(data.as_slice()));
        assert_eq!(read_message(&mut reader).unwrap().as_deref(), Some("{}"));
    }

    #[test]
    fn missing_length_is_error() {
        let data = b"Content-Type: text\r\n\r\n{}";
        let mut reader = BufReader::new(Cursor::new(data.as_slice()));
        assert!(read_message(&mut reader).is_err());
    }
}
