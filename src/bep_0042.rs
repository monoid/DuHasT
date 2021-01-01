/// Implements https://www.bittorrent.org/beps/bep_0042.html
use std::net::IpAddr;
use crate::dht::DhtId;
use rand::{CryptoRng, Rng};
use crc32c_hw;

pub(crate) fn get_crc(ip: IpAddr, r: u8) -> u32 {
    match ip {
        IpAddr::V4(v4) => {
            let mut masked: Vec<u8> = [0x03u8, 0x0f, 0x3f, 0xff].iter().zip(&v4.octets()).map(|(a, b)| a & b).collect();
            masked[0] |= r << 5;
            
            dbg!(crc32c_hw::compute(dbg!(&masked)))
        }
        IpAddr::V6(v6) => {
            let mut masked: Vec<u8> = [0x01u8, 0x03, 0x07, 0x0f, 0x1f, 0x3f, 0x7f, 0xff].iter().zip(&v6.octets()).map(|(a, b)| a & b).collect();
            masked[0] |= r << 5;
            crc32c_hw::compute(&masked)
        }
    }
}

pub(crate) fn gen_self_id<R: Rng + CryptoRng>(self_ip: IpAddr, rng: &mut R) -> DhtId {
    // We waste some bytes of random data, but this func is used rarely.
    let mut original = DhtId::new(rng);

    let crc = get_crc(self_ip, original.0[19]).to_be_bytes();

    original.0[0] = crc[0];
    original.0[1] = crc[1];
    original.0[2] = (crc[2] & 0xF8) | (original.0[2] & 0x07);

    original
}


#[cfg(test)]
mod test {
    use std::net::IpAddr;

    #[test]
    fn test_spec() {
        // 2797153130
        
        // Single test from the spec.
        let d = crate::dht::DhtId::from_str("5fbfbff10c5d6a4ec8a88e4c6ab4c28b95eee401").unwrap();
        let ip: IpAddr = [124, 31, 75, 21].into();
        
        let crc = super::get_crc(ip, d.0[19]);
        assert_eq!(crc >> 11, 0x05fbfbf >> 3);
        let crc = crc.to_be_bytes();
        assert_eq!([crc[0], crc[1], crc[2] & 0xF8],
                   [d.0[0], d.0[1], d.0[2] & 0xF8]);
    }
}
