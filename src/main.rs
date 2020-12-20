#[macro_use]
extern crate serde_derive;

use std::path::Path;

mod dht;

fn main() {
    let mut chacha = dht::init_chacha();
    let cfg = if Path::new(dht::DEFAULT_STATE_PATH).exists() {
        dht::Config::load(dht::DEFAULT_STATE_PATH).unwrap()
    } else {
        dht::Config::new(&mut chacha)
    };
    cfg.write(dht::DEFAULT_STATE_PATH).unwrap();
    println!("{}", cfg.dht_id);
}
