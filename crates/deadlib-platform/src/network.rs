use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, UdpSocket};
use std::sync::{Arc, OnceLock};

/// The process-preferred LAN address, resolved without sending network data.
///
/// This process-owned, thread-safe cache has one entry and lives until exit. It
/// is warmed on the first visible main-menu request, never evicted, and has no
/// gameplay miss path. A miss performs two bounded OS route lookups; steady
/// frames only read the `OnceLock` and clone an `Arc`.
pub fn local_ip() -> Option<Arc<str>> {
    static LOCAL_IP: OnceLock<Option<Arc<str>>> = OnceLock::new();
    LOCAL_IP
        .get_or_init(|| discover_local_ip().map(|ip| Arc::from(ip.to_string())))
        .clone()
}

fn discover_local_ip() -> Option<IpAddr> {
    route_local_ip(
        SocketAddr::from((Ipv4Addr::UNSPECIFIED, 0)),
        SocketAddr::from((Ipv4Addr::new(192, 0, 2, 1), 9)),
    )
    .or_else(|| {
        route_local_ip(
            SocketAddr::from((Ipv6Addr::UNSPECIFIED, 0)),
            SocketAddr::from((Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1), 9)),
        )
    })
}

fn route_local_ip(bind: SocketAddr, target: SocketAddr) -> Option<IpAddr> {
    let socket = UdpSocket::bind(bind).ok()?;
    socket.connect(target).ok()?;
    usable_ip(socket.local_addr().ok()?.ip())
}

fn usable_ip(ip: IpAddr) -> Option<IpAddr> {
    (!ip.is_loopback() && !ip.is_unspecified()).then_some(ip)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usable_ip_rejects_loopback_and_unspecified_addresses() {
        for ip in [
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            IpAddr::V4(Ipv4Addr::UNSPECIFIED),
            IpAddr::V6(Ipv6Addr::LOCALHOST),
            IpAddr::V6(Ipv6Addr::UNSPECIFIED),
        ] {
            assert_eq!(usable_ip(ip), None);
        }
        let lan = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 42));
        assert_eq!(usable_ip(lan), Some(lan));
    }
}
