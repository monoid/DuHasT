use std::path::Path;

mod dht;

fn main() {
    let cfg = if Path::new(dht::DEFAULT_STATE_PATH).exists() {
        dht::Config::load(dht::DEFAULT_STATE_PATH).unwrap()
    } else {
        let mut chacha = dht::init_chacha();
        dht::Config::new(&mut chacha)
    };
    cfg.write(dht::DEFAULT_STATE_PATH).unwrap();
    println!("{}", cfg.dht_id);
}
