//! The local DX cluster (contract sections 10 and 11): a read-only, DXSpider-style line service
//! that logging software (N1MM Logger+'s Telnet window and the like) connects to, fed by the
//! spots stream ([`crate::downstream`]).
//!
//! [`line`] formats a spot as the one line a DX cluster sends; [`server`] listens, logs clients
//! in and hands every spot line to each of them. Nothing a cluster client sends becomes a
//! contact, a spot or a request.

pub mod line;
pub mod server;

use std::net::{IpAddr, Ipv4Addr};

/// This computer's address on its network, for "Other computers on your network use ...": the
/// first private IPv4 address of an interface, else the first other non-loopback IPv4 one.
pub fn lan_address() -> Option<String> {
    let addrs = match if_addrs::get_if_addrs() {
        Ok(a) => a,
        Err(e) => {
            tracing::debug!(error = %e, "couldn't read the interface addresses");
            return None;
        }
    };
    let v4: Vec<Ipv4Addr> = addrs
        .iter()
        .filter_map(|i| match i.ip() {
            IpAddr::V4(ip) if !ip.is_loopback() && !ip.is_link_local() && !ip.is_unspecified() => {
                Some(ip)
            }
            _ => None,
        })
        .collect();
    v4.iter()
        .find(|ip| ip.is_private())
        .or_else(|| v4.first())
        .map(ToString::to_string)
}
