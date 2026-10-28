//! Server-Sent Events framing (contract 9.3, after the WHATWG "Server-sent events" section).
//!
//! Bytes go in as they arrive, events come out once their blank line has. Lines end in LF, CRLF
//! or CR, a CR at the end of one chunk and an LF at the start of the next included. Comments,
//! `id:` and `retry:` lines are ignored.

/// One dispatched event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SseEvent {
    /// `message` when the event had no `event:` line.
    pub event: String,
    /// The `data:` lines joined with LF.
    pub data: String,
}

/// A line longer than this ends the stream: the server sends one short JSON object a line.
pub const MAX_LINE: usize = 1 << 20;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[error("a line of the spots stream was longer than {MAX_LINE} bytes")]
pub struct LineTooLong;

/// The parser's state between chunks.
#[derive(Debug, Default)]
pub struct SseParser {
    line: Vec<u8>,
    /// The last byte seen was a CR, so an LF right after it ends nothing.
    after_cr: bool,
    /// Still at the very start of the stream, where a byte order mark is dropped.
    at_start: bool,
    event: String,
    data: Vec<String>,
}

impl SseParser {
    pub fn new() -> Self {
        Self {
            at_start: true,
            ..Self::default()
        }
    }

    /// Feeds one chunk and returns the events it completed, in order.
    pub fn feed(&mut self, chunk: &[u8]) -> Result<Vec<SseEvent>, LineTooLong> {
        let mut events = Vec::new();
        for &byte in chunk {
            match byte {
                b'\n' if self.after_cr => self.after_cr = false,
                b'\n' | b'\r' => {
                    self.after_cr = byte == b'\r';
                    let line = std::mem::take(&mut self.line);
                    if let Some(event) = self.line_done(&line) {
                        events.push(event);
                    }
                }
                _ => {
                    self.after_cr = false;
                    if self.line.len() >= MAX_LINE {
                        return Err(LineTooLong);
                    }
                    self.line.push(byte);
                }
            }
        }
        Ok(events)
    }

    fn line_done(&mut self, line: &[u8]) -> Option<SseEvent> {
        let mut line = String::from_utf8_lossy(line).into_owned();
        if std::mem::take(&mut self.at_start) {
            if let Some(rest) = line.strip_prefix('\u{feff}') {
                line = rest.to_string();
            }
        }
        if line.is_empty() {
            return self.dispatch();
        }
        if line.starts_with(':') {
            return None;
        }
        let (field, value) = match line.split_once(':') {
            Some((field, value)) => (field, value.strip_prefix(' ').unwrap_or(value)),
            None => (line.as_str(), ""),
        };
        match field {
            "event" => self.event = value.to_string(),
            "data" => self.data.push(value.to_string()),
            // `id` and `retry` are ignored (contract 9.3), as is any other field.
            _ => {}
        }
        None
    }

    fn dispatch(&mut self) -> Option<SseEvent> {
        let event = std::mem::take(&mut self.event);
        if self.data.is_empty() {
            return None;
        }
        let data = std::mem::take(&mut self.data).join("\n");
        Some(SseEvent {
            event: if event.is_empty() {
                "message".into()
            } else {
                event
            },
            data,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(event: &str, data: &str) -> SseEvent {
        SseEvent {
            event: event.into(),
            data: data.into(),
        }
    }

    #[test]
    fn reads_the_contract_example() {
        let body = "event: hello\ndata: {\"relay_id\":\"r\",\"device_name\":\"SHACK-PC\"}\n\n: keepalive\n\nevent: spot\ndata: {\"id\":1}\n\n: keepalive\n\n";
        let mut p = SseParser::new();
        assert_eq!(
            p.feed(body.as_bytes()).unwrap(),
            [
                ev("hello", "{\"relay_id\":\"r\",\"device_name\":\"SHACK-PC\"}"),
                ev("spot", "{\"id\":1}")
            ]
        );
    }

    #[test]
    fn every_line_end_and_any_chunking() {
        let body =
            b"event: spot\r\ndata: a\r\n\r\nevent: spot\rdata: b\r\revent: spot\ndata: c\n\n";
        let want = [ev("spot", "a"), ev("spot", "b"), ev("spot", "c")];
        let mut whole = SseParser::new();
        assert_eq!(whole.feed(body).unwrap(), want);
        let mut bytewise = SseParser::new();
        let mut got = Vec::new();
        for b in body {
            got.extend(bytewise.feed(&[*b]).unwrap());
        }
        assert_eq!(got, want);
    }

    #[test]
    fn joins_data_lines_and_ignores_the_rest() {
        let body = "\u{feff}id: 7\nretry: 10\nevent: spot\ndata: one\ndata:two\nunknown: x\n\n";
        let mut p = SseParser::new();
        assert_eq!(p.feed(body.as_bytes()).unwrap(), [ev("spot", "one\ntwo")]);
    }

    #[test]
    fn an_event_without_data_or_name() {
        let mut p = SseParser::new();
        assert_eq!(p.feed(b"event: hello\n\n").unwrap(), []);
        // The name does not carry over to the next event.
        assert_eq!(p.feed(b"data: x\n\n").unwrap(), [ev("message", "x")]);
        // An unfinished event waits for its blank line.
        assert_eq!(p.feed(b"event: spot\ndata: y\n").unwrap(), []);
        assert_eq!(p.feed(b"\n").unwrap(), [ev("spot", "y")]);
    }

    #[test]
    fn a_runaway_line_is_an_error() {
        let mut p = SseParser::new();
        assert_eq!(p.feed(&vec![b'a'; MAX_LINE + 1]), Err(LineTooLong));
    }
}
