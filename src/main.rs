use std::path::Path;
use tokio::net::UdpSocket;
use std::borrow::Cow;

mod dht;

#[tokio::main]
async fn main() {
    // We do not bother with async read/write for config.
    let mut chacha = dht::init_chacha();
    let cfg = if Path::new(dht::DEFAULT_STATE_PATH).exists() {
        dht::Config::load(dht::DEFAULT_STATE_PATH).unwrap()
    } else {
        dht::Config::new(&mut chacha)
    };
    cfg.write(dht::DEFAULT_STATE_PATH).unwrap();
    println!("{}", cfg.dht_id);

    // // Test message to the local client.  Just to make sure it works!
    // const LOCAL_PORT: u16 = 4242;
    // const REMOTE_PORT: u16 = 7881;
    // const THE_IP: &str = "localhost";

    let mut msg_id: u16 = 0;

    // form a message
    let msg = dht::OutgoingMessage::<()> {
        t: Cow::Owned(Vec::from(msg_id.to_be_bytes())),
        msg: dht::Message::Q(dht::Query::FindNode(
            dht::FindNodeQuery {
                id: cfg.dht_id.clone(),
                // target: dht::DhtId::new(&mut chacha),
                target: cfg.dht_id.clone(),
            }
        ))
    };

    let data = serde_bencoded::to_vec(&msg).unwrap();

    let udp = UdpSocket::bind("192.168.0.26:4242").await.unwrap();

    udp.send_to(&data, "192.168.0.26:7881").await.unwrap();

    let mut data = vec![0u8; 1 << 16];
    let (len, _) = udp.recv_from(&mut data).await.unwrap();

    let resp: dht::IncomingMessage = serde_bencoded::from_bytes(&data[..len]).unwrap();

    if (resp.y == "r") | (resp.y == "e") {
        let msg: dht::Message<dht::FindNodeResponse> = serde_bencoded::from_bytes(&data[..len]).unwrap();
        eprintln!("{:?}", msg);
    }

    msg_id += 1;

    // form a message
    let msg = dht::OutgoingMessage::<()> {
        t: Cow::Owned(Vec::from(msg_id.to_be_bytes())),
        msg: dht::Message::Q(dht::Query::GetPeers(
            dht::GetPeersQuery {
                id: cfg.dht_id.clone(),
                // target: dht::DhtId::new(&mut chacha),
                info_hash: dht::DhtId::from_str("e59f8fce7bfb0979afde1de86d6ab3b88f95167a").unwrap(),
            }
        ))
    };

    let data = serde_bencoded::to_vec(&msg).unwrap();

    udp.send_to(&data, "192.168.0.26:7881").await.unwrap();

    let mut data = vec![0u8; 1 << 16];
    let (len, _) = udp.recv_from(&mut data).await.unwrap();

    let resp: dht::IncomingMessage = serde_bencoded::from_bytes(&data[..len]).unwrap();

    if (resp.y == "r") | (resp.y == "e") {
        let msg: dht::Message<dht::GetPeersResponse> = serde_bencoded::from_bytes(&data[..len]).unwrap();
        eprintln!("{:?}", msg);
    }
}
