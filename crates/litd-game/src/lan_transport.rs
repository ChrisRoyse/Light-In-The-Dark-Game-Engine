//! #1787: the real-socket transport primitives for the LAN match flow.
//!
//! REAL sockets only -- no in-process channels, no mocks (#1783 / FSV house rule). The lobby +
//! lockstep channel is a `TcpStream` (reliable, ordered: see the transport justification in
//! `lan_protocol`); discovery is an optional `UdpSocket` broadcast. Everything here is thin I/O
//! wiring; the lobby rules and lockstep determinism live in their own modules.

use std::io::ErrorKind;
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::time::Duration;

use crate::lan_error::{LanError, io_err};
use crate::lan_protocol::{DiscoveryBeacon, LAN_DISCOVERY_PORT};

/// A decoded beacon plus the real UDP sender address. Discovery must preserve the
/// sender IP: the TCP host is on another machine, not loopback.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredHost {
    pub beacon: DiscoveryBeacon,
    pub source_addr: SocketAddr,
}

impl DiscoveredHost {
    pub fn tcp_addr(&self) -> SocketAddr {
        SocketAddr::new(self.source_addr.ip(), self.beacon.tcp_port)
    }
}

/// Bind a TCP listener on every IPv4 interface. Passing port 0 asks the OS for a free
/// port; the caller reads it back via [`TcpListener::local_addr`] and advertises it.
pub fn bind_host(port: u16) -> Result<TcpListener, LanError> {
    let addr = SocketAddr::from((Ipv4Addr::UNSPECIFIED, port));
    TcpListener::bind(addr).map_err(|error| io_err("host.bind", error))
}

/// Accept exactly one joiner (M1 is 1v1). Nagle off so single small frames flush immediately --
/// lockstep latency is dominated by per-frame round trips, not throughput.
pub fn accept_one(listener: &TcpListener) -> Result<TcpStream, LanError> {
    let (stream, _peer) = listener
        .accept()
        .map_err(|error| io_err("host.accept", error))?;
    stream
        .set_nodelay(true)
        .map_err(|error| io_err("host.nodelay", error))?;
    Ok(stream)
}

/// Connect a joiner to a host `ip:port`.
pub fn connect(addr: &str) -> Result<TcpStream, LanError> {
    let stream = TcpStream::connect(addr).map_err(|error| io_err("join.connect", error))?;
    stream
        .set_nodelay(true)
        .map_err(|error| io_err("join.nodelay", error))?;
    Ok(stream)
}

/// Arm the read timeout used for stall detection. A read that returns nothing within
/// `grace_ms` (plus a small slack so a legitimately resuming peer is never falsely kicked)
/// surfaces as a timeout the lockstep loop converts into a typed [`LanError::StallTimeout`].
pub fn arm_stall_timeout(stream: &TcpStream, grace_ms: u64) -> Result<(), LanError> {
    let slack = 500;
    stream
        .set_read_timeout(Some(Duration::from_millis(grace_ms.saturating_add(slack))))
        .map_err(|error| io_err("stall.timeout", error))
}

/// Broadcast a discovery beacon once on the LAN discovery port. Best-effort UDP -- a joiner
/// that misses it simply re-listens; the reliable join still happens over TCP.
pub fn broadcast_beacon(beacon: &DiscoveryBeacon) -> Result<(), LanError> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))
        .map_err(|error| io_err("discovery.bind", error))?;
    socket
        .set_broadcast(true)
        .map_err(|error| io_err("discovery.set_broadcast", error))?;
    let payload = beacon.encode()?;
    let target = SocketAddr::from((Ipv4Addr::BROADCAST, LAN_DISCOVERY_PORT));
    socket
        .send_to(&payload, target)
        .map_err(|error| io_err("discovery.send", error))?;
    Ok(())
}

/// Listen for a discovery beacon for up to `timeout`, retaining the UDP sender so
/// the joiner connects to the actual host instead of its own loopback interface.
pub fn listen_for_beacon(timeout: Duration) -> Result<Option<DiscoveredHost>, LanError> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, LAN_DISCOVERY_PORT))
        .map_err(|error| io_err("discovery.listen_bind", error))?;
    socket
        .set_read_timeout(Some(timeout))
        .map_err(|error| io_err("discovery.listen_timeout", error))?;
    let mut buffer = [0u8; 2048];
    match socket.recv_from(&mut buffer) {
        Ok((len, source_addr)) => Ok(Some(DiscoveredHost {
            beacon: DiscoveryBeacon::decode(&buffer[..len])?,
            source_addr,
        })),
        Err(error)
            if error.kind() == ErrorKind::WouldBlock || error.kind() == ErrorKind::TimedOut =>
        {
            Ok(None)
        }
        Err(error) => Err(io_err("discovery.recv", error)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_bind_listens_on_all_ipv4_interfaces() {
        let listener = bind_host(0).unwrap();
        let addr = listener.local_addr().unwrap();
        assert!(addr.ip().is_unspecified());
        assert_ne!(addr.port(), 0);
    }

    #[test]
    fn discovered_tcp_address_uses_sender_ip_and_advertised_port() {
        let discovered = DiscoveredHost {
            beacon: DiscoveryBeacon {
                protocol_version: crate::lan_protocol::LAN_PROTOCOL_VERSION,
                share_id: "physical-host".to_string(),
                tcp_port: 47811,
                datasets_fingerprint: "abc123".to_string(),
            },
            source_addr: "192.0.2.44:53000".parse().unwrap(),
        };
        assert_eq!(
            discovered.tcp_addr(),
            "192.0.2.44:47811".parse::<SocketAddr>().unwrap()
        );
    }

    #[test]
    fn connect_refused_is_typed_io() {
        // Nothing is listening on this loopback port; the error must be the typed LAN io code.
        let error = connect("127.0.0.1:9").unwrap_err();
        assert_eq!(error.code(), "LITD_LAN_IO");
    }
}
