//! Reads a streamed chat reply (`text/event-stream`, the OpenAI format)
//! as it arrives. Contract: takes bytes in any size of piece, keeps only
//! the reply text and one partial line, and never holds more than
//! [`MAX_REPLY_BYTES`] plus [`MAX_LINE_BYTES`], however much a server sends.

use std::mem;

use serde::Deserialize;

use crate::error::{AssistError, Service};

/// The longest reply accepted. Five commands with descriptions need a few
/// kilobytes; a model that runs on is cut off here.
pub(crate) const MAX_REPLY_BYTES: usize = 256 * 1024;

/// The longest single line of the stream.
const MAX_LINE_BYTES: usize = 64 * 1024;

/// The reply so far.
#[derive(Debug, Default)]
pub(crate) struct ChatStream {
    line: Vec<u8>,
    content: String,
    chars: usize,
    finished: bool,
}

impl ChatStream {
    /// Take the next piece of the stream.
    pub(crate) fn push(&mut self, bytes: &[u8]) -> Result<(), AssistError> {
        for piece in bytes.split_inclusive(|byte| *byte == b'\n') {
            if self.line.len() + piece.len() > MAX_LINE_BYTES {
                return Err(too_long());
            }
            self.line.extend_from_slice(piece);
            if piece.ends_with(b"\n") {
                self.end_line()?;
            }
        }
        Ok(())
    }

    /// Characters of reply so far.
    pub(crate) fn chars(&self) -> usize {
        self.chars
    }

    /// The stream ended: return the reply text.
    pub(crate) fn finish(mut self) -> Result<String, AssistError> {
        self.end_line()?;
        Ok(self.content)
    }

    fn end_line(&mut self) -> Result<(), AssistError> {
        let line = mem::take(&mut self.line);
        if self.finished {
            return Ok(());
        }
        let text = std::str::from_utf8(&line)
            .map_err(|error| AssistError::invalid(Service::Model, error))?;
        // Event names, ids and `:` keep-alive comments carry no reply text.
        let Some(data) = text.trim().strip_prefix("data:") else {
            return Ok(());
        };
        match data.trim() {
            "[DONE]" => self.finished = true,
            data => self.add_event(data)?,
        }
        Ok(())
    }

    fn add_event(&mut self, data: &str) -> Result<(), AssistError> {
        let event: Event = serde_json::from_str(data)
            .map_err(|error| AssistError::invalid(Service::Model, error))?;
        let text = event
            .choices
            .into_iter()
            .next()
            .and_then(|choice| choice.delta.content)
            .unwrap_or_default();
        if self.content.len() + text.len() > MAX_REPLY_BYTES {
            return Err(too_long());
        }
        self.chars += text.chars().count();
        self.content.push_str(&text);
        Ok(())
    }
}

/// The error for a reply over the limits.
pub(crate) fn too_long() -> AssistError {
    AssistError::invalid(Service::Model, "the reply is too long")
}

#[derive(Deserialize)]
struct Event {
    #[serde(default)]
    choices: Vec<EventChoice>,
}

#[derive(Deserialize)]
struct EventChoice {
    #[serde(default)]
    delta: Delta,
}

#[derive(Default, Deserialize)]
struct Delta {
    content: Option<String>,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{ChatStream, MAX_LINE_BYTES, MAX_REPLY_BYTES};
    use crate::error::{AssistError, Service};

    fn event(text: &str) -> String {
        let event = json!({ "choices": [{ "delta": { "content": text } }] });
        format!("data: {event}\n\n")
    }

    fn is_invalid(result: Result<(), AssistError>) -> bool {
        matches!(
            result,
            Err(AssistError::InvalidReply {
                service: Service::Model,
                ..
            })
        )
    }

    #[test]
    fn given_any_split_of_the_bytes_then_the_reply_is_the_same() {
        let body = format!(
            ": keep-alive\nevent: x\n{}{}{}data: [DONE]\n\n{}",
            event("héllo "),
            event("wörld"),
            "data: {\"choices\":[{\"delta\":{}}]}\n\n",
            event("after the end"),
        );

        for size in 1..body.len() {
            let mut stream = ChatStream::default();
            for piece in body.as_bytes().chunks(size) {
                stream.push(piece).expect("valid stream");
            }
            assert_eq!(stream.chars(), 11, "pieces of {size}");
            assert_eq!(stream.finish().as_deref(), Ok("héllo wörld"));
        }
    }

    #[test]
    fn given_a_last_line_with_no_line_break_then_it_still_counts() {
        let mut stream = ChatStream::default();
        let last = event("end");
        stream.push(last.trim_end().as_bytes()).expect("valid");
        assert_eq!(stream.finish().as_deref(), Ok("end"));
    }

    #[test]
    fn given_an_error_event_then_there_is_no_text() {
        let mut stream = ChatStream::default();
        stream
            .push(b"data: {\"error\":{\"message\":\"boom\"}}\n\n")
            .expect("not a reply, but valid");
        assert_eq!(stream.finish().as_deref(), Ok(""));
    }

    #[test]
    fn given_bad_data_then_it_is_invalid() {
        assert!(is_invalid(ChatStream::default().push(b"data: {oops\n")));
        assert!(is_invalid(ChatStream::default().push(b"data: \xff\n")));
    }

    #[test]
    fn given_a_line_that_never_ends_then_it_stops_at_the_limit() {
        let mut stream = ChatStream::default();
        let piece = vec![b'x'; 1024];
        let mut result = Ok(());
        for _ in 0..=MAX_LINE_BYTES / 1024 {
            result = stream.push(&piece);
            if result.is_err() {
                break;
            }
        }
        assert!(is_invalid(result));
        assert!(stream.line.len() <= MAX_LINE_BYTES);
    }

    #[test]
    fn given_a_reply_that_never_ends_then_it_stops_at_the_limit() {
        let mut stream = ChatStream::default();
        let piece = event(&"x".repeat(1024));
        let mut result = Ok(());
        for _ in 0..=MAX_REPLY_BYTES / 1024 {
            result = stream.push(piece.as_bytes());
            if result.is_err() {
                break;
            }
        }
        assert!(is_invalid(result));
        assert!(stream.content.len() <= MAX_REPLY_BYTES);
    }
}
