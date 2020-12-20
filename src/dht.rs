extern crate rand_chacha;
extern crate rand_core;
extern crate serde_bencode;

use std::fmt;
use std::fs::File;
use std::io::{Read, Write};
use std::net::Ipv4Addr;
use std::net::SocketAddrV4;

use byteorder::{ByteOrder, NetworkEndian};

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

    pub(crate) fn load(filename: &str) -> Result<Config, serde_bencode::Error> {
        let mut file = File::open(filename).unwrap();
        let mut config_data = vec![];
        file.read_to_end(&mut config_data).unwrap();
        serde_bencode::from_bytes::<Config>(&config_data)
    }

    pub(crate) fn write(&self, filename: &str) -> Result<(), serde_bencode::Error> {
        let config_data = serde_bencode::to_bytes(self)?;
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
