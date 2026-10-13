// Who may open a connection, and for how long it may stay unauthenticated (docs/ANDROID_LINK.md, "Admission").
//
// The server accepts connections from the local network before it knows who is calling. Left alone, one noisy or
// hostile device could fill every slot with silent connections and keep the paired phone out. So:
//   * one source address may hold at most 2 connections that have not yet said a valid hello;
//   * strangers (addresses that never authenticated) share a pool of 4 such connections, so at least half of the
//     8 slots are always left for a phone the computer already knows;
//   * an address that authenticated before is remembered (in memory only, 8 addresses) and is not held to the
//     stranger pool;
//   * when the pool is getting full the TLS handshake and the hello get 3 s instead of 10 s, so a flood of silent
//     connections frees its slots fast;
//   * callers from the internet (away from home, directly: remote.rs) that never authenticated have a pool of their
//     own, of 2, and are always hurried: the internet's scanners can neither take the local network's places nor
//     hold one for long.
// Pure bookkeeping: no sockets, no clock, so every rule is a unit test.

use std::collections::{HashMap, VecDeque};
use std::net::IpAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub const MAX_CONNECTIONS: usize = 8;
/// Connections from one address that have not yet sent a valid hello.
pub const MAX_UNAUTH_PER_ADDRESS: usize = 2;
/// Unauthenticated connections from addresses that never authenticated, all together.
pub const MAX_UNAUTH_STRANGERS: usize = 4;
/// Addresses remembered as "authenticated before".
pub const KNOWN_ADDRESSES: usize = 8;
/// Unauthenticated connections from the internet, from addresses that never authenticated, all together.
pub const MAX_UNAUTH_OUTSIDE: usize = 2;
pub const PATIENT: Duration = Duration::from_secs(10);
pub const HURRIED: Duration = Duration::from_secs(3);

#[derive(Default)]
struct State {
    total: usize,
    strangers_unauth: usize,
    outside_unauth: usize,
    unauth: HashMap<IpAddr, usize>,
    known: VecDeque<IpAddr>,
}

#[derive(Default)]
pub struct Admission {
    state: Mutex<State>,
}

fn canonical(ip: IpAddr) -> IpAddr {
    ip.to_canonical()
}

impl Admission {
    pub fn new() -> Arc<Admission> {
        Arc::new(Admission::default())
    }

    /// A slot for this caller on the local network (or the relay), or None (the connection is closed unserved). The
    /// slot is given back when the ticket drops.
    pub fn admit(self: &Arc<Self>, ip: IpAddr) -> Option<Ticket> {
        self.admit_from(ip, false)
    }

    /// A slot for a caller from the internet (away from home, directly).
    pub fn admit_outside(self: &Arc<Self>, ip: IpAddr) -> Option<Ticket> {
        self.admit_from(ip, true)
    }

    fn admit_from(self: &Arc<Self>, ip: IpAddr, outside: bool) -> Option<Ticket> {
        let ip = canonical(ip);
        let mut s = self.state.lock().unwrap();
        if s.total >= MAX_CONNECTIONS {
            return None;
        }
        if s.unauth.get(&ip).copied().unwrap_or(0) >= MAX_UNAUTH_PER_ADDRESS {
            return None;
        }
        let known = s.known.contains(&ip);
        // A stranger of the local network counts in its pool, a stranger of the internet in the other.
        let (stranger, outsider) = (!known && !outside, !known && outside);
        if (stranger && s.strangers_unauth >= MAX_UNAUTH_STRANGERS) || (outsider && s.outside_unauth >= MAX_UNAUTH_OUTSIDE) {
            return None;
        }
        // Looked at before this caller is counted: "is the pool getting full already?"
        let hurried = outsider || s.total + 2 >= MAX_CONNECTIONS || s.strangers_unauth * 2 >= MAX_UNAUTH_STRANGERS;
        s.total += 1;
        *s.unauth.entry(ip).or_insert(0) += 1;
        if stranger {
            s.strangers_unauth += 1;
        }
        if outsider {
            s.outside_unauth += 1;
        }
        Some(Ticket {
            admission: self.clone(),
            ip,
            stranger,
            outsider,
            outside,
            unauthenticated: Mutex::new(true),
            wait: if hurried { HURRIED } else { PATIENT },
        })
    }

    #[cfg(test)]
    pub fn counts(&self) -> (usize, usize) {
        let s = self.state.lock().unwrap();
        (s.total, s.strangers_unauth)
    }

    #[cfg(test)]
    pub fn outside_count(&self) -> usize {
        self.state.lock().unwrap().outside_unauth
    }

    #[cfg(test)]
    pub fn knows(&self, ip: IpAddr) -> bool {
        self.state.lock().unwrap().known.contains(&canonical(ip))
    }
}

/// One accepted connection's slot.
pub struct Ticket {
    admission: Arc<Admission>,
    ip: IpAddr,
    /// Counted in the local network's stranger pool until it authenticates.
    stranger: bool,
    /// Counted in the internet's stranger pool until it authenticates.
    outsider: bool,
    /// From the internet (whether its address was known or not).
    outside: bool,
    unauthenticated: Mutex<bool>,
    wait: Duration,
}

impl Ticket {
    /// How long the TLS handshake and then the hello may take (shorter when the pool is nearly full).
    pub fn wait(&self) -> Duration {
        self.wait
    }

    /// The caller came from the internet, not the local network.
    pub fn outside(&self) -> bool {
        self.outside
    }

    /// The caller said a valid hello: it no longer counts against the unauthenticated limits and its address is
    /// remembered, so a flood from elsewhere cannot keep it out.
    pub fn authenticated(&self) {
        let mut flag = self.unauthenticated.lock().unwrap();
        if !*flag {
            return;
        }
        *flag = false;
        let mut s = self.admission.state.lock().unwrap();
        release_unauth(&mut s, self.ip, self.stranger, self.outsider);
        s.known.retain(|k| *k != self.ip);
        s.known.push_front(self.ip);
        s.known.truncate(KNOWN_ADDRESSES);
    }
}

fn release_unauth(s: &mut State, ip: IpAddr, stranger: bool, outsider: bool) {
    if let Some(n) = s.unauth.get_mut(&ip) {
        *n -= 1;
        if *n == 0 {
            s.unauth.remove(&ip);
        }
    }
    if stranger {
        s.strangers_unauth -= 1;
    }
    if outsider {
        s.outside_unauth -= 1;
    }
}

impl Drop for Ticket {
    fn drop(&mut self) {
        let mut s = self.admission.state.lock().unwrap();
        s.total -= 1;
        if *self.unauthenticated.lock().unwrap() {
            release_unauth(&mut s, self.ip, self.stranger, self.outsider);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    #[test]
    fn one_address_holds_at_most_two_unauthenticated_connections() {
        let a = Admission::new();
        let t1 = a.admit(ip("192.168.1.50")).unwrap();
        let _t2 = a.admit(ip("192.168.1.50")).unwrap();
        assert!(a.admit(ip("192.168.1.50")).is_none(), "the third from one address");
        assert!(a.admit(ip("192.168.1.51")).is_some(), "another address is not affected");
        drop(t1);
        assert!(a.admit(ip("192.168.1.50")).is_some(), "a closed connection frees its place");
    }

    #[test]
    fn the_same_address_written_two_ways_is_one_address() {
        let a = Admission::new();
        let _t1 = a.admit(ip("192.168.1.50")).unwrap();
        let _t2 = a.admit(ip("::ffff:192.168.1.50")).unwrap();
        assert!(a.admit(ip("192.168.1.50")).is_none());
    }

    #[test]
    fn strangers_share_a_pool_smaller_than_the_whole() {
        let a = Admission::new();
        let mut held = Vec::new();
        for n in 0..MAX_UNAUTH_STRANGERS / MAX_UNAUTH_PER_ADDRESS {
            for _ in 0..MAX_UNAUTH_PER_ADDRESS {
                held.push(a.admit(ip(&format!("10.0.0.{}", n + 1))).unwrap());
            }
        }
        assert_eq!(held.len(), MAX_UNAUTH_STRANGERS);
        assert!(a.admit(ip("10.0.0.99")).is_none(), "the stranger pool is full");
        assert!(held.len() < MAX_CONNECTIONS, "the pool leaves slots for known phones");
    }

    #[test]
    fn a_flood_from_many_addresses_cannot_lock_out_the_paired_phone() {
        let a = Admission::new();
        // The phone connected once before: its address is known.
        a.admit(ip("192.168.1.20")).unwrap().authenticated();
        assert!(a.knows(ip("192.168.1.20")));
        // A flood fills everything strangers may take.
        let mut flood = Vec::new();
        for n in 0..50 {
            if let Some(t) = a.admit(ip(&format!("192.168.1.{}", 100 + n))) {
                flood.push(t);
            }
        }
        assert_eq!(flood.len(), MAX_UNAUTH_STRANGERS, "the flood is capped");
        // The paired phone still gets in, and keeps getting in after it reconnects.
        let phone = a.admit(ip("192.168.1.20")).expect("the paired phone is not locked out");
        phone.authenticated();
        let again = a.admit(ip("192.168.1.20")).expect("and can reconnect while the flood goes on");
        again.authenticated();
        let (total, strangers) = a.counts();
        assert_eq!(strangers, MAX_UNAUTH_STRANGERS);
        assert!(total <= MAX_CONNECTIONS);
    }

    #[test]
    fn a_flood_from_one_address_leaves_every_other_address_alone() {
        let a = Admission::new();
        let _flood: Vec<_> = (0..20).filter_map(|_| a.admit(ip("192.168.1.66"))).collect();
        let newcomer = a.admit(ip("192.168.1.20"));
        assert!(newcomer.is_some(), "a new phone, never seen before, still connects");
        assert_eq!(a.counts().1, MAX_UNAUTH_PER_ADDRESS + 1);
    }

    #[test]
    fn authenticating_frees_the_unauthenticated_places_and_is_counted_once() {
        let a = Admission::new();
        let t = a.admit(ip("192.168.1.20")).unwrap();
        t.authenticated();
        t.authenticated();
        assert_eq!(a.counts(), (1, 0));
        // An authenticated connection no longer counts against the two-per-address limit.
        let _u1 = a.admit(ip("192.168.1.20")).unwrap();
        let _u2 = a.admit(ip("192.168.1.20")).unwrap();
        drop(t);
        assert_eq!(a.counts().0, 2);
    }

    #[test]
    fn the_places_come_back_when_tickets_drop_whatever_their_state() {
        let a = Admission::new();
        {
            let t1 = a.admit(ip("192.168.1.20")).unwrap();
            let _t2 = a.admit(ip("192.168.1.21")).unwrap();
            t1.authenticated();
        }
        assert_eq!(a.counts(), (0, 0));
        assert!(a.admit(ip("192.168.1.21")).is_some());
    }

    #[test]
    fn the_total_never_goes_over_eight_even_for_known_phones() {
        let a = Admission::new();
        let mut held = Vec::new();
        for n in 0..MAX_CONNECTIONS {
            let t = a.admit(ip(&format!("192.168.1.{}", 10 + n))).unwrap();
            t.authenticated();
            held.push(t);
        }
        assert!(a.admit(ip("192.168.1.10")).is_none());
        assert_eq!(a.counts().0, MAX_CONNECTIONS);
    }

    #[test]
    fn the_wait_is_shorter_when_the_pool_is_filling_up() {
        let a = Admission::new();
        assert_eq!(a.admit(ip("192.168.1.1")).unwrap().wait(), PATIENT, "an empty server is patient");
        let mut held = Vec::new();
        for n in 0..2 {
            held.push(a.admit(ip(&format!("192.168.2.{n}"))).unwrap());
        }
        // Two strangers waiting is half of their pool: new callers are hurried.
        let t = a.admit(ip("192.168.3.1")).unwrap();
        assert_eq!(t.wait(), HURRIED);
        // And when the whole pool is nearly full, even a known phone gets the short wait.
        let b = Admission::new();
        let mut known = Vec::new();
        for n in 0..6 {
            let t = b.admit(ip(&format!("192.168.4.{n}"))).unwrap();
            t.authenticated();
            known.push(t);
        }
        assert_eq!(b.admit(ip("192.168.4.0")).unwrap().wait(), HURRIED);
    }

    #[test]
    fn the_internet_never_takes_the_local_networks_places() {
        let a = Admission::new();
        // Scanners from everywhere: two get a hurried place, the rest are closed unserved.
        let scanners: Vec<_> = (0..30).filter_map(|n| a.admit_outside(ip(&format!("203.0.113.{n}")))).collect();
        assert_eq!(scanners.len(), MAX_UNAUTH_OUTSIDE);
        assert!(scanners.iter().all(|t| t.wait() == HURRIED && t.outside()));
        assert_eq!(a.counts().1, 0, "the local network's stranger pool is untouched");
        // A phone at home that never connected before still gets in, with the full wait.
        let home = a.admit(ip("192.168.1.30")).expect("the local network keeps its places");
        assert!(!home.outside());
        // And the paired phone, away on its carrier's address, gets in once a scanner leaves.
        drop(scanners);
        let away = a.admit_outside(ip("198.51.100.40")).unwrap();
        away.authenticated();
        assert_eq!(a.outside_count(), 0, "an authenticated phone no longer counts against the internet's pool");
        let _flood: Vec<_> = (0..30).filter_map(|n| a.admit_outside(ip(&format!("203.0.113.{n}")))).collect();
        assert!(a.admit_outside(ip("198.51.100.40")).is_some(), "its address is known: the flood cannot keep it out");
    }

    #[test]
    fn a_place_from_the_internet_comes_back_whatever_way_it_ends() {
        let a = Admission::new();
        {
            let _t = a.admit_outside(ip("203.0.113.9")).unwrap();
            assert_eq!(a.outside_count(), 1);
        }
        assert_eq!((a.counts(), a.outside_count()), ((0, 0), 0));
        let t = a.admit_outside(ip("203.0.113.9")).unwrap();
        t.authenticated();
        drop(t);
        assert_eq!((a.counts(), a.outside_count()), ((0, 0), 0));
    }

    #[test]
    fn at_most_eight_addresses_are_remembered() {
        let a = Admission::new();
        for n in 0..(KNOWN_ADDRESSES + 3) {
            a.admit(ip(&format!("192.168.5.{n}"))).unwrap().authenticated();
        }
        assert!(!a.knows(ip("192.168.5.0")), "the oldest is forgotten");
        assert!(a.knows(ip(&format!("192.168.5.{}", KNOWN_ADDRESSES + 2))));
    }
}
