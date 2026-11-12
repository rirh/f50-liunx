//! The two kernel sockets the relay needs, opened with libc: a route-netlink socket subscribed to IPv6 address and
//! route changes (only a wakeup: what changed is read again with `ip`), and a raw ICMPv6 socket for one router
//! advertisement. Linux only; elsewhere (a host build) both are unsupported.
#![allow(unsafe_code)]

use std::io;

#[cfg(target_os = "linux")]
mod imp {
    use std::io;
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};

    use tokio::io::unix::AsyncFd;

    const RTMGRP_IPV6_IFADDR: u32 = 0x100;
    const RTMGRP_IPV6_ROUTE: u32 = 0x400;

    fn check(r: libc::c_int) -> io::Result<libc::c_int> {
        if r < 0 { Err(io::Error::last_os_error()) } else { Ok(r) }
    }

    fn socklen<T>() -> libc::socklen_t {
        libc::socklen_t::try_from(std::mem::size_of::<T>()).unwrap_or(0)
    }

    /// Wakes on every IPv6 address or route change the kernel announces.
    #[derive(Debug)]
    pub struct RouteEvents {
        fd: AsyncFd<OwnedFd>,
    }

    fn recv_once(fd: RawFd) -> io::Result<usize> {
        let mut buf = [0u8; 8192];
        // SAFETY: buf is a live, writable buffer of buf.len() bytes for the duration of the call.
        let n = unsafe { libc::recv(fd, buf.as_mut_ptr().cast(), buf.len(), 0) };
        if n < 0 { Err(io::Error::last_os_error()) } else { Ok(usize::try_from(n).unwrap_or(0)) }
    }

    impl RouteEvents {
        pub fn open() -> io::Result<RouteEvents> {
            // SAFETY: socket(2) with constant arguments; the descriptor is owned below.
            let raw = check(unsafe {
                libc::socket(
                    libc::AF_NETLINK,
                    libc::SOCK_RAW | libc::SOCK_CLOEXEC | libc::SOCK_NONBLOCK,
                    libc::NETLINK_ROUTE,
                )
            })?;
            // SAFETY: raw is a descriptor we just created and nothing else owns.
            let fd = unsafe { OwnedFd::from_raw_fd(raw) };
            // SAFETY: an all-zero sockaddr_nl is a valid value of that plain C struct.
            let mut sa: libc::sockaddr_nl = unsafe { std::mem::zeroed() };
            sa.nl_family = libc::sa_family_t::try_from(libc::AF_NETLINK).unwrap_or(16);
            sa.nl_groups = RTMGRP_IPV6_IFADDR | RTMGRP_IPV6_ROUTE;
            // SAFETY: sa is a valid sockaddr_nl and the length passed is its size.
            check(unsafe {
                libc::bind(fd.as_raw_fd(), std::ptr::from_ref(&sa).cast(), socklen::<libc::sockaddr_nl>())
            })?;
            // SAFETY: the OwnedFd is moved into the AsyncFd and owned by it for its whole life, so the descriptor stays
            // open and the same; nothing else holds it.
            let fd = unsafe { AsyncFd::register(fd) }?;
            Ok(RouteEvents { fd })
        }

        /// The next burst of announcements (all that are queued are read).
        pub async fn next(&self) -> io::Result<()> {
            loop {
                let mut g = self.fd.readable().await?;
                match g.try_io(|fd| recv_once(fd.get_ref().as_raw_fd())) {
                    Ok(Ok(_)) => {
                        while recv_once(self.fd.get_ref().as_raw_fd()).is_ok() {}
                        return Ok(());
                    }
                    // ENOBUFS: announcements were lost, which is a change too
                    Ok(Err(e)) if e.raw_os_error() == Some(libc::ENOBUFS) => return Ok(()),
                    Ok(Err(e)) => return Err(e),
                    Err(_would_block) => {}
                }
            }
        }
    }

    /// One ICMPv6 message to all nodes (ff02::1) on interface `ifindex`, hop limit 255 (an RA is only valid with it,
    /// RFC 4861 6.1.2). The kernel fills in the checksum of a raw ICMPv6 socket and picks the link-local source.
    pub fn send_all_nodes(ifindex: u32, pkt: &[u8]) -> io::Result<()> {
        // SAFETY: socket(2) with constant arguments; the descriptor is owned below.
        let raw =
            check(unsafe { libc::socket(libc::AF_INET6, libc::SOCK_RAW | libc::SOCK_CLOEXEC, libc::IPPROTO_ICMPV6) })?;
        // SAFETY: raw is a descriptor we just created and nothing else owns.
        let fd = unsafe { OwnedFd::from_raw_fd(raw) };
        let opt = |name: libc::c_int, v: libc::c_int| {
            // SAFETY: v lives for the call and the length is that of a c_int.
            check(unsafe {
                libc::setsockopt(
                    fd.as_raw_fd(),
                    libc::IPPROTO_IPV6,
                    name,
                    std::ptr::from_ref(&v).cast(),
                    socklen::<libc::c_int>(),
                )
            })
        };
        opt(libc::IPV6_MULTICAST_HOPS, 255)?;
        opt(libc::IPV6_MULTICAST_IF, libc::c_int::try_from(ifindex).unwrap_or(0))?;
        // SAFETY: an all-zero sockaddr_in6 is a valid value of that plain C struct.
        let mut to: libc::sockaddr_in6 = unsafe { std::mem::zeroed() };
        to.sin6_family = libc::sa_family_t::try_from(libc::AF_INET6).unwrap_or(10);
        to.sin6_addr.s6_addr = [0xff, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1];
        to.sin6_scope_id = ifindex;
        // SAFETY: pkt is a live buffer of pkt.len() bytes; to is a valid sockaddr_in6 of the length passed.
        let n = unsafe {
            libc::sendto(
                fd.as_raw_fd(),
                pkt.as_ptr().cast(),
                pkt.len(),
                0,
                std::ptr::from_ref(&to).cast(),
                socklen::<libc::sockaddr_in6>(),
            )
        };
        if n < 0 {
            return Err(io::Error::last_os_error());
        }
        if usize::try_from(n).ok() != Some(pkt.len()) {
            return Err(io::Error::other("short send"));
        }
        Ok(())
    }
}

#[cfg(not(target_os = "linux"))]
mod imp {
    use std::io;

    #[derive(Debug)]
    pub struct RouteEvents;

    impl RouteEvents {
        pub fn open() -> io::Result<RouteEvents> {
            Err(io::Error::new(io::ErrorKind::Unsupported, "route netlink is Linux only"))
        }
        pub async fn next(&self) -> io::Result<()> {
            std::future::pending().await
        }
    }

    pub fn send_all_nodes(_ifindex: u32, _pkt: &[u8]) -> io::Result<()> {
        Err(io::Error::new(io::ErrorKind::Unsupported, "raw ICMPv6 is Linux only"))
    }
}

pub(crate) use imp::{RouteEvents, send_all_nodes};

/// The next event, or never when there is no socket.
pub(crate) async fn next_event(ev: Option<&RouteEvents>) -> io::Result<()> {
    match ev {
        Some(e) => e.next().await,
        None => std::future::pending().await,
    }
}
