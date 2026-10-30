//! The status chip, `$FF50`: power and system readings, like a handheld's
//! fuel gauge. Battery charge, mains and charging, CPU temperature and the
//! network address, which come from the host and are refreshed once a
//! second. See docs/machine.md.

use std::fs;
use std::net::{IpAddr, UdpSocket};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, AtomicU32, Ordering};
use std::time::Duration;

const BATTERY: u16 = 0xFF50;
const POWER: u16 = 0xFF51;
const TEMPERATURE: u16 = 0xFF52;
const IP: u16 = 0xFF54;
const IP_END: u16 = 0xFF57;

/// `BATTERY`: there is no battery.
pub const NO_BATTERY: u8 = 0xFF;
/// `POWER` bits.
pub const MAINS: u8 = 0x01;
pub const CHARGING: u8 = 0x02;

const REFRESH: Duration = Duration::from_secs(1);

#[derive(Debug)]
pub struct Status {
    pub battery: AtomicU8,
    pub power: AtomicU8,
    pub temperature: AtomicU8,
    /// IPv4 address as a big-endian `u32`; 0 when offline.
    pub ip: AtomicU32,
}

impl Default for Status {
    fn default() -> Self {
        Status {
            battery: AtomicU8::new(NO_BATTERY),
            power: AtomicU8::new(0),
            temperature: AtomicU8::new(0),
            ip: AtomicU32::new(0),
        }
    }
}

impl Status {
    /// Start refreshing the readings from the host in the background.
    pub fn start() -> Arc<Status> {
        let status = Arc::new(Status::default());
        let shared = Arc::clone(&status);
        std::thread::spawn(move || {
            loop {
                shared.refresh();
                std::thread::sleep(REFRESH);
            }
        });
        status
    }

    /// On battery: there is one and the host is not on mains.
    pub fn on_battery(&self) -> bool {
        self.battery.load(Ordering::Relaxed) != NO_BATTERY
            && self.power.load(Ordering::Relaxed) & MAINS == 0
    }

    pub fn read(&self, addr: u16) -> u8 {
        match addr {
            BATTERY => self.battery.load(Ordering::Relaxed),
            POWER => self.power.load(Ordering::Relaxed),
            TEMPERATURE => self.temperature.load(Ordering::Relaxed),
            IP..=IP_END => self.ip.load(Ordering::Relaxed).to_be_bytes()[(addr - IP) as usize],
            _ => 0,
        }
    }

    fn refresh(&self) {
        let (battery, power) = power_supplies();
        self.battery.store(battery, Ordering::Relaxed);
        self.power.store(power, Ordering::Relaxed);
        self.temperature.store(cpu_temperature(), Ordering::Relaxed);
        self.ip.store(ipv4_address(), Ordering::Relaxed);
    }
}

fn read_trimmed(path: &Path) -> Option<String> {
    fs::read_to_string(path).ok().map(|text| text.trim().to_string())
}

/// Battery charge and `POWER` bits from `/sys/class/power_supply`.
fn power_supplies() -> (u8, u8) {
    let mut battery = NO_BATTERY;
    let mut power = 0;
    let Ok(supplies) = fs::read_dir("/sys/class/power_supply") else {
        return (battery, power);
    };
    for supply in supplies.flatten() {
        let path = supply.path();
        match read_trimmed(&path.join("type")).as_deref() {
            Some("Battery") => {
                if let Some(capacity) = read_trimmed(&path.join("capacity")) {
                    battery = capacity.parse::<u8>().map_or(battery, |c| c.min(100));
                }
                if read_trimmed(&path.join("status")).as_deref() == Some("Charging") {
                    power |= CHARGING;
                }
            }
            Some("Mains" | "USB") => {
                if read_trimmed(&path.join("online")).as_deref() == Some("1") {
                    power |= MAINS;
                }
            }
            _ => {}
        }
    }
    (battery, power)
}

/// The first thermal zone's temperature in °C, 0 if there is none.
fn cpu_temperature() -> u8 {
    read_trimmed(Path::new("/sys/class/thermal/thermal_zone0/temp"))
        .and_then(|millidegrees| millidegrees.parse::<i32>().ok())
        .map_or(0, |millidegrees| (millidegrees / 1000).clamp(0, 255) as u8)
}

/// The address of the interface the default route uses. Connecting a UDP
/// socket only picks a route; nothing is sent.
fn ipv4_address() -> u32 {
    let address = UdpSocket::bind("0.0.0.0:0")
        .and_then(|socket| {
            socket.connect("192.0.2.1:9")?;
            socket.local_addr()
        })
        .map(|local| local.ip());
    match address {
        Ok(IpAddr::V4(ip)) if !ip.is_unspecified() => u32::from(ip),
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_its_readings() {
        let status = Status::default();
        assert_eq!(status.read(BATTERY), NO_BATTERY);
        status.battery.store(87, Ordering::Relaxed);
        status.power.store(MAINS | CHARGING, Ordering::Relaxed);
        status.temperature.store(45, Ordering::Relaxed);
        status.ip.store(u32::from(std::net::Ipv4Addr::new(192, 168, 12, 41)), Ordering::Relaxed);
        assert_eq!((status.read(BATTERY), status.read(POWER), status.read(TEMPERATURE)), (87, 3, 45));
        let ip: Vec<u8> = (IP..=IP_END).map(|addr| status.read(addr)).collect();
        assert_eq!(ip, [192, 168, 12, 41]);
        assert!(!status.on_battery());
    }
}
