use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hasher};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::time::Duration;

const TYPE_SRV: u16 = 33;
const CLASS_IN: u16 = 1;
const MAX_SERVERS: usize = 3;
const TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Srv {
    pub priority: u16,
    pub weight: u16,
    pub port: u16,
    pub target: String,
}

pub fn build_query(id: u16, name: &str) -> Option<Vec<u8>> {
    let mut q = Vec::with_capacity(18 + name.len());
    q.extend_from_slice(&id.to_be_bytes());
    q.extend_from_slice(&[0x01, 0x00, 0, 1, 0, 0, 0, 0, 0, 0]);
    for label in name.trim_end_matches('.').split('.') {
        if label.is_empty() || label.len() > 63 {
            return None;
        }
        q.push(label.len() as u8);
        q.extend_from_slice(label.as_bytes());
    }
    q.push(0);
    q.extend_from_slice(&TYPE_SRV.to_be_bytes());
    q.extend_from_slice(&CLASS_IN.to_be_bytes());
    Some(q)
}

fn u16_at(m: &[u8], i: usize) -> Option<u16> {
    Some(u16::from_be_bytes([*m.get(i)?, *m.get(i + 1)?]))
}

/// Returns the offset just past the name starting at `i`.
fn skip_name(m: &[u8], mut i: usize) -> Option<usize> {
    loop {
        let b = *m.get(i)?;
        match b {
            0 => return Some(i + 1),
            b if b & 0xc0 == 0xc0 => return Some(i + 2),
            b if b & 0xc0 == 0 => i += 1 + usize::from(b),
            _ => return None,
        }
    }
}

fn read_name(m: &[u8], mut i: usize) -> Option<String> {
    let mut name = String::new();
    let mut jumps = 0;
    loop {
        let b = *m.get(i)?;
        match b {
            0 => return Some(name),
            b if b & 0xc0 == 0xc0 => {
                jumps += 1;
                if jumps > 16 {
                    return None;
                }
                i = usize::from(u16_at(m, i)? & 0x3fff);
            }
            b if b & 0xc0 == 0 => {
                let label = m.get(i + 1..i + 1 + usize::from(b))?;
                if !name.is_empty() {
                    name.push('.');
                }
                name.push_str(std::str::from_utf8(label).ok()?);
                i += 1 + usize::from(b);
            }
            _ => return None,
        }
    }
}

pub fn parse_response(id: u16, m: &[u8]) -> Option<Vec<Srv>> {
    if u16_at(m, 0)? != id || m[2] & 0x80 == 0 || m[3] & 0x0f != 0 {
        return None;
    }
    let (qd, an) = (u16_at(m, 4)?, u16_at(m, 6)?);
    let mut i = 12;
    for _ in 0..qd {
        i = skip_name(m, i)? + 4;
    }
    let mut out = Vec::new();
    for _ in 0..an {
        i = skip_name(m, i)?;
        let (ty, rdlen) = (u16_at(m, i)?, usize::from(u16_at(m, i + 8)?));
        let rdata = i + 10;
        if ty == TYPE_SRV && rdlen >= 7 {
            out.push(Srv {
                priority: u16_at(m, rdata)?,
                weight: u16_at(m, rdata + 2)?,
                port: u16_at(m, rdata + 4)?,
                target: read_name(m, rdata + 6)?,
            });
        }
        i = rdata + rdlen;
    }
    Some(out)
}

/// Lowest priority wins, then highest weight; a lone "." target means the service is absent.
pub fn pick(records: Vec<Srv>) -> Option<Srv> {
    records
        .into_iter()
        .filter(|r| !r.target.is_empty())
        .min_by_key(|r| (r.priority, std::cmp::Reverse(r.weight)))
}

pub fn nameservers() -> Vec<SocketAddr> {
    let mut v: Vec<SocketAddr> = std::fs::read_to_string("/etc/resolv.conf")
        .unwrap_or_default()
        .lines()
        .filter_map(|l| {
            let ip: IpAddr = l.trim().strip_prefix("nameserver")?.trim().parse().ok()?;
            Some(SocketAddr::new(ip, 53))
        })
        .collect();
    if v.is_empty() {
        v.push(SocketAddr::new(Ipv4Addr::new(1, 1, 1, 1).into(), 53));
        v.push(SocketAddr::new(Ipv4Addr::new(8, 8, 8, 8).into(), 53));
    }
    v.truncate(MAX_SERVERS);
    v
}

fn ask(server: SocketAddr, query: &[u8], id: u16) -> Option<Vec<Srv>> {
    let bind: SocketAddr = if server.is_ipv4() {
        (Ipv4Addr::UNSPECIFIED, 0).into()
    } else {
        (std::net::Ipv6Addr::UNSPECIFIED, 0).into()
    };
    let sock = UdpSocket::bind(bind).ok()?;
    sock.set_read_timeout(Some(TIMEOUT)).ok()?;
    sock.connect(server).ok()?;
    sock.send(query).ok()?;
    let mut buf = [0u8; 1500];
    let n = sock.recv(&mut buf).ok()?;
    parse_response(id, &buf[..n])
}

pub fn lookup_srv(name: &str, servers: &[SocketAddr]) -> Option<Srv> {
    let id = RandomState::new().build_hasher().finish() as u16;
    let query = build_query(id, name)?;
    servers
        .iter()
        .find_map(|&s| ask(s, &query, id))
        .and_then(pick)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn response(id: u16, records: &[(u16, u16, u16, &str)]) -> Vec<u8> {
        let mut m = id.to_be_bytes().to_vec();
        m.extend_from_slice(&[0x81, 0x80, 0, 1]);
        m.extend_from_slice(&(records.len() as u16).to_be_bytes());
        m.extend_from_slice(&[0, 0, 0, 0]);
        let q = build_query(id, "_minecraft._tcp.example.com").unwrap();
        m.extend_from_slice(&q[12..]);
        for &(prio, weight, port, target) in records {
            m.extend_from_slice(&[0xc0, 12]);
            m.extend_from_slice(&TYPE_SRV.to_be_bytes());
            m.extend_from_slice(&CLASS_IN.to_be_bytes());
            m.extend_from_slice(&[0, 0, 0, 60]);
            let mut rdata = Vec::new();
            for v in [prio, weight, port] {
                rdata.extend_from_slice(&v.to_be_bytes());
            }
            for label in target.split('.').filter(|l| !l.is_empty()) {
                rdata.push(label.len() as u8);
                rdata.extend_from_slice(label.as_bytes());
            }
            rdata.push(0);
            m.extend_from_slice(&(rdata.len() as u16).to_be_bytes());
            m.extend_from_slice(&rdata);
        }
        m
    }

    #[test]
    fn query_layout() {
        let q = build_query(0xabcd, "a.bc").unwrap();
        assert_eq!(
            q,
            [
                0xab, 0xcd, 1, 0, 0, 1, 0, 0, 0, 0, 0, 0, 1, b'a', 2, b'b', b'c', 0, 0, 33, 0, 1
            ]
        );
        assert!(build_query(1, "a..b").is_none());
    }

    #[test]
    fn parses_and_picks_best() {
        let m = response(
            7,
            &[
                (10, 1, 25570, "slow.example.com"),
                (5, 3, 25580, "fast.example.com"),
            ],
        );
        let recs = parse_response(7, &m).unwrap();
        assert_eq!(recs.len(), 2);
        let best = pick(recs).unwrap();
        assert_eq!(
            (best.port, best.target.as_str()),
            (25580, "fast.example.com")
        );
    }

    #[test]
    fn rejects_wrong_id_and_garbage() {
        let m = response(7, &[(0, 0, 1, "x.y")]);
        assert!(parse_response(8, &m).is_none());
        assert!(parse_response(7, &m[..20]).is_none());
        assert!(parse_response(7, &[]).is_none());
    }

    #[test]
    fn root_target_means_absent() {
        let m = response(1, &[(0, 0, 0, ".")]);
        assert!(pick(parse_response(1, &m).unwrap()).is_none());
    }
}
