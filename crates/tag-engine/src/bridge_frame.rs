//! Bounded protocol-3 newline framing for a future supervised child connection.
use std::io::{self, BufRead, Write};

/// Maximum payload bytes, excluding the newline.
pub const MAX_FRAME_BYTES: usize = 1024 * 1024;

/// Read one complete frame. Any error is terminal for this transport session.
pub fn read_frame(reader: &mut impl BufRead) -> io::Result<Option<Vec<u8>>> {
    let mut frame = Vec::new();
    loop {
        let available = reader.fill_buf()?;
        if available.is_empty() {
            return if frame.is_empty() {
                Ok(None)
            } else {
                Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "partial frame",
                ))
            };
        }
        let newline = available.iter().position(|&b| b == b'\n');
        let count = newline.unwrap_or(available.len());
        if count > MAX_FRAME_BYTES - frame.len() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "oversized frame",
            ));
        }
        frame.extend_from_slice(&available[..count]);
        reader.consume(count + usize::from(newline.is_some()));
        if newline.is_some() {
            if frame.is_empty() {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "empty frame"));
            }
            return Ok(Some(frame));
        }
    }
}

/// Write a single bounded frame. The caller must terminate the session on I/O error.
pub fn write_frame(writer: &mut impl Write, frame: &[u8]) -> io::Result<()> {
    if frame.is_empty() || frame.len() > MAX_FRAME_BYTES || frame.contains(&b'\n') {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "invalid frame"));
    }
    writer.write_all(frame)?;
    writer.write_all(b"\n")?;
    writer.flush()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufReader, Cursor};

    #[test]
    fn fragmented_frames_and_clean_eof() -> io::Result<()> {
        let mut input = BufReader::with_capacity(2, Cursor::new(b"abc\ndef\n"));
        assert_eq!(read_frame(&mut input)?, Some(b"abc".to_vec()));
        assert_eq!(read_frame(&mut input)?, Some(b"def".to_vec()));
        assert_eq!(read_frame(&mut input)?, None);
        Ok(())
    }

    #[test]
    fn rejects_partial_empty_and_oversized() {
        assert!(read_frame(&mut Cursor::new(b"abc")).is_err());
        assert!(read_frame(&mut Cursor::new(b"\n")).is_err());
        assert!(read_frame(&mut Cursor::new(vec![b'x'; MAX_FRAME_BYTES + 1])).is_err());
    }

    #[test]
    fn exact_limit_and_output_validation() -> io::Result<()> {
        let payload = vec![b'x'; MAX_FRAME_BYTES];
        let mut wire = Vec::new();
        write_frame(&mut wire, &payload)?;
        assert_eq!(read_frame(&mut Cursor::new(wire))?, Some(payload));
        assert!(write_frame(&mut Vec::new(), b"a\nb").is_err());
        assert!(write_frame(&mut Vec::new(), b"").is_err());
        Ok(())
    }
}
