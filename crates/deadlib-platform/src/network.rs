use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, UdpSocket};
use std::sync::{Arc, OnceLock};

static LOCAL_IP: OnceLock<Option<Arc<str>>> = OnceLock::new();

/// Returns the preferred local address used to reach the public network.
///
/// Connecting a UDP socket selects an interface without sending a packet.
pub fn local_ip() -> Option<Arc<str>> {
    LOCAL_IP
        .get_or_init(|| discover_local_ip().map(|ip| Arc::from(ip.to_string())))
        .clone()
}

fn discover_local_ip() -> Option<IpAddr> {
    route_local_ip(
        SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0),
        SocketAddr::new(IpAddr::V4(Ipv4Addr::new(1, 1, 1, 1)), 53),
    )
    .or_else(|| {
        route_local_ip(
            SocketAddr::new(IpAddr::V6(Ipv6Addr::UNSPECIFIED), 0),
            SocketAddr::new(
                IpAddr::V6(Ipv6Addr::new(0x2606, 0x4700, 0x4700, 0, 0, 0, 0, 0x1111)),
                53,
            ),
        )
    })
}

fn route_local_ip(bind: SocketAddr, target: SocketAddr) -> Option<IpAddr> {
    let socket = UdpSocket::bind(bind).ok()?;
    socket.connect(target).ok()?;
    Some(socket.local_addr().ok()?.ip())
}
