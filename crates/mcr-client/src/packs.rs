use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};

use mcr_protocol::common::{ResourcePackPush, ResourcePackResponse};
use mcr_protocol::types::Component;
use uuid::Uuid;

/// What the server asked for.
#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum PackRequest {
    Push(ResourcePackPush),
    Pop(Option<Uuid>),
}

/// What the app wants sent back to the server.
#[derive(Debug, Clone)]
pub enum PackReply {
    Status(ResourcePackResponse),
    Disconnect(Component),
}

/// The client's half of the pack channel pair; it works in configuration and play alike.
#[derive(Debug, Clone)]
pub struct PackLink {
    pub(crate) requests: Sender<PackRequest>,
    replies: Arc<Mutex<Receiver<PackReply>>>,
}

impl PackLink {
    pub(crate) fn next_reply(&self) -> Option<PackReply> {
        self.replies.lock().ok()?.try_recv().ok()
    }
}

/// The app's half: read requests, answer with replies.
#[derive(Debug)]
pub struct PackEnd {
    pub requests: Receiver<PackRequest>,
    replies: Sender<PackReply>,
}

impl PackEnd {
    pub fn reply(&self, reply: PackReply) {
        let _ = self.replies.send(reply);
    }
}

pub fn pack_channel() -> (PackLink, PackEnd) {
    let (request_tx, request_rx) = channel();
    let (reply_tx, reply_rx) = channel();
    (
        PackLink {
            requests: request_tx,
            replies: Arc::new(Mutex::new(reply_rx)),
        },
        PackEnd {
            requests: request_rx,
            replies: reply_tx,
        },
    )
}
