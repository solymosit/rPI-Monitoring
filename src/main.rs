use axum::{
    extract::{ws::{Message, WebSocket, WebSocketUpgrade}, State},
    response::IntoResponse,
    routing::get,
    Router, Json,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, VecDeque},
    process::Command,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
    fs,
    path::Path,
};
use sysinfo::{System, Networks, Disks, CpuRefreshKind};
use tokio::sync::{broadcast, RwLock};
use tower_http::services::{ServeDir, ServeFile};

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct EthInterface {
    pub name: String,
    pub status: String,
    pub ip: String,
    pub speed: String,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct WifiInfo {
    pub name: String,
    pub status: String,
    pub ip: String,
    pub signal: String,
    pub freq: String,
    pub sec: String,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Stats {
    pub timestamp: u64,
    pub cpu: f32,
    pub memory_percent: f32,
    pub temp: f32,
    pub uptime_human: String,
    pub load_avg: String,
    pub cpu_cores: usize,
    pub cpu_pressure_1m: f32,
    pub cpu_pressure_5m: f32,
    pub cpu_pressure_15m: f32,
    pub fan_rpm: u32,
    pub cpu_freq: f32,
    pub disk_usage_percent: f32,
    pub gpu_freq: u32,
    pub disk_read_mb_s: f32,
    pub disk_write_mb_s: f32,
    pub net_recv_mb_s: f32,
    pub net_sent_mb_s: f32,
    pub eth_interfaces: Vec<EthInterface>,
    pub eth_name: String,
    pub eth_status: String,
    pub eth_ip: String,
    pub eth_speed: String,
    pub wifi: Option<WifiInfo>,
    pub wifi_status: String,
    pub wifi_ip: String,
    pub wifi_signal: String,
    pub wifi_freq: String,
    pub wifi_sec: String,
}

#[derive(Serialize)]
struct SystemInfo {
    hostname: String,
    os: String,
}

#[derive(Serialize)]
struct HistoryPayload<'a> {
    #[serde(rename = "type")]
    msg_type: &'static str,
    info: SystemInfo,
    data: &'a VecDeque<Stats>,
}

#[derive(Serialize)]
struct UpdatePayload<'a> {
    #[serde(rename = "type")]
    msg_type: &'static str,
    data: &'a Stats,
}

#[derive(Serialize)]
struct PowerReading {
    rail: String,
    name: String,
    description: String,
    voltage: f32,
    current: f32,
    power: f32,
}

#[derive(Serialize)]
struct PowerResponse {
    total_power: f32,
    readings: Vec<PowerReading>,
}

#[derive(Clone)]
struct AppState {
    history: Arc<RwLock<VecDeque<Stats>>>,
    tx: broadcast::Sender<Stats>,
}

const HISTORY_SIZE: usize = 300;

#[tokio::main]
async fn main() {
    let (tx, _rx) = broadcast::channel(16);
    let history = Arc::new(RwLock::new(VecDeque::with_capacity(HISTORY_SIZE)));

    let state = AppState {
        history: history.clone(),
        tx: tx.clone(),
    };

    tokio::spawn(collect_stats(history, tx));

    let app = Router::new()
        .route("/ws", get(ws_handler))
        .route("/api/power", get(power_api))
        .fallback_service(
            ServeDir::new("static")
                .fallback(ServeFile::new("static/index.html")),
        )
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:5000").await.unwrap();
    println!("Server running on http://0.0.0.0:5000");
    axum::serve(listener, app).await.unwrap();
}

fn get_real_hostname() -> String {
    if let Ok(content) = fs::read_to_string("/etc/hostname") {
        let name = content.trim();
        if !name.is_empty() {
            return name.to_string();
        }
    }
    if let Ok(content) = fs::read_to_string("/proc/sys/kernel/hostname") {
        let name = content.trim();
        if !name.is_empty() {
            return name.to_string();
        }
    }
    sysinfo::System::host_name().unwrap_or_else(|| "Unknown".to_string())
}

fn get_real_os() -> String {
    if let Ok(content) = fs::read_to_string("/etc/os-release") {
        let mut pretty_name = None;
        let mut name = None;
        let mut version = None;
        for line in content.lines() {
            let line = line.trim();
            if let Some(val) = line.strip_prefix("PRETTY_NAME=") {
                pretty_name = Some(val.trim_matches('"').to_string());
            } else if let Some(val) = line.strip_prefix("NAME=") {
                name = Some(val.trim_matches('"').to_string());
            } else if let Some(val) = line.strip_prefix("VERSION=") {
                version = Some(val.trim_matches('"').to_string());
            }
        }
        if let Some(pn) = pretty_name {
            if !pn.is_empty() {
                return pn;
            }
        }
        if let Some(n) = name {
            if let Some(v) = version {
                return format!("{} {}", n, v);
            }
            return n;
        }
    }
    let os = sysinfo::System::name().unwrap_or_else(|| "Linux".to_string());
    let os_ver = sysinfo::System::os_version().unwrap_or_default();
    let combined = format!("{} {}", os, os_ver);
    let trimmed = combined.trim();
    if trimmed.is_empty() {
        "Linux".to_string()
    } else {
        trimmed.to_string()
    }
}

fn get_uptime() -> String {
    let uptime = System::uptime();
    let minutes = (uptime / 60) % 60;
    let hours = (uptime / 3600) % 24;
    let days = uptime / 86400;
    format!("{}d {}h {}m", days, hours, minutes)
}

fn get_gpu_freq() -> u32 {
    match Command::new("vcgencmd").args(&["measure_clock", "core"]).output() {
        Ok(output) if output.status.success() => {
            let out_str = String::from_utf8_lossy(&output.stdout);
            if let Some(freq_str) = out_str.split('=').nth(1) {
                if let Ok(freq) = freq_str.trim().parse::<u64>() {
                    return (freq / 1_000_000) as u32;
                }
            }
            0
        }
        _ => 0,
    }
}

fn read_fan_rpm() -> u32 {
    let hwmon_base = "/sys/class/hwmon";
    if let Ok(entries) = fs::read_dir(hwmon_base) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Ok(name) = fs::read_to_string(path.join("name")) {
                if name.trim() == "pwmfan" {
                    if let Ok(rpm_str) = fs::read_to_string(path.join("fan1_input")) {
                        return rpm_str.trim().parse().unwrap_or(0);
                    }
                }
            }
        }
    }
    0
}

fn get_cpu_temp() -> f32 {
    if let Ok(temp_str) = fs::read_to_string("/sys/class/thermal/thermal_zone0/temp") {
        if let Ok(temp) = temp_str.trim().parse::<f32>() {
            return temp / 1000.0;
        }
    }
    0.0
}

fn get_load_avg() -> (f32, f32, f32) {
    if let Ok(load_str) = fs::read_to_string("/proc/loadavg") {
        let parts: Vec<&str> = load_str.split_whitespace().collect();
        if parts.len() >= 3 {
            let l1 = parts[0].parse().unwrap_or(0.0);
            let l5 = parts[1].parse().unwrap_or(0.0);
            let l15 = parts[2].parse().unwrap_or(0.0);
            return (l1, l5, l15);
        }
    }
    (0.0, 0.0, 0.0)
}

fn is_wireless_interface(name: &str, path: &Path) -> bool {
    path.join("wireless").exists()
        || path.join("phy80211").exists()
        || name.starts_with("wl")
        || name.starts_with("wlan")
        || name.starts_with("wifi")
}

fn find_eth_interfaces() -> Vec<String> {
    let mut eths = Vec::new();
    let net_path = Path::new("/sys/class/net");
    if let Ok(entries) = fs::read_dir(net_path) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name == "lo"
                || name.starts_with("docker")
                || name.starts_with("br-")
                || name.starts_with("veth")
                || name.starts_with("virbr")
                || name.starts_with("tun")
                || name.starts_with("tap")
            {
                continue;
            }
            let path = entry.path();
            if is_wireless_interface(&name, &path) {
                continue;
            }
            let is_physical = path.join("device").exists();
            let is_eth_name = name.starts_with("eth") || name.starts_with("en");
            if is_physical || is_eth_name {
                eths.push(name);
            }
        }
    }
    eths.sort();
    eths
}

fn find_wifi_interface() -> Option<String> {
    let net_path = Path::new("/sys/class/net");
    if let Ok(entries) = fs::read_dir(net_path) {
        let mut candidates = Vec::new();
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name == "lo" {
                continue;
            }
            let path = entry.path();
            if is_wireless_interface(&name, &path) {
                candidates.push(name);
            }
        }
        candidates.sort();
        return candidates.into_iter().next();
    }
    None
}

fn is_interface_disabled(iface: &str) -> bool {
    let base = format!("/sys/class/net/{}", iface);
    if let Ok(flags_str) = fs::read_to_string(format!("{}/flags", base)) {
        if let Ok(flags) = u32::from_str_radix(flags_str.trim().trim_start_matches("0x"), 16) {
            if (flags & 0x1) == 0 {
                return true;
            }
        }
    }
    if let Ok(state) = fs::read_to_string(format!("{}/operstate", base)) {
        let s = state.trim().to_lowercase();
        if s == "down" {
            return true;
        }
    }
    false
}

fn get_net_status(iface: &str) -> String {
    if let Ok(state) = fs::read_to_string(format!("/sys/class/net/{}/operstate", iface)) {
        state.trim().to_uppercase()
    } else {
        "DOWN".to_string()
    }
}

fn get_ip(iface: &str) -> String {
    if let Ok(output) = Command::new("ip").args(&["-4", "addr", "show", iface]).output() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            if line.trim().starts_with("inet ") {
                let parts: Vec<&str> = line.trim().split_whitespace().collect();
                if parts.len() >= 2 {
                    return parts[1].split('/').next().unwrap_or("--").to_string();
                }
            }
        }
    }
    "--".to_string()
}

fn get_eth_speed(iface: &str) -> String {
    if let Ok(speed) = fs::read_to_string(format!("/sys/class/net/{}/speed", iface)) {
        let s = speed.trim();
        if s != "-1" && !s.is_empty() {
            return format!("{} Mbps", s);
        }
    }
    "--".to_string()
}

fn get_wifi_details(iface: &str) -> (String, String) {
    let mut signal = "--".to_string();
    let mut freq = "--".to_string();
    
    if let Ok(output) = Command::new("iw").args(&["dev", iface, "link"]).output() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            let l = line.trim();
            if l.starts_with("signal:") {
                let parts: Vec<&str> = l.split_whitespace().collect();
                if parts.len() >= 2 {
                    signal = parts[1..].join(" ");
                }
            } else if l.starts_with("freq:") {
                let parts: Vec<&str> = l.split_whitespace().collect();
                if parts.len() >= 2 {
                    freq = parts[1..].join(" ");
                }
            }
        }
    }
    (signal, freq)
}

fn get_wifi_security(iface: &str) -> String {
    if let Ok(output) = Command::new("wpa_cli").args(&["-i", iface, "status"]).output() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            if line.starts_with("key_mgmt=") {
                return line.replace("key_mgmt=", "");
            }
        }
    }
    "--".to_string()
}

async fn collect_stats(history: Arc<RwLock<VecDeque<Stats>>>, tx: broadcast::Sender<Stats>) {
    let mut sys = System::new_all();
    let mut networks = Networks::new_with_refreshed_list();
    let mut disks = Disks::new_with_refreshed_list();
    
    let mut last_read = 0;
    let mut last_write = 0;
    let mut last_recv = 0;
    let mut last_sent = 0;

    let mut interval = tokio::time::interval(Duration::from_secs(1));

    loop {
        interval.tick().await;

        sys.refresh_cpu_specifics(CpuRefreshKind::everything());
        sys.refresh_memory();
        networks.refresh(true);
        disks.refresh(true);

        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64;

        let cpu = sys.global_cpu_usage() / 100.0;
        let memory_percent = sys.used_memory() as f32 / sys.total_memory() as f32;
        let temp = get_cpu_temp();
        let uptime_human = get_uptime();
        let (load1, load5, load15) = get_load_avg();
        
        let cores = sys.cpus().len().max(1);
        let cpu_pressure_1m = (load1 / cores as f32) * 100.0;
        let cpu_pressure_5m = (load5 / cores as f32) * 100.0;
        let cpu_pressure_15m = (load15 / cores as f32) * 100.0;

        let load_avg = format!("{:.2}, {:.2}, {:.2}", load1, load5, load15);
        let fan_rpm = read_fan_rpm();
        
        let cpu_freq = sys.cpus().first().map(|c| c.frequency() as f32).unwrap_or(0.0);

        let root_disk = disks.list().iter().find(|d| d.mount_point().to_string_lossy() == "/");
        let disk_usage_percent = if let Some(d) = root_disk {
            (d.total_space() - d.available_space()) as f32 / d.total_space() as f32
        } else {
            0.0
        };

        let mut current_read = 0;
        let mut current_write = 0;
        if let Ok(diskstats) = fs::read_to_string("/proc/diskstats") {
            for line in diskstats.lines() {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 13 {
                    let name = parts[2];
                    if name.starts_with("mmcblk") || name.starts_with("sda") || name.starts_with("nvme") {
                        if let (Ok(r_sec), Ok(w_sec)) = (parts[5].parse::<u64>(), parts[9].parse::<u64>()) {
                            current_read += r_sec * 512;
                            current_write += w_sec * 512;
                        }
                    }
                }
            }
        }

        let disk_read_mb_s = if last_read > 0 { (current_read.saturating_sub(last_read)) as f32 / (1024.0 * 1024.0) } else { 0.0 };
        let disk_write_mb_s = if last_write > 0 { (current_write.saturating_sub(last_write)) as f32 / (1024.0 * 1024.0) } else { 0.0 };
        last_read = current_read;
        last_write = current_write;

        let mut current_recv = 0;
        let mut current_sent = 0;
        for (_, data) in networks.iter() {
            current_recv += data.total_received();
            current_sent += data.total_transmitted();
        }

        let net_recv_mb_s = if last_recv > 0 { (current_recv.saturating_sub(last_recv)) as f32 / (1024.0 * 1024.0) } else { 0.0 };
        let net_sent_mb_s = if last_sent > 0 { (current_sent.saturating_sub(last_sent)) as f32 / (1024.0 * 1024.0) } else { 0.0 };
        last_recv = current_recv;
        last_sent = current_sent;

        let eth_names = find_eth_interfaces();
        let mut eth_interfaces = Vec::new();
        for eth_name in &eth_names {
            eth_interfaces.push(EthInterface {
                name: eth_name.clone(),
                status: get_net_status(eth_name),
                ip: get_ip(eth_name),
                speed: get_eth_speed(eth_name),
            });
        }

        let (eth_name, eth_status, eth_ip, eth_speed) = 
            if let Some(primary) = eth_interfaces.iter().find(|e| e.status == "UP").or_else(|| eth_interfaces.first()) {
                (primary.name.clone(), primary.status.clone(), primary.ip.clone(), primary.speed.clone())
            } else {
                ("eth0".to_string(), "DOWN".to_string(), "--".to_string(), "--".to_string())
            };

        let wifi_name_opt = find_wifi_interface();
        let (wifi, wifi_status, wifi_ip, wifi_signal, wifi_freq, wifi_sec) = if let Some(wname) = wifi_name_opt {
            let wstatus = get_net_status(&wname);
            let disabled = is_interface_disabled(&wname);
            if !disabled && wstatus == "UP" {
                let wip = get_ip(&wname);
                let (wsig, wfreq) = get_wifi_details(&wname);
                let wsec = get_wifi_security(&wname);
                let info = WifiInfo {
                    name: wname,
                    status: wstatus.clone(),
                    ip: wip.clone(),
                    signal: wsig.clone(),
                    freq: wfreq.clone(),
                    sec: wsec.clone(),
                };
                (Some(info), wstatus, wip, wsig, wfreq, wsec)
            } else {
                let status_str = if disabled { "DISABLED".to_string() } else { wstatus };
                (None, status_str, "--".to_string(), "--".to_string(), "--".to_string(), "--".to_string())
            }
        } else {
            (None, "DISABLED".to_string(), "--".to_string(), "--".to_string(), "--".to_string(), "--".to_string())
        };

        let stats = Stats {
            timestamp,
            cpu,
            memory_percent,
            temp,
            uptime_human,
            load_avg,
            cpu_cores: cores,
            cpu_pressure_1m,
            cpu_pressure_5m,
            cpu_pressure_15m,
            fan_rpm,
            cpu_freq,
            disk_usage_percent,
            gpu_freq: get_gpu_freq(),
            disk_read_mb_s,
            disk_write_mb_s,
            net_recv_mb_s,
            net_sent_mb_s,
            eth_interfaces,
            eth_name,
            eth_status,
            eth_ip,
            eth_speed,
            wifi,
            wifi_status,
            wifi_ip,
            wifi_signal,
            wifi_freq,
            wifi_sec,
        };

        {
            let mut h = history.write().await;
            if h.len() >= HISTORY_SIZE {
                h.pop_front();
            }
            h.push_back(stats.clone());
        }

        let _ = tx.send(stats);
    }
}

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

async fn handle_socket(mut socket: WebSocket, state: AppState) {
    let mut rx = state.tx.subscribe();

    {
        let h = state.history.read().await;
        let hostname = get_real_hostname();
        let os = get_real_os();
        
        let history_msg = HistoryPayload {
            msg_type: "history",
            info: SystemInfo { hostname, os },
            data: &*h,
        };

        if let Ok(msg) = serde_json::to_string(&history_msg) {
            if socket.send(Message::Text(msg.into())).await.is_err() {
                return;
            }
        }
    }

    loop {
        if let Ok(stats) = rx.recv().await {
            let update_msg = UpdatePayload {
                msg_type: "update",
                data: &stats,
            };
            if let Ok(msg) = serde_json::to_string(&update_msg) {
                if socket.send(Message::Text(msg.into())).await.is_err() {
                    break;
                }
            }
        }
    }
}

async fn power_api() -> impl IntoResponse {
    let rails = [
        ("VDD_CORE", "CPU & GPU Cores", "Main SoC processor and GPU graphics core power", 7, 15),
        ("3V3_SYS", "GPIO & 3.3V System", "40-pin GPIO header and primary 3.3V peripherals", 1, 9),
        ("DDR_VDD2", "RAM Core (DDR_VDD2)", "LPDDR4X SDRAM core voltage", 3, 11),
        ("DDR_VDDQ", "RAM I/O (DDR_VDDQ)", "LPDDR4X SDRAM data line I/O voltage", 4, 12),
        ("1V1_SYS", "PCIe & USB (1.1V)", "PCI Express and USB controller logic power", 5, 13),
        ("1V8_SYS", "1.8V System & I/O", "General SoC internal 1.8V logic voltage", 2, 10),
        ("0V8_SW", "RP1 I/O Controller", "RP1 southbridge peripheral controller core power", 6, 14),
        ("0V8_AON", "Standby / Always-On", "Continuous low-power always-on circuitry", 16, 19),
        ("3V7_WL_SW", "Wi-Fi & Bluetooth", "Wireless module power supply", 0, 8),
        ("HDMI", "HDMI Display", "HDMI video output and display circuitry", 22, 23),
        ("3V3_DAC", "Audio DAC", "Analog audio digital-to-analog converter", 17, 20),
        ("3V3_ADC", "Analog ADC", "Analog-to-digital converter circuitry", 18, 21),
    ];

    let mut channel_values: HashMap<u32, f32> = HashMap::new();

    if let Ok(output) = Command::new("vcgencmd").arg("pmic_read_adc").output() {
        if output.status.success() {
            let out_str = String::from_utf8_lossy(&output.stdout);
            for line in out_str.lines() {
                if let (Some(open_paren), Some(close_paren), Some(eq_pos)) = (
                    line.find('('),
                    line.find(')'),
                    line.find('='),
                ) {
                    if open_paren < close_paren && close_paren < eq_pos {
                        if let Ok(ch) = line[open_paren + 1..close_paren].trim().parse::<u32>() {
                            let val_str = &line[eq_pos + 1..];
                            let cleaned: String = val_str
                                .chars()
                                .filter(|c| c.is_ascii_digit() || *c == '.' || *c == '-')
                                .collect();
                            if let Ok(val) = cleaned.parse::<f32>() {
                                channel_values.insert(ch, val);
                            }
                        }
                    }
                }
            }
        }
    }

    let mut readings = Vec::new();
    let mut total_power = 0.0;

    for (rail_id, name, desc, ch_current, ch_voltage) in rails {
        if let (Some(&c), Some(&v)) = (channel_values.get(&ch_current), channel_values.get(&ch_voltage)) {
            let p = c * v;
            readings.push(PowerReading {
                rail: rail_id.to_string(),
                name: name.to_string(),
                description: desc.to_string(),
                voltage: v,
                current: c,
                power: p,
            });
            total_power += p;
        }
    }

    Json(PowerResponse {
        total_power,
        readings,
    })
}
