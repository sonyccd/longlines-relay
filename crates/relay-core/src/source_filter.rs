//! Local-only rule: accept a datagram only from a loopback address. Everything else is counted
//! and dropped unread; its contents are never logged.
//!
//! The listener binds every address, but a packet's source address is whatever the sender
//! wrote, so this computer's own network addresses are not accepted: another computer on the
//! network could put one of them on a forged packet. Operating systems drop loopback-sourced
//! packets that arrive from the network, so a loopback source means a program on this
//! computer. Loggers send to `127.0.0.1` (docs/loggers.md).

use std::net::IpAddr;
use std::sync::atomic::{AtomicU64, Ordering};

/// The filter and its drop counter. Shared between the listener and the feed card.
#[derive(Debug, Default)]
pub struct SourceFilter {
    dropped: AtomicU64,
}

impl SourceFilter {
    pub fn new() -> Self {
        Self::default()
    }

    /// True when a datagram from `source` may be read.
    pub fn allows(&self, source: IpAddr) -> bool {
        source.to_canonical().is_loopback()
    }

    /// Counts one dropped datagram.
    pub fn record_drop(&self) {
        self.dropped.fetch_add(1, Ordering::Relaxed);
    }

    /// Datagrams dropped since launch.
    pub fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, Ipv6Addr};

    #[test]
    fn loopback_is_allowed() {
        let filter = SourceFilter::new();
        assert!(filter.allows(IpAddr::V4(Ipv4Addr::LOCALHOST)));
        assert!(filter.allows(IpAddr::V4(Ipv4Addr::new(127, 0, 1, 1))));
        assert!(filter.allows(IpAddr::V6(Ipv6Addr::LOCALHOST)));
        assert!(filter.allows(IpAddr::V6(Ipv4Addr::LOCALHOST.to_ipv6_mapped())));
    }

    #[test]
    fn everything_else_is_refused_even_this_computers_own_addresses() {
        let filter = SourceFilter::new();
        // Whatever this computer's interfaces are, a forged packet can carry their addresses.
        if let Ok(interfaces) = if_addrs::get_if_addrs() {
            for address in interfaces.iter().map(|i| i.ip()) {
                assert_eq!(filter.allows(address), address.is_loopback(), "{address}");
            }
        }
        assert!(!filter.allows(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 20))));
        assert!(!filter.allows(IpAddr::V6(Ipv4Addr::new(192, 168, 1, 20).to_ipv6_mapped())));
        assert!(!filter.allows("fe80::1234".parse().unwrap()));
        assert!(!filter.allows("2001:db8::1".parse().unwrap()));
        assert!(!filter.allows(IpAddr::V4(Ipv4Addr::UNSPECIFIED)));
        assert!(!filter.allows(IpAddr::V4(Ipv4Addr::BROADCAST)));
    }

    #[test]
    fn drops_are_counted() {
        let filter = SourceFilter::new();
        assert_eq!(filter.dropped(), 0);
        filter.record_drop();
        filter.record_drop();
        assert_eq!(filter.dropped(), 2);
    }
}
