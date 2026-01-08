use std::net::IpAddr;

#[derive(Clone, Debug)]
pub struct AddressInfo {
    pub ip: IpAddr,
    pub is_self_assigned: bool,
}

#[derive(Clone, Debug)]
pub struct InterfaceInfo {
    pub name: String,
    pub addresses: Vec<AddressInfo>,
    pub is_default: bool,
}

#[cfg(unix)]
mod unix_impl {
    use super::*;
    use nix::ifaddrs::getifaddrs;
    use std::collections::HashMap;

    pub fn get_network_interfaces_impl() -> Vec<InterfaceInfo> {
        let default_ip = get_default_ip();
        let mut iface_map: HashMap<String, Vec<AddressInfo>> = HashMap::new();

        if let Ok(addrs) = getifaddrs() {
            for ifaddr in addrs {
                if let Some(addr) = ifaddr.address {
                    if let Some(ip) = sockaddr_to_ip(&addr) {
                        if is_valid_address(&ip) {
                            let addr_info = AddressInfo {
                                ip,
                                is_self_assigned: is_self_assigned(&ip),
                            };
                            iface_map
                                .entry(ifaddr.interface_name)
                                .or_default()
                                .push(addr_info);
                        }
                    }
                }
            }
        }

        let mut result: Vec<InterfaceInfo> = iface_map
            .into_iter()
            .map(|(name, addresses)| {
                let is_default = default_ip
                    .as_ref()
                    .map(|d| addresses.iter().any(|a| &a.ip == d))
                    .unwrap_or(false);
                InterfaceInfo {
                    name,
                    addresses,
                    is_default,
                }
            })
            .collect();

        result.sort_by(|a, b| b.is_default.cmp(&a.is_default).then(a.name.cmp(&b.name)));
        result
    }

    fn sockaddr_to_ip(addr: &nix::sys::socket::SockaddrStorage) -> Option<IpAddr> {
        if let Some(v4) = addr.as_sockaddr_in() {
            Some(IpAddr::V4(std::net::Ipv4Addr::from(v4.ip())))
        } else if let Some(v6) = addr.as_sockaddr_in6() {
            Some(IpAddr::V6(v6.ip()))
        } else {
            None
        }
    }

    fn get_default_ip() -> Option<IpAddr> {
        use std::net::UdpSocket;
        let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
        socket.connect("8.8.8.8:80").ok()?;
        socket.local_addr().ok().map(|a| a.ip())
    }
}

#[cfg(windows)]
mod windows_impl {
    use super::*;
    use std::collections::HashMap;
    use std::net::{Ipv4Addr, Ipv6Addr};
    use windows::Win32::Foundation::ERROR_BUFFER_OVERFLOW;
    use windows::Win32::NetworkManagement::IpHelper::{
        GetAdaptersAddresses, GAA_FLAG_SKIP_ANYCAST, GAA_FLAG_SKIP_DNS_SERVER,
        GAA_FLAG_SKIP_MULTICAST, IP_ADAPTER_ADDRESSES_LH,
    };
    use windows::Win32::Networking::WinSock::{
        AF_INET, AF_INET6, AF_UNSPEC, SOCKADDR_IN, SOCKADDR_IN6,
    };

    pub fn get_network_interfaces_impl() -> Vec<InterfaceInfo> {
        let default_ip = get_default_ip();
        let mut iface_map: HashMap<String, Vec<AddressInfo>> = HashMap::new();

        let flags = GAA_FLAG_SKIP_ANYCAST | GAA_FLAG_SKIP_MULTICAST | GAA_FLAG_SKIP_DNS_SERVER;
        let mut buf_len: u32 = 0;

        unsafe {
            let result = GetAdaptersAddresses(AF_UNSPEC.0 as u32, flags, None, None, &mut buf_len);
            if result != ERROR_BUFFER_OVERFLOW.0 {
                return Vec::new();
            }

            let mut buffer: Vec<u8> = vec![0; buf_len as usize];
            let adapter_addresses = buffer.as_mut_ptr() as *mut IP_ADAPTER_ADDRESSES_LH;

            let result = GetAdaptersAddresses(
                AF_UNSPEC.0 as u32,
                flags,
                None,
                Some(adapter_addresses),
                &mut buf_len,
            );

            if result != 0 {
                return Vec::new();
            }

            let mut adapter = adapter_addresses;
            while !adapter.is_null() {
                let name = (*adapter)
                    .FriendlyName
                    .to_string()
                    .unwrap_or_else(|_| "Unknown".to_string());

                let mut unicast = (*adapter).FirstUnicastAddress;
                while !unicast.is_null() {
                    let sockaddr = (*unicast).Address.lpSockaddr;
                    if !sockaddr.is_null() {
                        let family = (*sockaddr).sa_family;
                        let ip = if family == AF_INET {
                            let sockaddr_in = sockaddr as *const SOCKADDR_IN;
                            let addr = (*sockaddr_in).sin_addr.S_un.S_addr.to_ne_bytes();
                            Some(IpAddr::V4(Ipv4Addr::new(addr[0], addr[1], addr[2], addr[3])))
                        } else if family == AF_INET6 {
                            let sockaddr_in6 = sockaddr as *const SOCKADDR_IN6;
                            let addr = (*sockaddr_in6).sin6_addr.u.Byte;
                            Some(IpAddr::V6(Ipv6Addr::from(addr)))
                        } else {
                            None
                        };

                        if let Some(ip) = ip {
                            if is_valid_address(&ip) {
                                let addr_info = AddressInfo {
                                    ip,
                                    is_self_assigned: is_self_assigned(&ip),
                                };
                                iface_map.entry(name.clone()).or_default().push(addr_info);
                            }
                        }
                    }
                    unicast = (*unicast).Next;
                }
                adapter = (*adapter).Next;
            }
        }

        let mut result: Vec<InterfaceInfo> = iface_map
            .into_iter()
            .map(|(name, addresses)| {
                let is_default = default_ip
                    .as_ref()
                    .map(|d| addresses.iter().any(|a| &a.ip == d))
                    .unwrap_or(false);
                InterfaceInfo {
                    name,
                    addresses,
                    is_default,
                }
            })
            .collect();

        result.sort_by(|a, b| b.is_default.cmp(&a.is_default).then(a.name.cmp(&b.name)));
        result
    }

    fn get_default_ip() -> Option<IpAddr> {
        use std::net::UdpSocket;
        let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
        socket.connect("8.8.8.8:80").ok()?;
        socket.local_addr().ok().map(|a| a.ip())
    }
}

pub fn get_network_interfaces() -> Vec<InterfaceInfo> {
    #[cfg(unix)]
    {
        unix_impl::get_network_interfaces_impl()
    }
    #[cfg(windows)]
    {
        windows_impl::get_network_interfaces_impl()
    }
}

fn is_valid_address(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => !v4.is_loopback() && !v4.is_link_local(),
        IpAddr::V6(v6) => !v6.is_loopback() && !is_link_local_v6(v6),
    }
}

fn is_self_assigned(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let octets = v4.octets();
            octets[0] == 169 && octets[1] == 254
        }
        IpAddr::V6(_) => false,
    }
}

fn is_link_local_v6(ip: &std::net::Ipv6Addr) -> bool {
    let segments = ip.segments();
    (segments[0] & 0xffc0) == 0xfe80
}
