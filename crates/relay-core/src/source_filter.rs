//! Local-only rule: accept a datagram only from loopback or one of this machine's own interface
//! addresses. Everything else is counted and dropped unread; its contents are never logged.

use std::collections::HashSet;
use std::net::IpAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// How long the interface address list is trusted before it is read again.
pub const REFRESH_INTERVAL: Duration = Duration::from_secs(60);

/// Where the interface addresses come from.
#[derive(Debug)]
enum AddressSource {
    /// This machine's interfaces, re-read every [`REFRESH_INTERVAL`].
    System,
    /// A fixed set, for tests.
    Fixed,
}

#[derive(Debug)]
struct Addresses {
    set: HashSet<IpAddr>,
    read_at: Option<Instant>,
}

/// The filter and its drop counter. Shared between the listener and the feed card.
#[derive(Debug)]
pub struct SourceFilter {
    source: AddressSource,
    addresses: Mutex<Addresses>,
    dropped: AtomicU64,
}

impl SourceFilter {
    /// A filter over this machine's interface addresses.
    pub fn system() -> Self {
        Self {
            source: AddressSource::System,
            addresses: Mutex::new(Addresses {
                set: HashSet::new(),
                read_at: None,
            }),
            dropped: AtomicU64::new(0),
        }
    }

    /// A filter over a fixed address set (loopback is always allowed as well).
    pub fn with_addresses(addresses: impl IntoIterator<Item = IpAddr>) -> Self {
        Self {
            source: AddressSource::Fixed,
            addresses: Mutex::new(Addresses {
                set: addresses.into_iter().map(|a| a.to_canonical()).collect(),
                read_at: None,
            }),
            dropped: AtomicU64::new(0),
        }
    }

    /// True when a datagram from `source` may be read. Re-reads the interface list when stale.
    pub fn allows(&self, source: IpAddr) -> bool {
        let source = source.to_canonical();
        if source.is_loopback() {
            return true;
        }
        let mut addresses = self.addresses.lock().unwrap_or_else(|e| e.into_inner());
        if let AddressSource::System = self.source {
            let stale = addresses
                .read_at
                .is_none_or(|t| t.elapsed() >= REFRESH_INTERVAL);
            if stale {
                addresses.set = interface_addresses();
                addresses.read_at = Some(Instant::now());
            }
        }
        addresses.set.contains(&source)
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

fn interface_addresses() -> HashSet<IpAddr> {
    match if_addrs::get_if_addrs() {
        Ok(interfaces) => interfaces
            .into_iter()
            .map(|i| i.ip().to_canonical())
            .collect(),
        Err(e) => {
            tracing::warn!(error = %e, "couldn't read this computer's interface addresses");
            HashSet::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, Ipv6Addr};

    #[test]
    fn loopback_is_always_allowed() {
        let filter = SourceFilter::with_addresses([]);
        assert!(filter.allows(IpAddr::V4(Ipv4Addr::LOCALHOST)));
        assert!(filter.allows(IpAddr::V4(Ipv4Addr::new(127, 0, 1, 1))));
        assert!(filter.allows(IpAddr::V6(Ipv6Addr::LOCALHOST)));
        assert!(filter.allows(IpAddr::V6(Ipv4Addr::LOCALHOST.to_ipv6_mapped())));
    }

    #[test]
    fn own_addresses_are_allowed_and_others_are_not() {
        let own = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 20));
        let own6: IpAddr = "fe80::1234".parse().unwrap();
        let filter = SourceFilter::with_addresses([own, own6]);
        assert!(filter.allows(own));
        assert!(filter.allows(own6));
        assert!(filter.allows(IpAddr::V6(Ipv4Addr::new(192, 168, 1, 20).to_ipv6_mapped())));
        assert!(!filter.allows(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 21))));
        assert!(!filter.allows(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1))));
        assert!(!filter.allows("2001:db8::1".parse().unwrap()));
        assert!(!filter.allows(IpAddr::V4(Ipv4Addr::BROADCAST)));
    }

    #[test]
    fn drops_are_counted() {
        let filter = SourceFilter::with_addresses([]);
        assert_eq!(filter.dropped(), 0);
        filter.record_drop();
        filter.record_drop();
        assert_eq!(filter.dropped(), 2);
    }

    #[test]
    fn system_filter_reads_the_interfaces() {
        let filter = SourceFilter::system();
        assert!(filter.allows(IpAddr::V4(Ipv4Addr::LOCALHOST)));
        // A documentation address is never one of ours.
        assert!(!filter.allows(IpAddr::V4(Ipv4Addr::new(203, 0, 113, 7))));
        let first_read = filter.addresses.lock().unwrap().read_at;
        assert!(first_read.is_some());
        assert!(!filter.allows(IpAddr::V4(Ipv4Addr::new(203, 0, 113, 8))));
        assert_eq!(filter.addresses.lock().unwrap().read_at, first_read);
    }
}
