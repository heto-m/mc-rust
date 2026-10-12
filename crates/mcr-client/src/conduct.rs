use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};

/// A server's code of conduct is shown, never cut by more than this.
pub const MAX_CONDUCT_CHARS: usize = 16_384;

/// Java's `String.hashCode`, which vanilla stores in `servers.dat` to remember an acceptance.
pub fn conduct_hash(text: &str) -> i32 {
    text.encode_utf16()
        .fold(0i32, |h, u| h.wrapping_mul(31).wrapping_add(i32::from(u)))
}

/// The text with controls and formatting escapes removed, bounded for display.
pub fn sanitize_conduct(text: &str) -> String {
    text.chars()
        .filter(|c| (!c.is_control() || matches!(c, '\n' | '\t')) && *c != '§')
        .take(MAX_CONDUCT_CHARS)
        .collect()
}

#[derive(Debug, Clone)]
pub struct ConductRequest {
    pub text: String,
    pub hash: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConductReply {
    Accept,
    Decline,
}

#[derive(Debug, Clone)]
pub struct ConductLink {
    pub(crate) requests: Sender<ConductRequest>,
    replies: Arc<Mutex<Receiver<ConductReply>>>,
}

impl ConductLink {
    pub(crate) fn next_reply(&self) -> Option<ConductReply> {
        self.replies.lock().ok()?.try_recv().ok()
    }
}

#[derive(Debug)]
pub struct ConductEnd {
    pub requests: Receiver<ConductRequest>,
    replies: Sender<ConductReply>,
}

impl ConductEnd {
    pub fn reply(&self, reply: ConductReply) {
        let _ = self.replies.send(reply);
    }
}

pub fn conduct_channel() -> (ConductLink, ConductEnd) {
    let (request_tx, request_rx) = channel();
    let (reply_tx, reply_rx) = channel();
    (
        ConductLink {
            requests: request_tx,
            replies: Arc::new(Mutex::new(reply_rx)),
        },
        ConductEnd {
            requests: request_rx,
            replies: reply_tx,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_matches_java() {
        assert_eq!(conduct_hash(""), 0);
        assert_eq!(conduct_hash("hello"), 99_162_322);
        let surrogates = (97i32 * 31 + 0xD83D).wrapping_mul(31).wrapping_add(0xDE00);
        assert_eq!(conduct_hash("a\u{1F600}"), surrogates);
    }

    #[test]
    fn sanitize_strips_escapes_and_bounds() {
        assert_eq!(sanitize_conduct("a§cb\u{7}\nc"), "acb\nc");
        let long = "x".repeat(MAX_CONDUCT_CHARS * 2);
        assert_eq!(sanitize_conduct(&long).len(), MAX_CONDUCT_CHARS);
    }
}
