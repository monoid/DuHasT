extern crate rand_chacha;
extern crate rand_core;
extern crate serde_bencoded;

use std::fmt;
use std::fs::File;
use std::io::{Read, Write};
use std::net::SocketAddrV4;

use fmt::Debug;
use rand::rngs::OsRng;
use rand::{CryptoRng, Rng, RngCore, SeedableRng};
use rand_chacha::ChaCha20Rng;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

const DHT_ID_BYTE_SIZE: usize = 160 / 8;
// Standard 4 bytes IPv4 address + 2 bytes port
const IP_ADDR_BYTE_SIZE: usize = 6;
pub(crate) const DEFAULT_STATE_PATH: &'static str = "duhast.state";

type KeyBuf = [u8; DHT_ID_BYTE_SIZE];
type ContactIdBuf = [u8; DHT_ID_BYTE_SIZE + IP_ADDR_BYTE_SIZE];

#[derive(Clone, Default)]
pub(crate) struct DhtId(KeyBuf);

// Serde doesn't yet call serialize_bytes; call it manually.
impl Serialize for DhtId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_bytes(&self.0)
    }
}

impl DhtId {
    pub(crate) fn new<R: Rng + CryptoRng>(rng: &mut R) -> Self {
        let mut buf: KeyBuf = Default::default();
        rng.fill(&mut buf);
        DhtId(buf)
    }
}

impl Debug for DhtId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("{dht_id}")
    }
}

// Packed representation of DHT ID and ipv4/port pair.
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

impl Debug for DhtContactId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("{DHT_CONTACT_ID}")
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
}

// Serde doesn't yet call serialize_bytes; call it manually.
impl<'de> Deserialize<'de> for DhtId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_bytes(DhtIdDeserializerVisitor)
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

// TODO: generalize these two deserializer...
struct DhtContactIdDeserializerVisitor;

impl<'de> serde::de::Visitor<'de> for DhtContactIdDeserializerVisitor {
    type Value = DhtContactId;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        write!(formatter, "{} bytes", std::mem::size_of::<ContactIdBuf>())
    }

    fn visit_bytes<E: serde::de::Error>(self, v: &[u8]) -> Result<Self::Value, E> {
        if v.len() == std::mem::size_of::<ContactIdBuf>() {
            let mut buf: ContactIdBuf = Default::default();
            &buf.copy_from_slice(&v[..]);

            Ok(DhtContactId(buf))
        } else {
            Err(E::invalid_length(v.len(), &"26 bytes"))
        }
    }
}

// Serde doesn't yet call serialize_bytes; call it manually.
impl<'de> Deserialize<'de> for DhtContactId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_bytes(DhtContactIdDeserializerVisitor)
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct CompactNodeInfo {
    // TODO
}

#[derive(Deserialize, Serialize, Debug)]
pub(crate) struct PingQuery {
    pub(crate) id: DhtId,
}

#[derive(Deserialize, Serialize, Debug)]
pub(crate) struct FindNodeQuery {
    id: DhtContactId,
    target: DhtContactId,
}

#[derive(Deserialize, Serialize, Debug)]
pub(crate) struct GetPeersQuery {
    id: DhtContactId,
    info_hash: DhtId,
}

#[derive(Deserialize, Serialize, Debug)]
pub(crate) struct AnnouncePeerQuery {
    id: DhtContactId,
    implied_port: u8,
    info_hash: DhtId,
    port: u16,
    token: String, // TODO Token is a byte string
}

#[derive(Deserialize, Serialize, Debug)]
#[serde(tag = "q", content = "a")]
pub(crate) enum Query {
    #[serde(rename = "ping")]
    Ping(PingQuery),
    #[serde(rename = "find_node")]
    FindNode(FindNodeQuery),
    #[serde(rename = "get_peers")]
    GetPeers(GetPeersQuery),
    #[serde(rename = "announce_peer")]
    AnnouncePeer(AnnouncePeerQuery),
}

#[derive(Deserialize, Serialize, Debug)]
pub(crate) struct PingResponse {
    id: DhtContactId,
}

#[derive(Deserialize, Serialize, Debug)]
pub(crate) struct FindNodeResponse {
    id: DhtContactId,
    nodes: CompactNodeInfo,
}

#[derive(Deserialize, Serialize, Debug)]
pub(crate) struct GetPeersResponse {
    id: DhtContactId,
    token: String, // TODO just bytes
    values: Option<Vec<DhtContactId>>,
    nodes: Option<CompactNodeInfo>,
}

#[derive(Deserialize, Serialize, Debug)]
pub(crate) struct AnnouncePeerResponse {
    id: DhtContactId,
}

type ErrorKind = u32;

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "y")]
pub(crate) enum Message<R> {
    #[serde(rename = "q")]
    Q(Query),
    #[serde(rename = "r")]
    R { r: R },
    #[serde(rename = "e")]
    E { e: (ErrorKind, String) },
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
    fn test_unpack_ping_query() -> Result<(), Box<dyn Error>> {
        const DATA: &'static [u8] = b"d1:q4:ping1:ad2:id20:abcdefghij0123456789ee";
        // const DATA: [u8; 43] = [
        //     100u8, 49, 58, 97, 100, 50, 58, 105, 100, 50, 48, 58, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        //     0, 0, 0, 0, 0, 0, 0, 0, 0, 101, 49, 58, 113, 52, 58, 112, 105, 110, 103, 101,
        // ];
        let err: Query = serde_bencoded::from_bytes(&DATA)?;
        Ok(())
    }

    #[test]
    fn test_unpack_ping_query_1() -> Result<(), Box<dyn Error>> {
        const DATA: &'static [u8] = b"d2:id20:abcdefghij0123456789e";
        let err: PingQuery = serde_bencoded::from_bytes(DATA)?;
        Ok(())
    }

    #[test]
    fn test_unpack_find_node_query() {}

    #[test]
    fn test_unpack_get_peers_query() {}

    #[test]
    fn test_unpack_announce_peer_query() {}

    #[test]
    fn test_unpack_ping_response() {}

    #[test]
    fn test_unpack_find_node_response() {}

    #[test]
    fn test_unpack_get_peers_response() {}

    #[test]
    fn test_unpack_announce_peer_response() {}

    #[test]
    fn test_unpack_error_response() -> Result<(), Box<dyn Error>> {
        const DATA: &'static [u8] = b"d1:eli201e23:A Generic Error Ocurrede1:t2:aa1:y1:ee";
        let err: Message<PingResponse> = serde_bencoded::from_bytes(DATA)?;

        assert!(matches!(dbg!(err), Message::E{e: (201, _)}));
        Ok(())
    }
}
