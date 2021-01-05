use crate::dht;
use serde::Serialize;
use std::borrow::Cow;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::UdpSocket;
use tokio::sync::oneshot;
use std::sync::Mutex as StdMutex;

struct TimeoutError {}

pub(crate) type QueryId = u16;

struct ReplyInfo {
    send: oneshot::Sender<Vec<u8>>,
}

/// Each node (ip + port combination) has its own queue.
pub struct NodeQueue {
    id: QueryId,
    // TODO: wrap with Option to make deallocatable?
    // We do not want drop NodeQueue as it will reset the id counter.
    // We drop it only if it is dead.
    waiting_for_reply: HashMap<QueryId, ReplyInfo>,
}

impl NodeQueue {
    pub fn new() -> Self {
        Self {
            id: Default::default(),
            waiting_for_reply: Default::default(),
        }
    }

    #[inline]
    pub fn get_next_id(&mut self) -> QueryId {
        self.id += 1;
        self.id
    }

    pub fn add_reply_info(&mut self, id: QueryId, send: oneshot::Sender<Vec<u8>>) {
        // TODO we are hiding here a previous reply if it still
        // exists.  Misconfigured instances may get misrouted
        // messages.
        self.waiting_for_reply.insert(id, ReplyInfo { send });
    }

    pub fn got_reply(&mut self, id: QueryId, packet: Vec<u8>) {
        if let Some((_, info)) = self.waiting_for_reply.remove_entry(&id) {
            // If receiver doesn't exist anymore, not problem at all.
            let _ = info.send.send(packet).unwrap();
        }
    }

    pub fn remove(&mut self, id: QueryId) {
        self.waiting_for_reply.remove(&id);
    }
}

impl Default for NodeQueue {
    fn default() -> Self {
        Self::new()
    }
}

pub struct QueryQueue {
    timeout: Duration,
    // A std mutex can be used instead.
    nodes: StdMutex<HashMap<SocketAddr, NodeQueue>>,
}

impl QueryQueue {
    pub fn new(timeout: Duration) -> Self {
        Self {
            timeout,
            nodes: StdMutex::new(Default::default()),
        }
    }

    pub(crate) async fn send_message<R: Serialize>(
        self: Arc<Self>,
        udp: Arc<UdpSocket>,
        sock_addr: SocketAddr,
        msg: dht::Message<'static, R>,
    ) -> Result<Vec<u8>, ()> {
        let (send, recv) = oneshot::channel();
        let id = {
            // expect is reasonable here because if nodes lock is poisoned,
            // we can only crash.
            let mut guard = self.nodes.lock().expect("cannot handle poinsoned lock");
            let node_queue = guard.entry(sock_addr).or_default();

            node_queue.get_next_id()
        };
        let id_bytes = id.to_be_bytes();

        let out_msg = dht::OutgoingMessage {
            t: Cow::Borrowed(&id_bytes),
            msg,
        };

        if let Ok(buf) = serde_bencoded::to_vec(&out_msg) {
            udp.send_to(&buf, sock_addr).await.map_err(|_| ())?;

            {
                let mut guard = self.nodes.lock().expect("cannot handle poinsoned lock");
                let node_queue = guard.entry(sock_addr).or_default();
                node_queue.add_reply_info(id, send);
            }

            let timeout = self.timeout;

            tokio::select! {
                res = recv => {
                    res.map_err(|_| ())
                }
                _ = tokio::time::sleep(timeout) => {
                    // clear
                    self.query_expired(sock_addr, id);
                    Err(())
                }
            }
        } else {
            Err(())
        }
    }

    fn query_expired(&self, addr: SocketAddr, id: QueryId) {
        let mut guard = self.nodes.lock().expect("cannot handle poinsoned lock");
        if let Some(node_queue) = guard.get_mut(&addr) {
            node_queue.remove(id);
        }
    }

    // It handles only normal replies and error replies.
    pub(crate) fn got_reply(&self, sock_addr: SocketAddr, id: QueryId, packet: Vec<u8>) {
        let mut guard = self.nodes.lock().expect("cannot handle poinsoned lock");
        if let Some(node_info) = guard.get_mut(&sock_addr) {
            node_info.got_reply(id, packet)
        } else {
            // TODO logging
            eprintln!("WARNING: Not found node info for {}", sock_addr);
        }
    }

    pub(crate) async fn declare_dead(&self, sock_addr: SocketAddr) {
        self.nodes.lock()
            .expect("cannot handle poinsoned lock")
            .remove(&sock_addr);
    }
}
