extern crate rand_core;
extern crate rand_chacha;
extern crate serde_bencode;

use std::fs::File;
use std::io::{Read, Write};
use std::fmt;

use rand::{CryptoRng, SeedableRng, Rng, RngCore};
use rand::rngs::OsRng;
use rand_chacha::ChaCha20Rng;


const DHT_ID_BYTE_SIZE: usize = 160/8;
pub(crate) const DEFAULT_STATE_PATH: &'static str = "duhast.state";

type KeyBuf = [u8; 160/8];

#[derive(Deserialize, Serialize)]
pub(crate) struct DhtId {
    buf: KeyBuf,
}

impl fmt::Display for DhtId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result  {
        for b in &self.buf {
            write!(f, "{:02x}", b)?;
        }
        Ok(())
    }
}

impl DhtId {
    pub(crate) fn new<R: Rng + CryptoRng>(rng: &mut R) -> Self {
        let mut buf: KeyBuf = Default::default();
        rng.fill(&mut buf);
        DhtId { buf }
    }
}

#[derive(Deserialize, Serialize)]
pub(crate) struct Config {
    pub(crate) dht_id: DhtId,
    peers: Vec<String>,  // String is a stub here.
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
