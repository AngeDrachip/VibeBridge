use std::io::{BufRead, Write};

use vibe_core::error::{Result, VibeError};

use crate::protocol::Message;

/// Write exactly one JSON message plus a newline, then flush.
pub fn write_message<W: Write>(writer: &mut W, message: &Message) -> Result<()> {
    let text = serde_json::to_string(message)
        .map_err(|e| VibeError::Protocol(format!("could not serialize message: {e}")))?;
    if text.contains('\n') {
        return Err(VibeError::Protocol(
            "serialized message unexpectedly contains a newline".into(),
        ));
    }
    writer.write_all(text.as_bytes())?;
    writer.write_all(b"\n")?;
    writer.flush()?;
    Ok(())
}

/// Read one JSON message. `Ok(None)` is a clean EOF.
pub fn read_message<R: BufRead>(reader: &mut R) -> Result<Option<Message>> {
    let mut line = String::new();
    let read = reader.read_line(&mut line)?;
    if read == 0 {
        return Ok(None);
    }
    let line = line.trim();
    if line.is_empty() {
        return read_message(reader);
    }
    let message = serde_json::from_str(line)
        .map_err(|e| VibeError::Protocol(format!("invalid bridge message: {e}")))?;
    Ok(Some(message))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::Hello;

    #[test]
    fn writes_and_reads_a_message() {
        let mut buffer = Vec::new();
        let message = Message::Hello(Hello {
            protocol: 1,
            runtime: "test".into(),
            java_version: "17".into(),
            java_vendor: "test".into(),
            os: "test".into(),
            arch: "test".into(),
            pid: 1,
            supported_forge_api: Vec::new(),
        });
        write_message(&mut buffer, &message).unwrap();
        let mut reader = std::io::BufReader::new(buffer.as_slice());
        assert_eq!(read_message(&mut reader).unwrap(), Some(message));
    }
}
