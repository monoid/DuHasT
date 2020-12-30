extern crate rand_chacha;
extern crate rand_core;
extern crate serde_bencoded;

use std::borrow::Cow;
use std::fmt;
use std::fs::File;
use std::io::{Read, Write};
use std::net::Ipv4Addr;
use std::net::SocketAddrV4;

use fmt::Debug;
use rand::rngs::OsRng;
use rand::{CryptoRng, Rng, RngCore, SeedableRng};
use rand_chacha::ChaCha20Rng;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

const DHT_ID_BYTE_SIZE: usize = 160 / 8;
// Standard 4 bytes IPv4 address + 2 bytes port
const NODE_ADDR_BYTE_SIZE: usize = 6;
const COMPACT_NODE_BYTE_SIZE: usize = DHT_ID_BYTE_SIZE + NODE_ADDR_BYTE_SIZE;
pub(crate) const DEFAULT_STATE_PATH: &'static str = "duhast.state";

type KeyBuf = [u8; DHT_ID_BYTE_SIZE];
type NodeBuf = [u8; NODE_ADDR_BYTE_SIZE];
type ContactIdBuf = [u8; COMPACT_NODE_BYTE_SIZE];

/// 20-byte node id/torrent id.
#[derive(Clone, Default, PartialEq, Eq)]
pub(crate) struct DhtId(pub(crate) KeyBuf);

impl DhtId {
    pub(crate) fn new<R: Rng + CryptoRng>(rng: &mut R) -> Self {
        let mut buf: KeyBuf = Default::default();
        rng.fill(&mut buf);
        DhtId(buf)
    }

    pub(crate) fn from_str(s: &str) -> Result<Self, &'static str> {
        if s.len() == 40 {
            let mut buf: KeyBuf = Default::default();
            // TODO: as_bytes().chunks(2) is a dirty hack.
            for (b, c) in buf.iter_mut().zip(
                s.as_bytes()
                    .chunks(2)
                    .map(|x| std::str::from_utf8(x).unwrap()),
            ) {
                *b = u8::from_str_radix(c, 16).map_err(|_| "malformed hex")?;
            }
            Ok(DhtId(buf))
        } else {
            Err("expecting 40 char string")
        }
    }
}

impl fmt::Display for DhtId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for b in &self.0 {
            write!(f, "{:02x}", b)?;
        }
        Ok(())
    }
}

impl fmt::Debug for DhtId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for b in &self.0 {
            write!(f, "{:02x}", b)?;
        }
        Ok(())
    }
}

// Serde doesn't yet call serialize_bytes; call it manually.
impl Serialize for DhtId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_bytes(&self.0)
    }
}

/// Packed IPv4 + port address.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NodeAddr(NodeBuf);

// Serde doesn't yet call serialize_bytes; call it manually.
impl Serialize for NodeAddr {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_bytes(&self.0)
    }
}

struct NodeAddrDeserializerVisitor;

impl<'de> serde::de::Visitor<'de> for NodeAddrDeserializerVisitor {
    type Value = NodeAddr;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        write!(formatter, "{} bytes", NODE_ADDR_BYTE_SIZE)
    }

    fn visit_bytes<E: serde::de::Error>(self, v: &[u8]) -> Result<Self::Value, E> {
        if v.len() == NODE_ADDR_BYTE_SIZE {
            let mut buf: NodeBuf = Default::default();
            &buf.copy_from_slice(&v[..]);
            Ok(NodeAddr(buf))
        } else {
            Err(E::invalid_length(v.len(), &"6 bytes"))
        }
    }

    fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Self::Value, E> {
        self.visit_bytes(v.as_bytes())
    }
}

// Serde doesn't yet call serialize_bytes; call it manually.
impl<'de> Deserialize<'de> for NodeAddr {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_bytes(NodeAddrDeserializerVisitor)
    }
}

// node ID and ipv4/port pair packed together.
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct DhtContactId(ContactIdBuf);

impl DhtContactId {
    fn new(dht_id: &DhtId, socket_addr: &SocketAddrV4) -> Self {
        let mut buf: ContactIdBuf = Default::default();

        let mut slice = &mut buf[..];
        slice.write(&dht_id.0).unwrap();
        slice.write(&socket_addr.ip().octets()).unwrap();
        // be is Big Endian, the Network Byte Order
        slice.write(&socket_addr.port().to_be_bytes()).unwrap();

        debug_assert!(slice.is_empty());

        DhtContactId(buf)
    }
}

impl Serialize for DhtContactId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_bytes(&self.0)
    }
}

struct DhtIdDeserializerVisitor;

impl<'de> serde::de::Visitor<'de> for DhtIdDeserializerVisitor {
    type Value = DhtId;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        write!(formatter, "{} bytes", DHT_ID_BYTE_SIZE)
    }

    fn visit_bytes<E: serde::de::Error>(self, v: &[u8]) -> Result<Self::Value, E> {
        if v.len() == DHT_ID_BYTE_SIZE {
            let mut buf: KeyBuf = Default::default();
            &buf.copy_from_slice(&v[..]);
            Ok(DhtId(buf))
        } else {
            Err(E::invalid_length(v.len(), &"20 bytes"))
        }
    }

    fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Self::Value, E> {
        self.visit_bytes(v.as_bytes())
    }
}

// Serde doesn't yet call serialize_bytes; call it manually.
impl<'de> Deserialize<'de> for DhtId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_bytes(DhtIdDeserializerVisitor)
    }
}

#[derive(Debug)]
pub(crate) struct CompactNode {
    pub(crate) id: DhtId,
    pub(crate) ip: Ipv4Addr,
    pub(crate) port: u16,
}

impl CompactNode {
    fn unpack(buf: &[u8]) -> Self {
        assert!(buf.len() == COMPACT_NODE_BYTE_SIZE);
        let mut id: DhtId = Default::default();
        id.0.copy_from_slice(&buf[..20]);
        let ip = Ipv4Addr::new(buf[20], buf[21], buf[22], buf[23]);
        let port = u16::from_le_bytes([buf[24], buf[25]]);
        Self { id, ip, port }
    }
}

#[derive(PartialEq, Eq)]
pub(crate) struct CompactNodesList<'msg>(Cow<'msg, [u8]>);

impl<'msg> CompactNodesList<'msg> {
    fn iter(&'msg self) -> impl Iterator<Item = CompactNode> + 'msg {
        self.0
            .chunks(COMPACT_NODE_BYTE_SIZE)
            .map(CompactNode::unpack)
    }
}

impl Debug for CompactNodesList<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Slow, but works
        let data: Vec<_> = self.iter().collect();
        Debug::fmt(&data[..], f)
    }
}

impl<'msg> Serialize for CompactNodesList<'msg> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_bytes(&self.0)
    }
}

struct CompactNodesListDeserializerVisitor;

impl<'de> serde::de::Visitor<'de> for CompactNodesListDeserializerVisitor {
    type Value = CompactNodesList<'de>;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        write!(formatter, "{} bytes", DHT_ID_BYTE_SIZE)
    }

    fn visit_borrowed_bytes<E: serde::de::Error>(self, v: &'de [u8]) -> Result<Self::Value, E> {
        if v.len() % COMPACT_NODE_BYTE_SIZE == 0 {
            Ok(CompactNodesList(Cow::Borrowed(v)))
        } else {
            Err(E::invalid_length(v.len(), &"divisible by 26 bytes"))
        }
    }

    fn visit_byte_buf<E: serde::de::Error>(self, v: Vec<u8>) -> Result<Self::Value, E> {
        if v.len() % COMPACT_NODE_BYTE_SIZE == 0 {
            Ok(CompactNodesList(Cow::Owned(v)))
        } else {
            Err(E::invalid_length(v.len(), &"divisible by 26 bytes"))
        }
    }
}

// Serde doesn't yet call serialize_bytes; call it manually.
impl<'de: 'a, 'a> Deserialize<'de> for CompactNodesList<'a> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_bytes(CompactNodesListDeserializerVisitor)
    }
}

#[derive(Deserialize, Serialize, Debug, Eq, PartialEq)]
pub(crate) struct PingQuery {
    pub(crate) id: DhtId,
}

#[derive(Deserialize, Serialize, Debug, Eq, PartialEq)]
pub(crate) struct FindNodeQuery {
    pub(crate) id: DhtId,
    pub(crate) target: DhtId,
}

#[derive(Deserialize, Serialize, Debug, Eq, PartialEq)]
pub(crate) struct GetPeersQuery {
    pub(crate) id: DhtId,
    pub(crate) info_hash: DhtId,
}

#[derive(Deserialize, Serialize, Debug, Eq, PartialEq)]
pub(crate) struct AnnouncePeerQuery<'msg> {
    pub(crate) id: DhtId,
    pub(crate) info_hash: DhtId,
    #[serde(borrow, with = "serde_bytes")]
    pub(crate) token: Cow<'msg, [u8]>,
    pub(crate) port: u16,
    pub(crate) implied_port: u8,
}

#[derive(Deserialize, Serialize, Debug, Eq, PartialEq)]
#[serde(tag = "q", content = "a")]
pub(crate) enum Query<'msg> {
    #[serde(rename = "ping")]
    Ping(PingQuery),
    #[serde(rename = "find_node")]
    FindNode(FindNodeQuery),
    #[serde(rename = "get_peers")]
    GetPeers(GetPeersQuery),
    #[serde(borrow, rename = "announce_peer")]
    AnnouncePeer(AnnouncePeerQuery<'msg>),
}

#[derive(Deserialize, Serialize, Debug, PartialEq)]
pub(crate) struct PingResponse {
    id: DhtId,
}

#[derive(Deserialize, Serialize, Debug, Eq, PartialEq)]
pub(crate) struct FindNodeResponse<'msg> {
    id: DhtId,
    #[serde(borrow)]
    nodes: CompactNodesList<'msg>,
}

#[derive(Deserialize, Serialize, Debug, PartialEq, Eq)]
pub(crate) struct GetPeersResponse<'msg> {
    id: DhtId,
    #[serde(borrow)]
    token: Cow<'msg, [u8]>,
    values: Option<Vec<NodeAddr>>,
    #[serde(borrow)]
    nodes: Option<CompactNodesList<'msg>>,
}

#[derive(Deserialize, Serialize, Debug, Eq, PartialEq)]
pub(crate) struct AnnouncePeerResponse {
    id: DhtId,
}

type ErrorKind = u32;

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq)]
#[serde(tag = "y")]
pub(crate) enum Message<'msg, R> {
    #[serde(borrow, rename = "q")]
    Q(Query<'msg>),
    #[serde(rename = "r")]
    R { r: R },
    #[serde(rename = "e")]
    E { e: (ErrorKind, String) },
}

// When `y` is "q", it is an incoming query with new `t` token.  When
// `y` is "r" or "e", it is response to old quer with `t` that should
// be known, and with this knowledge one can parse it.
#[derive(Deserialize, Debug, Eq, PartialEq)]
pub(crate) struct IncomingMessage<'msg> {
    #[serde(borrow)]
    pub(crate) y: &'msg str,
    #[serde(borrow, with = "serde_bytes")]
    pub(crate) t: &'msg [u8],
}

#[derive(Serialize, Debug)]
pub(crate) struct OutgoingMessage<'msg, R> {
    #[serde(borrow, with = "serde_bytes")]
    pub(crate) t: Cow<'msg, [u8]>,
    #[serde(borrow, flatten)]
    pub(crate) msg: Message<'msg, R>,
}

#[derive(Deserialize, Serialize)]
pub(crate) struct Config {
    pub(crate) dht_id: DhtId,
    peers: Vec<String>, // String is a stub here.
}

impl Config {
    pub(crate) fn new<R: Rng + CryptoRng>(rng: &mut R) -> Self {
        Config {
            dht_id: DhtId::new(rng),
            peers: vec![],
        }
    }

    pub(crate) fn load(filename: &str) -> Result<Config, serde_bencoded::DeError> {
        let mut file = File::open(filename).unwrap();
        let mut config_data = vec![];
        file.read_to_end(&mut config_data).unwrap();
        serde_bencoded::from_bytes::<Config>(&config_data)
    }

    pub(crate) fn write(&self, filename: &str) -> Result<(), serde_bencoded::SerError> {
        let config_data = serde_bencoded::to_vec(self)?;
        let mut file = File::create(filename).unwrap();
        file.write(&config_data).unwrap();
        Ok(())
    }
}

pub(crate) fn init_chacha() -> ChaCha20Rng {
    let mut random: <ChaCha20Rng as SeedableRng>::Seed = Default::default();
    OsRng.fill_bytes(&mut random);
    ChaCha20Rng::from_seed(random)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;

    #[test]
    fn test_unpack_incoming_msg() -> Result<(), Box<dyn Error>> {
        const DATA: &[u8] = b"d1:ad2:id20:\xFFbcdefghij0123456789e1:q4:ping1:y1:q1:t2:\xFF\xFFe";
        let ping: IncomingMessage = serde_bencoded::from_bytes(&DATA)?;

        assert_eq!(
            ping,
            IncomingMessage {
                y: "q",
                t: b"\xFF\xFF"
            }
        );
        Ok(())
    }

    #[test]
    fn test_unpack_ping_query() -> Result<(), Box<dyn Error>> {
        const DATA: &[u8] =
            b"d1:ad2:id20:\xFFbcdefghij0123456789e1:q4:ping1:t2:aa1:y1:q1:t2:\xFF\xFFe";
        let ping: Message<()> = serde_bencoded::from_bytes(&DATA)?;

        assert_eq!(
            ping,
            Message::Q(Query::Ping(PingQuery {
                id: DhtId(b"\xFFbcdefghij0123456789".clone())
            }))
        );
        Ok(())
    }

    #[test]
    fn test_unpack_find_node_query() -> Result<(), Box<dyn Error>> {
        const DATA: &[u8] = b"d1:ad2:id20:abcdefghij01234567896:target20:mnopqrstuvwxyz123456e1:q9:find_node1:t2:aa1:y1:qe";
        let find_node: Message<()> = serde_bencoded::from_bytes(&DATA)?;

        assert_eq!(
            find_node,
            Message::Q(Query::FindNode(FindNodeQuery {
                id: DhtId(b"abcdefghij0123456789".clone()),
                target: DhtId(b"mnopqrstuvwxyz123456".clone()),
            }))
        );
        Ok(())
    }

    #[test]
    fn test_unpack_get_peers_query() -> Result<(), Box<dyn Error>> {
        const DATA: &[u8] = b"d1:ad2:id20:abcdefghij01234567899:info_hash20:mnopqrstuvwxyz123456e1:q9:get_peers1:t2:aa1:y1:qe";
        let get_peers: Message<()> = serde_bencoded::from_bytes(&DATA)?;
        assert_eq!(
            get_peers,
            Message::Q(Query::GetPeers(GetPeersQuery {
                id: DhtId(b"abcdefghij0123456789".clone()),
                info_hash: DhtId(b"mnopqrstuvwxyz123456".clone()),
            }))
        );
        Ok(())
    }

    #[test]
    fn test_unpack_announce_peer_query() -> Result<(), Box<dyn Error>> {
        const DATA: &[u8] = b"d1:ad2:id20:abcdefghij012345678912:implied_porti1e9:info_hash20:mnopqrstuvwxyz1234564:porti6881e5:token8:aoeusnthe1:q13:announce_peer1:t2:aa1:y1:qe";
        let announce_peer: Message<()> = serde_bencoded::from_bytes(&DATA)?;
        assert_eq!(
            announce_peer,
            Message::Q(Query::AnnouncePeer(AnnouncePeerQuery {
                id: DhtId(b"abcdefghij0123456789".clone()),
                implied_port: 1,
                info_hash: DhtId(b"mnopqrstuvwxyz123456".clone()),
                port: 6881,
                token: Cow::Borrowed(b"aoeusnth")
            }))
        );
        Ok(())
    }

    #[test]
    fn test_unpack_ping_response() -> Result<(), Box<dyn Error>> {
        const DATA: &[u8] = b"d1:rd2:id20:mnopqrstuvwxyz123456e1:t2:aa1:y1:re";
        let ping: Message<PingResponse> = serde_bencoded::from_bytes(&DATA)?;
        assert_eq!(
            ping,
            Message::R {
                r: PingResponse {
                    id: DhtId(b"mnopqrstuvwxyz123456".clone()),
                }
            }
        );
        Ok(())
    }

    #[test]
    fn test_unpack_find_node_response() -> Result<(), Box<dyn Error>> {
        const DATA: &[u8] =
            b"d1:rd2:id20:0123456789abcdefghij5:nodes26:01234567890123456789abcdefe1:t2:aa1:y1:re";
        let find_node: Message<FindNodeResponse> = serde_bencoded::from_bytes(&DATA)?;
        assert_eq!(
            find_node,
            Message::R {
                r: FindNodeResponse {
                    id: DhtId(b"0123456789abcdefghij".clone()),
                    nodes: CompactNodesList(Cow::Owned(Vec::from(
                        b"01234567890123456789abcdef".clone()
                    )))
                }
            }
        );
        Ok(())
    }

    #[test]
    fn test_unpack_get_peers_response_values() -> Result<(), Box<dyn Error>> {
        const DATA: &[u8] =
    b"d1:rd2:id20:abcdefghij01234567895:token8:aoeusnth6:valuesl6:axje.u6:idhtnmee1:t2:aa1:y1:re";
        let get_peers: Message<GetPeersResponse> = serde_bencoded::from_bytes(DATA)?;
        assert_eq!(
            get_peers,
            Message::R {
                r: GetPeersResponse {
                    id: DhtId(b"abcdefghij0123456789".clone()),
                    token: Cow::Borrowed(b"aoeusnth"),
                    values: Some(vec![
                        NodeAddr(b"axje.u".clone()),
                        NodeAddr(b"idhtnm".clone())
                    ]),
                    nodes: None,
                }
            }
        );
        Ok(())
    }

    #[test]
    fn test_unpack_get_peers_response_nodes() -> Result<(), Box<dyn Error>> {
        const DATA: &[u8] =
    b"d1:rd2:id20:abcdefghij01234567895:token8:aoeusnth5:nodes26:01234567890123456789012345e1:t2:aa1:y1:re";
        let get_peers: Message<GetPeersResponse> = serde_bencoded::from_bytes(DATA)?;
        assert_eq!(
            get_peers,
            Message::R {
                r: GetPeersResponse {
                    id: DhtId(b"abcdefghij0123456789".clone()),
                    token: Cow::Borrowed(b"aoeusnth"),
                    values: None,
                    nodes: Some(CompactNodesList(Cow::Owned(Vec::from(
                        b"01234567890123456789012345".clone()
                    )))),
                }
            }
        );
        Ok(())
    }

    #[test]
    fn test_unpack_announce_peer_response() -> Result<(), Box<dyn Error>> {
        const DATA: &[u8] = b"d1:rd2:id20:mnopqrstuvwxyz123456e1:t2:aa1:y1:re";
        let announce_peers: Message<AnnouncePeerResponse> = serde_bencoded::from_bytes(DATA)?;
        assert_eq!(
            announce_peers,
            Message::R {
                r: AnnouncePeerResponse {
                    id: DhtId(b"mnopqrstuvwxyz123456".clone()),
                }
            }
        );
        Ok(())
    }

    #[test]
    fn test_unpack_error_response() -> Result<(), Box<dyn Error>> {
        const DATA: &[u8] = b"d1:eli201e23:A Generic Error Ocurrede1:t2:aa1:y1:ee";
        let err: Message<PingResponse> = serde_bencoded::from_bytes(DATA)?;

        assert!(matches!(dbg!(err), Message::E{e: (201, _)}));
        Ok(())
    }
}
