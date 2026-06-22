// 局域网 IP 助手：识别当前 IPv4 子网，扫描在线设备，并给出“疑似空闲”候选地址。
//
// 注意：无响应不能证明地址永久空闲。这里综合 ICMP、TCP 和系统邻居表，结果仍需结合
// DHCP 租约/保留范围判断；前端和 CLI 都必须保留这层提示。

use super::ping;
use futures::future;
use futures::stream::{self, StreamExt};
use network_interface::{Addr, NetworkInterface, NetworkInterfaceConfig};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::time::{Duration, Instant};
use tokio::net::TcpStream;
use tokio::process::Command;

const MAX_SCAN_HOSTS: usize = 1024;
const TCP_DISCOVERY_PORTS: [u16; 6] = [22, 80, 443, 445, 3389, 9100];

#[derive(Debug, Serialize, Clone)]
pub struct LanInterface {
    pub name: String,
    pub address: String,
    pub netmask: String,
    pub prefix: u8,
    pub cidr: String,
    pub broadcast: String,
    pub mac: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
pub struct LanInfo {
    pub interface_name: String,
    pub address: String,
    pub netmask: String,
    pub prefix: u8,
    pub cidr: String,
    pub suggested_cidr: String,
    pub broadcast: String,
    pub gateway: Option<String>,
    pub mac: Option<String>,
    pub interfaces: Vec<LanInterface>,
}

#[derive(Debug, Serialize, Clone)]
pub struct LanHost {
    pub ip: String,
    pub mac: Option<String>,
    pub methods: Vec<String>,
    pub open_ports: Vec<u16>,
    pub reserved_reason: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
pub struct ReservedAddress {
    pub ip: String,
    pub reason: String,
}

#[derive(Debug, Serialize, Clone)]
pub struct LanScanReport {
    pub info: LanInfo,
    pub cidr: String,
    pub scanned_hosts: usize,
    pub active_hosts: Vec<LanHost>,
    pub candidates: Vec<String>,
    pub reserved: Vec<ReservedAddress>,
    pub duration_ms: f64,
    pub warning: String,
}

#[tauri::command]
pub async fn lan_info() -> Result<LanInfo, String> {
    discover_lan_info(None).await
}

#[tauri::command]
pub async fn lan_scan(
    cidr: Option<String>,
    timeout_ms: u64,
    concurrency: usize,
    suggestion_count: usize,
    interface_address: Option<String>,
) -> Result<LanScanReport, String> {
    let start = Instant::now();
    let info = discover_lan_info(interface_address.as_deref()).await?;
    let local_network = Ipv4Network::parse(&info.cidr)?;
    let requested = cidr
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(&info.suggested_cidr);
    let network = Ipv4Network::parse(requested)?;

    if !local_network.contains(network.network) || !local_network.contains(network.broadcast) {
        return Err(format!(
            "扫描范围 {network} 不在当前直连子网 {} 内；第一版仅允许扫描本地子网",
            info.cidr
        ));
    }

    // 用户 CIDR 只是扫描范围；网络/广播地址必须按真实接口掩码判断。
    let targets = scan_targets(network, local_network)?;
    if targets.is_empty() {
        return Err(format!("扫描范围 {network} 内没有可探测的本机子网地址"));
    }

    let timeout_ms = timeout_ms.clamp(100, 5000);
    let concurrency = concurrency.clamp(1, 64);
    let suggestion_count = suggestion_count.clamp(1, 100);
    let timeout = Duration::from_millis(timeout_ms);

    let observations: Vec<ProbeObservation> = stream::iter(targets.iter().copied())
        .map(|ip| async move { probe_host(ip, timeout_ms, timeout).await })
        .buffer_unordered(concurrency)
        .collect()
        .await;

    // 主动探测会刷新 ARP/neighbor cache，因此在扫描完成后读取最有价值。
    let neighbors = read_neighbors().await;
    let local_ip = info.address.parse::<Ipv4Addr>().ok();
    let gateway = info
        .gateway
        .as_deref()
        .and_then(|value| value.parse::<Ipv4Addr>().ok());
    let mut occupied = HashSet::new();
    let mut active_hosts = Vec::new();

    for observation in observations {
        let mac = neighbors.get(&observation.ip).cloned();
        let is_local = local_ip == Some(observation.ip);
        let is_gateway = gateway == Some(observation.ip);
        if !observation.ping && !observation.tcp && mac.is_none() && !is_local && !is_gateway {
            continue;
        }

        let mut methods = Vec::new();
        if observation.ping {
            methods.push("icmp".to_string());
        }
        if observation.tcp {
            methods.push("tcp".to_string());
        }
        if mac.is_some() {
            methods.push("arp".to_string());
        }
        if is_local {
            methods.push("local".to_string());
        }
        let reserved_reason = if is_local {
            Some("本机地址".to_string())
        } else if is_gateway {
            Some("默认网关".to_string())
        } else {
            None
        };

        occupied.insert(observation.ip);
        active_hosts.push(LanHost {
            ip: observation.ip.to_string(),
            mac,
            methods,
            open_ports: observation.open_ports,
            reserved_reason,
        });
    }
    active_hosts.sort_by_key(|host| ipv4_sort_key(&host.ip));

    let mut reserved = Vec::new();
    if network.contains(local_network.network) {
        reserved.push(ReservedAddress {
            ip: local_network.network.to_string(),
            reason: "网络地址".to_string(),
        });
    }
    if network.contains(local_network.broadcast) {
        reserved.push(ReservedAddress {
            ip: local_network.broadcast.to_string(),
            reason: "广播地址".to_string(),
        });
    }
    if let Some(ip) = local_ip.filter(|ip| network.contains(*ip)) {
        reserved.push(ReservedAddress {
            ip: ip.to_string(),
            reason: "本机地址".to_string(),
        });
        occupied.insert(ip);
    }
    if let Some(ip) = gateway.filter(|ip| network.contains(*ip)) {
        reserved.push(ReservedAddress {
            ip: ip.to_string(),
            reason: "默认网关".to_string(),
        });
        occupied.insert(ip);
    }
    reserved.sort_by_key(|row| ipv4_sort_key(&row.ip));
    reserved.dedup_by(|left, right| left.ip == right.ip);

    let scanned_hosts = targets.len();
    let candidates = targets
        .into_iter()
        .filter(|ip| !occupied.contains(ip))
        .take(suggestion_count)
        .map(|ip| ip.to_string())
        .collect();

    Ok(LanScanReport {
        info,
        cidr: network.to_string(),
        scanned_hosts,
        active_hosts,
        candidates,
        reserved,
        duration_ms: start.elapsed().as_secs_f64() * 1000.0,
        warning:
            "候选地址仅表示本次 ICMP/TCP/ARP 探测未发现响应；配置前仍需核对 DHCP 租约与保留范围。"
                .to_string(),
    })
}

async fn discover_lan_info(interface_address: Option<&str>) -> Result<LanInfo, String> {
    let route = default_route().await;
    let system_interfaces =
        NetworkInterface::show().map_err(|error| format!("读取网络接口失败: {error}"))?;
    let mut interfaces = Vec::new();

    for interface in system_interfaces {
        if interface.internal {
            continue;
        }
        for address in &interface.addr {
            let Addr::V4(v4) = address else {
                continue;
            };
            if !is_usable_interface_ip(v4.ip) {
                continue;
            }
            let Some(netmask) = v4.netmask else {
                continue;
            };
            let prefix = prefix_from_netmask(netmask)?;
            let network = Ipv4Network::from_ip(v4.ip, prefix);
            interfaces.push(LanInterface {
                name: interface.name.clone(),
                address: v4.ip.to_string(),
                netmask: netmask.to_string(),
                prefix,
                cidr: network.to_string(),
                broadcast: network.broadcast.to_string(),
                mac: interface.mac_addr.clone().map(normalize_mac),
            });
        }
    }

    if interfaces.is_empty() {
        return Err("未找到可用的 IPv4 网络接口".to_string());
    }
    interfaces.sort_by_key(|interface| !is_private_ipv4(&interface.address));

    let selected_index = if let Some(address) = interface_address
        .map(str::trim)
        .filter(|address| !address.is_empty())
    {
        interfaces
            .iter()
            .position(|item| item.address == address)
            .ok_or_else(|| format!("所选网络接口 {address} 已不可用，请重新打开此页面后重试"))?
    } else {
        route
            .interface
            .as_deref()
            .and_then(|name| interfaces.iter().position(|item| item.name == name))
            .or_else(|| {
                route.local_ip.and_then(|ip| {
                    interfaces
                        .iter()
                        .position(|item| item.address == ip.to_string())
                })
            })
            .unwrap_or(0)
    };
    let selected = interfaces[selected_index].clone();
    let suggested_cidr = if selected.prefix < 22 {
        Ipv4Network::from_ip(selected.address.parse().unwrap(), 24).to_string()
    } else {
        selected.cidr.clone()
    };

    Ok(LanInfo {
        interface_name: selected.name.clone(),
        address: selected.address.clone(),
        netmask: selected.netmask.clone(),
        prefix: selected.prefix,
        cidr: selected.cidr.clone(),
        suggested_cidr,
        broadcast: selected.broadcast.clone(),
        gateway: route.gateway.map(|ip| ip.to_string()),
        mac: selected.mac.clone(),
        interfaces,
    })
}

#[derive(Debug)]
struct ProbeObservation {
    ip: Ipv4Addr,
    ping: bool,
    tcp: bool,
    open_ports: Vec<u16>,
}

async fn probe_host(ip: Ipv4Addr, timeout_ms: u64, timeout: Duration) -> ProbeObservation {
    let ping_future = async {
        tokio::time::timeout(
            timeout + Duration::from_secs(1),
            ping::ping_host_cli(ip.to_string(), 1, timeout_ms),
        )
        .await
        .ok()
        .and_then(Result::ok)
        .is_some_and(|rows| rows.iter().any(|row| row.success))
    };
    let tcp_future = tcp_presence(ip, timeout);
    let (ping, (tcp, open_ports)) = tokio::join!(ping_future, tcp_future);
    ProbeObservation {
        ip,
        ping,
        tcp,
        open_ports,
    }
}

async fn tcp_presence(ip: Ipv4Addr, timeout: Duration) -> (bool, Vec<u16>) {
    let attempts = TCP_DISCOVERY_PORTS.into_iter().map(|port| async move {
        let socket = SocketAddr::V4(SocketAddrV4::new(ip, port));
        (
            port,
            tokio::time::timeout(timeout, TcpStream::connect(socket)).await,
        )
    });
    let mut present = false;
    let mut open_ports = Vec::new();
    for (port, result) in future::join_all(attempts).await {
        match result {
            Ok(Ok(_)) => {
                present = true;
                open_ports.push(port);
            }
            Ok(Err(error)) if error.kind() == std::io::ErrorKind::ConnectionRefused => {
                // RST/拒绝连接仍能证明这个 IP 上有主机响应。
                present = true;
            }
            _ => {}
        }
    }
    (present, open_ports)
}

#[derive(Default)]
struct RouteHint {
    interface: Option<String>,
    gateway: Option<Ipv4Addr>,
    local_ip: Option<Ipv4Addr>,
}

#[cfg(target_os = "macos")]
async fn default_route() -> RouteHint {
    let output = Command::new("route")
        .args(["-n", "get", "default"])
        .output()
        .await;
    let Ok(output) = output else {
        return RouteHint::default();
    };
    let text = String::from_utf8_lossy(&output.stdout);
    let mut hint = RouteHint::default();
    for line in text.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        match key.trim() {
            "gateway" => hint.gateway = value.trim().parse().ok(),
            "interface" => hint.interface = Some(value.trim().to_string()),
            _ => {}
        }
    }
    hint
}

#[cfg(target_os = "linux")]
async fn default_route() -> RouteHint {
    let output = Command::new("ip")
        .args(["-4", "route", "show", "default"])
        .output()
        .await;
    let Ok(output) = output else {
        return RouteHint::default();
    };
    parse_linux_default_route(&String::from_utf8_lossy(&output.stdout))
}

#[cfg(target_os = "windows")]
async fn default_route() -> RouteHint {
    let output = windows_hidden_command("route")
        .args(["print", "-4"])
        .output()
        .await;
    let Ok(output) = output else {
        return RouteHint::default();
    };
    parse_windows_default_route(&String::from_utf8_lossy(&output.stdout))
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
async fn default_route() -> RouteHint {
    RouteHint::default()
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn parse_linux_default_route(text: &str) -> RouteHint {
    let tokens: Vec<&str> = text.split_whitespace().collect();
    let mut hint = RouteHint::default();
    for pair in tokens.windows(2) {
        match pair[0] {
            "via" => hint.gateway = pair[1].parse().ok(),
            "dev" => hint.interface = Some(pair[1].to_string()),
            "src" => hint.local_ip = pair[1].parse().ok(),
            _ => {}
        }
    }
    hint
}

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn parse_windows_default_route(text: &str) -> RouteHint {
    for line in text.lines() {
        let columns: Vec<&str> = line.split_whitespace().collect();
        if columns.len() >= 5 && columns[0] == "0.0.0.0" && columns[1] == "0.0.0.0" {
            return RouteHint {
                gateway: columns[2].parse().ok(),
                local_ip: columns[3].parse().ok(),
                interface: None,
            };
        }
    }
    RouteHint::default()
}

async fn read_neighbors() -> HashMap<Ipv4Addr, String> {
    #[cfg(target_os = "linux")]
    let output = Command::new("ip").args(["neigh", "show"]).output().await;
    #[cfg(target_os = "macos")]
    let output = Command::new("arp").arg("-an").output().await;
    #[cfg(target_os = "windows")]
    let output = windows_hidden_command("arp").arg("-a").output().await;
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    let output: Result<std::process::Output, std::io::Error> = Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "unsupported platform",
    ));

    output
        .ok()
        .map(|value| parse_neighbor_table(&String::from_utf8_lossy(&value.stdout)))
        .unwrap_or_default()
}

/// Windows GUI 构建调用控制台程序时必须禁止创建窗口，否则每次刷新网卡或扫描都会闪黑框。
#[cfg(target_os = "windows")]
fn windows_hidden_command(program: &str) -> Command {
    let mut command = Command::new(program);
    command.creation_flags(0x08000000); // CREATE_NO_WINDOW
    command
}

fn parse_neighbor_table(text: &str) -> HashMap<Ipv4Addr, String> {
    let mut rows = HashMap::new();
    for line in text.lines() {
        let tokens: Vec<&str> = line.split_whitespace().collect();
        let ip = tokens.iter().find_map(|token| {
            token
                .trim_matches(|ch| ch == '(' || ch == ')' || ch == ',')
                .parse::<Ipv4Addr>()
                .ok()
        });
        let mac = tokens.iter().find_map(|token| {
            let candidate = token
                .trim_matches(|ch: char| !ch.is_ascii_hexdigit() && ch != ':' && ch != '-')
                .replace('-', ":")
                .to_ascii_lowercase();
            is_mac_address(&candidate).then_some(candidate)
        });
        if let (Some(ip), Some(mac)) = (ip, mac) {
            if mac != "00:00:00:00:00:00" && mac != "ff:ff:ff:ff:ff:ff" {
                rows.insert(ip, mac);
            }
        }
    }
    rows
}

fn is_mac_address(value: &str) -> bool {
    let parts: Vec<&str> = value.split(':').collect();
    parts.len() == 6
        && parts
            .iter()
            .all(|part| part.len() == 2 && part.chars().all(|ch| ch.is_ascii_hexdigit()))
}

fn normalize_mac(value: String) -> String {
    value.replace('-', ":").to_ascii_lowercase()
}

fn prefix_from_netmask(netmask: Ipv4Addr) -> Result<u8, String> {
    let bits = u32::from(netmask);
    let prefix = bits.count_ones() as u8;
    let expected = if prefix == 0 {
        0
    } else {
        u32::MAX << (32 - prefix)
    };
    if bits != expected {
        return Err(format!("不连续的 IPv4 子网掩码: {netmask}"));
    }
    Ok(prefix)
}

fn is_usable_interface_ip(ip: Ipv4Addr) -> bool {
    !ip.is_loopback() && !ip.is_unspecified() && !ip.is_link_local() && !ip.is_multicast()
}

fn is_private_ipv4(value: &str) -> bool {
    value.parse::<Ipv4Addr>().is_ok_and(|ip| ip.is_private())
}

fn ipv4_sort_key(value: &str) -> u32 {
    value.parse::<Ipv4Addr>().map(u32::from).unwrap_or(u32::MAX)
}

fn scan_targets(range: Ipv4Network, local_network: Ipv4Network) -> Result<Vec<Ipv4Addr>, String> {
    Ok(range
        .addresses()?
        .into_iter()
        .filter(|ip| *ip != local_network.network && *ip != local_network.broadcast)
        .collect())
}

#[derive(Clone, Copy)]
struct Ipv4Network {
    network: Ipv4Addr,
    broadcast: Ipv4Addr,
    prefix: u8,
}

impl Ipv4Network {
    fn parse(value: &str) -> Result<Self, String> {
        let (ip, prefix) = value
            .split_once('/')
            .ok_or_else(|| format!("CIDR 格式错误: {value}"))?;
        let ip = ip
            .parse::<Ipv4Addr>()
            .map_err(|_| format!("非法 IPv4 地址: {ip}"))?;
        let prefix = prefix
            .parse::<u8>()
            .map_err(|_| format!("非法 CIDR 前缀: {prefix}"))?;
        if prefix > 32 {
            return Err(format!("CIDR 前缀必须在 0-32 之间: {prefix}"));
        }
        Ok(Self::from_ip(ip, prefix))
    }

    fn from_ip(ip: Ipv4Addr, prefix: u8) -> Self {
        let mask = if prefix == 0 {
            0
        } else {
            u32::MAX << (32 - prefix)
        };
        let network = u32::from(ip) & mask;
        Self {
            network: Ipv4Addr::from(network),
            broadcast: Ipv4Addr::from(network | !mask),
            prefix,
        }
    }

    fn contains(self, ip: Ipv4Addr) -> bool {
        let value = u32::from(ip);
        value >= u32::from(self.network) && value <= u32::from(self.broadcast)
    }

    #[cfg(test)]
    fn usable_host_count(self) -> usize {
        u32::from(self.broadcast)
            .saturating_sub(u32::from(self.network))
            .saturating_sub(1) as usize
    }

    #[cfg(test)]
    fn hosts(self) -> Result<Vec<Ipv4Addr>, String> {
        let count = self.usable_host_count();
        if count == 0 {
            return Err(format!("{} 没有可扫描的主机地址", self));
        }
        if count > MAX_SCAN_HOSTS {
            return Err(format!(
                "扫描范围包含 {count} 个主机，超过安全上限 {MAX_SCAN_HOSTS}；请缩小 CIDR"
            ));
        }
        let start = u32::from(self.network) + 1;
        Ok((0..count)
            .map(|offset| Ipv4Addr::from(start + offset as u32))
            .collect())
    }

    fn addresses(self) -> Result<Vec<Ipv4Addr>, String> {
        let count = u64::from(u32::from(self.broadcast)) - u64::from(u32::from(self.network)) + 1;
        if count > MAX_SCAN_HOSTS as u64 {
            return Err(format!(
                "扫描范围包含 {count} 个地址，超过安全上限 {MAX_SCAN_HOSTS}；请缩小 CIDR"
            ));
        }
        let start = u32::from(self.network);
        Ok((0..count)
            .map(|offset| Ipv4Addr::from(start + offset as u32))
            .collect())
    }
}

impl std::fmt::Display for Ipv4Network {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}/{}", self.network, self.prefix)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_normalizes_cidr() {
        let network = Ipv4Network::parse("192.168.10.42/24").unwrap();
        assert_eq!(network.to_string(), "192.168.10.0/24");
        assert_eq!(network.broadcast, Ipv4Addr::new(192, 168, 10, 255));
        assert_eq!(network.usable_host_count(), 254);
    }

    #[test]
    fn subrange_boundaries_remain_scannable() {
        let local = Ipv4Network::parse("192.168.10.0/24").unwrap();
        let range = Ipv4Network::parse("192.168.10.36/30").unwrap();
        let targets = scan_targets(range, local).unwrap();
        assert_eq!(targets.len(), 4);
        assert_eq!(targets[0], Ipv4Addr::new(192, 168, 10, 36));
        assert_eq!(targets[3], Ipv4Addr::new(192, 168, 10, 39));
    }

    #[test]
    fn rejects_oversized_scan() {
        let network = Ipv4Network::parse("10.0.0.0/16").unwrap();
        assert!(network.hosts().unwrap_err().contains("安全上限"));
    }

    #[test]
    fn validates_contiguous_netmask() {
        assert_eq!(
            prefix_from_netmask(Ipv4Addr::new(255, 255, 254, 0)).unwrap(),
            23
        );
        assert!(prefix_from_netmask(Ipv4Addr::new(255, 0, 255, 0)).is_err());
    }

    #[test]
    fn parses_neighbor_tables() {
        let text = "? (192.168.1.1) at aa:bb:cc:dd:ee:ff on en0\n\
                    192.168.1.20 dev eth0 lladdr 11:22:33:44:55:66 REACHABLE\n\
                    192.168.1.30  77-88-99-aa-bb-cc dynamic";
        let rows = parse_neighbor_table(text);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[&Ipv4Addr::new(192, 168, 1, 30)], "77:88:99:aa:bb:cc");
    }

    #[test]
    fn parses_route_outputs() {
        let linux = parse_linux_default_route(
            "default via 192.168.1.1 dev eth0 proto dhcp src 192.168.1.42 metric 100",
        );
        assert_eq!(linux.interface.as_deref(), Some("eth0"));
        assert_eq!(linux.gateway, Some(Ipv4Addr::new(192, 168, 1, 1)));

        let windows = parse_windows_default_route(
            "0.0.0.0          0.0.0.0      10.0.0.1      10.0.0.42     25",
        );
        assert_eq!(windows.local_ip, Some(Ipv4Addr::new(10, 0, 0, 42)));
        assert_eq!(windows.gateway, Some(Ipv4Addr::new(10, 0, 0, 1)));
    }
}
