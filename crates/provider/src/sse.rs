use minimax_protocol::RuntimeErrorCode;

/// Maximum size of one decoded SSE event, including field names and separators.
pub const MAX_SSE_EVENT_BYTES: usize = 1_048_576;

#[derive(Debug, Default)]
pub struct SseDecoder {
    buffer: Vec<u8>,
}

impl SseDecoder {
    #[must_use]
    pub const fn new() -> Self {
        Self { buffer: Vec::new() }
    }

    pub fn push(&mut self, chunk: &[u8]) -> Result<Vec<String>, RuntimeErrorCode> {
        self.buffer.extend_from_slice(chunk);
        self.drain(false)
    }

    pub fn finish(&mut self) -> Result<Vec<String>, RuntimeErrorCode> {
        self.drain(true)
    }

    fn drain(&mut self, final_chunk: bool) -> Result<Vec<String>, RuntimeErrorCode> {
        let mut events = Vec::new();
        while let Some(end) = blank_line_end(&self.buffer) {
            if end > MAX_SSE_EVENT_BYTES {
                return Err(RuntimeErrorCode::ProtocolMalformedJson);
            }
            let raw = self.buffer.drain(..end).collect::<Vec<_>>();
            if let Some(data) = decode_event(&raw)? {
                events.push(data);
            }
        }

        if self.buffer.len() > MAX_SSE_EVENT_BYTES {
            return Err(RuntimeErrorCode::ProtocolMalformedJson);
        }
        if final_chunk && !self.buffer.is_empty() {
            let raw = std::mem::take(&mut self.buffer);
            if let Some(data) = decode_event(&raw)? {
                events.push(data);
            }
        }
        Ok(events)
    }
}

fn blank_line_end(bytes: &[u8]) -> Option<usize> {
    let mut index = 0;
    let mut previous_was_newline = false;
    while index < bytes.len() {
        let newline_len = match bytes[index] {
            b'\n' => 1,
            b'\r' if bytes.get(index + 1) == Some(&b'\n') => 2,
            b'\r' => 1,
            _ => {
                previous_was_newline = false;
                index += 1;
                continue;
            }
        };
        index += newline_len;
        if previous_was_newline {
            return Some(index);
        }
        previous_was_newline = true;
    }
    None
}

fn decode_event(raw: &[u8]) -> Result<Option<String>, RuntimeErrorCode> {
    let raw = std::str::from_utf8(raw).map_err(|_| RuntimeErrorCode::ProtocolMalformedJson)?;
    let normalized = raw.replace("\r\n", "\n").replace('\r', "\n");
    let mut data = Vec::new();
    for line in normalized.lines() {
        if line.is_empty() || line.starts_with(':') {
            continue;
        }
        if let Some(value) = line.strip_prefix("data:") {
            data.push(value.strip_prefix(' ').unwrap_or(value));
        }
    }
    if data.is_empty() {
        Ok(None)
    } else {
        Ok(Some(data.join("\n")))
    }
}

#[cfg(test)]
mod tests {
    use super::{MAX_SSE_EVENT_BYTES, SseDecoder};

    #[test]
    fn decodes_split_crlf_comments_and_multiline_data() {
        let mut decoder = SseDecoder::new();
        assert!(
            decoder
                .push(b": keepalive\r")
                .expect("valid chunk")
                .is_empty()
        );
        let events = decoder
            .push(b"\ndata: one\r\ndata: two\r\n\r\ndata: three\n\n")
            .expect("valid stream");
        assert_eq!(events, ["one\ntwo", "three"]);
        assert!(decoder.finish().expect("clean EOF").is_empty());
    }

    #[test]
    fn rejects_invalid_utf8_and_oversize_event() {
        let mut decoder = SseDecoder::new();
        assert!(decoder.push(&[0xff, b'\n', b'\n']).is_err());
        let mut decoder = SseDecoder::new();
        assert!(decoder.push(&vec![b'x'; MAX_SSE_EVENT_BYTES + 1]).is_err());
    }
}
