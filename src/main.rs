use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::UdpSocket;

mod dht;
mod query_queue;
mod bep_0042;

#[tokio::main]
async fn main() {
    // We do not bother with async read/write for config.
    let mut chacha = dht::init_chacha();

    let results = Arc::new(std::sync::Mutex::new(Vec::<(dht::DhtId, SocketAddr)>::new()));
    let results1 = results.clone();

    let local = tokio::net::lookup_host("192.168.0.26:4242")
        .await
        .unwrap()
        .next()
        .unwrap();

    let cfg = if Path::new(dht::DEFAULT_STATE_PATH).exists() {
        dht::Config::load(dht::DEFAULT_STATE_PATH).unwrap()
    } else {
        dht::Config::new(&mut chacha, local.ip())
    };
    cfg.write(dht::DEFAULT_STATE_PATH).unwrap();
    println!("{}", cfg.dht_id);

    let self_id1 = cfg.dht_id.clone();
    let self_id2 = cfg.dht_id.clone();

    let udp = Arc::new(UdpSocket::bind(local).await.unwrap());
    let udp1 = udp.clone();
    let udp2 = udp.clone();

    let qq = Arc::new(crate::query_queue::QueryQueue::new(Duration::from_secs(1)));
    let qq1 = qq.clone();
    let qq2 = qq.clone();

    let remote = tokio::net::lookup_host("192.168.0.26:7881")
        .await
        .unwrap()
        .next()
        .unwrap();
    let remote1 = remote.clone();
    let remote2 = remote.clone();

    tokio::task::spawn(async move {
        // form a message 1
        let msg1 = dht::Message::<()>::Q(dht::Query::FindNode(dht::FindNodeQuery {
            id: self_id1.clone(),
            // target: dht::DhtId::new(&mut chacha),
            target: self_id1.clone(),
        }));

        let qq11 = qq1.clone();
        let udp11 = udp1.clone();
        match qq1.send_message(udp1, remote1, msg1).await {
            Ok(resp) => {
                let msg = serde_bencoded::from_bytes_auto::<dht::Message<dht::FindNodeResponse>>(&resp)
                    .unwrap();
                eprintln!("{:?}", msg);
                if let dht::Message::R {
                    r: dht::FindNodeResponse { id: _, nodes },
                } = &msg
                {
                    {
                        let mut results = results1.lock().unwrap();
                        for node in nodes.iter() {
                            results.push((node.id.clone(), (node.ip, node.port).into()));
                        }
                    }
                    for node in nodes.iter() {
                        let results = results1.clone();

                        let msg1 =
                            dht::Message::<()>::Q(dht::Query::FindNode(dht::FindNodeQuery {
                                id: self_id1.clone(),
                                // target: dht::DhtId::new(&mut chacha),
                                target: self_id1.clone(),
                            }));
                        match qq11
                            .clone()
                            .send_message(udp11.clone(), (node.ip, node.port).into(), msg1)
                            .await
                        {
                            Ok(resp) => {
                                let msg = serde_bencoded::from_bytes::<
                                    dht::Message<dht::FindNodeResponse>,
                                >(&resp)
                                .unwrap();
                                eprintln!("{:?}", msg);
                                if let dht::Message::R {
                                    r: dht::FindNodeResponse { id: _, nodes },
                                } = &msg
                                {
                                    {
                                        let mut results = results.lock().unwrap();
                                        for node in nodes.iter() {
                                            results.push((
                                                node.id.clone(),
                                                (node.ip, node.port).into(),
                                            ));
                                        }
                                    }
                                }
                            }
                            Err(_) => eprintln!("ERROR"),
                        }
                    }
                }
            }
            Err(_) => eprintln!("ERROR"),
        }
    });

    tokio::task::spawn(async move {
        // form a message 2
        let msg2 = dht::Message::<()>::Q(dht::Query::GetPeers(dht::GetPeersQuery {
            id: self_id2,
            // target: dht::DhtId::new(&mut chacha),
            info_hash: dht::DhtId::from_str("4175EF7E2691D08AA4DC6B848E35DF84E8FE175B").unwrap(),
        }));

        match qq2.send_message(udp2, remote2, msg2).await {
            Ok(resp) => {
                let msg = serde_bencoded::from_bytes::<dht::Message<dht::FindNodeResponse>>(&resp)
                    .unwrap();
                eprintln!("{:?}", msg);
            }
            Err(_) => eprintln!("ERROR"),
        }
    });

    let sleep = tokio::time::sleep(Duration::from_secs(20));
    tokio::pin!(sleep);

    for _ in 0u8..200 {
        let mut data = vec![0u8; 1 << 16];

        tokio::select! {
            res = udp.recv_from(&mut data) => {
                let (len, from) = res.unwrap();
                data.resize(len, 0);

                let resp: dht::IncomingMessage = serde_bencoded::from_bytes(&data[..len]).unwrap();
                let id = query_queue::QueryId::from_ne_bytes([resp.t[0], resp.t[1]]);

                if (resp.y == "r") | (resp.y == "e") {
                    qq.got_reply(from, id, data);
                } else {
                    // TODO We should reply with some kind of error.
                    eprintln!("WARNING: ignoring yet message with y={}", resp.y);
                }
            }
            _ = &mut sleep => { break }
        }
    }

    let mut res = results.lock().unwrap();
    (&mut res[..]).sort();
    for (id, addr) in res.iter() {
        eprintln!("{:?} {:?}", id, addr);
    }
}
