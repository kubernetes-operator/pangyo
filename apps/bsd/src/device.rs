//! 기기 상태 (요구 09): 배터리 · Wi-Fi · Bluetooth. FreeBSD 우선, Linux는 개발용.
//!
//! 키오스크 정책(docs/10_BSD.md): 셸을 거치지 않고 **정해진 명령을 정해진 인자로만** 실행합니다.
//! 명령 출력 해석은 순수 함수로 분리해 실제 출력 예시(fixture)로 테스트합니다.
//! 실제 칩셋/드라이버별 출력은 하드웨어 선정 후 실기기에서 다시 확인해야 합니다.
use std::{fs, process::Command};

// === 0. 상태 읽기 변수 ===
/// FreeBSD 무선 인터페이스 이름 (보드에 따라 wlan0 외일 수 있음)
pub const WLAN_INTERFACE: &str = "wlan0";
/// FreeBSD 블루투스 HCI 노드 (USB 동글 기본값)
pub const BT_NODE: &str = "ubt0hci";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Battery {
    pub percent: u8,
    pub charging: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Wifi {
    #[default]
    Unknown,
    Off,
    Disconnected,
    Connected(String),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeviceStatus {
    pub battery: Option<Battery>,
    pub wifi: Wifi,
    /// None = 장치/드라이버 없음
    pub bluetooth: Option<bool>,
}

impl DeviceStatus {
    /// 상단 표시줄 문구: "BAT 80%+ · WiFi home · BT on"
    pub fn summary(&self) -> String {
        let bat = match &self.battery {
            Some(b) => format!("BAT {}%{}", b.percent, if b.charging { "+" } else { "" }),
            None => "BAT --".to_owned(),
        };
        let wifi = match &self.wifi {
            Wifi::Unknown => "WiFi --".to_owned(),
            Wifi::Off => "WiFi off".to_owned(),
            Wifi::Disconnected => "WiFi 끊김".to_owned(),
            Wifi::Connected(ssid) => format!("WiFi {ssid}"),
        };
        let bt = match self.bluetooth {
            None => "BT --",
            Some(true) => "BT on",
            Some(false) => "BT off",
        };
        format!("{bat} · {wifi} · {bt}")
    }
}

/// 정해진 명령 실행 (셸 없음). 실패하면 None
fn run(program: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(program).args(args).output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// 현재 OS에 맞게 상태를 읽습니다 (느릴 수 있으니 화면 스레드 밖에서 호출).
pub fn read_status() -> DeviceStatus {
    if cfg!(target_os = "freebsd") {
        DeviceStatus {
            battery: run(
                "sysctl",
                &["-n", "hw.acpi.battery.life", "hw.acpi.battery.state"],
            )
            .and_then(|s| parse_freebsd_battery(&s)),
            wifi: run("ifconfig", &[WLAN_INTERFACE]).map_or(Wifi::Off, |s| parse_freebsd_wifi(&s)),
            bluetooth: run("hccontrol", &["-n", BT_NODE, "read_scan_enable"])
                .map(|s| parse_freebsd_bt_scan(&s)),
        }
    } else {
        DeviceStatus {
            battery: read_linux_battery(),
            wifi: fs::read_to_string("/proc/net/wireless")
                .ok()
                .map_or(Wifi::Unknown, |s| parse_linux_wireless(&s)),
            bluetooth: fs::read_dir("/sys/class/bluetooth")
                .ok()
                .map(|mut d| d.next().is_some()),
        }
    }
}

/// `sysctl -n hw.acpi.battery.life hw.acpi.battery.state` → "87\n2\n"
/// state 비트: 1=방전, 2=충전, 4=위험. 배터리 없으면 life=-1 또는 오류
pub fn parse_freebsd_battery(out: &str) -> Option<Battery> {
    let mut it = out.split_whitespace();
    let life: i32 = it.next()?.parse().ok()?;
    let state: i32 = it.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    if !(0..=100).contains(&life) {
        return None;
    }
    Some(Battery {
        percent: life as u8,
        charging: state & 2 != 0,
    })
}

/// `ifconfig wlan0` 출력에서 연결 상태와 SSID
pub fn parse_freebsd_wifi(out: &str) -> Wifi {
    let associated = out.lines().any(|l| l.trim() == "status: associated");
    if !associated {
        return if out.contains("status: no carrier") {
            Wifi::Disconnected
        } else {
            Wifi::Off
        };
    }
    out.lines()
        .find_map(|l| l.trim().strip_prefix("ssid "))
        .map(|rest| {
            // "ssid MyHome channel 6 (2437 MHz 11g ht/20) bssid ..." / 공백 SSID는 따옴표로 감쌈
            let ssid = if let Some(quoted) = rest.strip_prefix('"') {
                quoted.split('"').next().unwrap_or_default().to_owned()
            } else {
                rest.split_whitespace()
                    .next()
                    .unwrap_or_default()
                    .to_owned()
            };
            Wifi::Connected(ssid)
        })
        .unwrap_or(Wifi::Connected(String::new()))
}

/// `hccontrol -n ubt0hci read_scan_enable` → "Scan enable: Page and Inquiry scan enabled [0x3]"
pub fn parse_freebsd_bt_scan(out: &str) -> bool {
    !out.contains("[0x0]")
}

fn read_linux_battery() -> Option<Battery> {
    let dir = fs::read_dir("/sys/class/power_supply").ok()?;
    for entry in dir.flatten() {
        let path = entry.path();
        if !entry.file_name().to_string_lossy().starts_with("BAT") {
            continue;
        }
        let percent = fs::read_to_string(path.join("capacity"))
            .ok()?
            .trim()
            .parse()
            .ok()?;
        let status = fs::read_to_string(path.join("status")).unwrap_or_default();
        return Some(Battery {
            percent,
            charging: status.trim() == "Charging",
        });
    }
    None
}

/// `/proc/net/wireless`: 헤더 2줄 뒤에 인터페이스가 있으면 연결된 것으로 봄 (SSID는 알 수 없음)
pub fn parse_linux_wireless(out: &str) -> Wifi {
    if out.lines().skip(2).any(|l| l.contains(':')) {
        Wifi::Connected(String::new())
    } else {
        Wifi::Disconnected
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const IFCONFIG_ASSOCIATED: &str =
        "wlan0: flags=8843<UP,BROADCAST,RUNNING,SIMPLEX,MULTICAST> metric 0 mtu 1500
\tether 00:11:22:33:44:55
\tinet 192.0.2.10 netmask 0xffffff00 broadcast 192.0.2.255
\tgroups: wlan
\tssid \"Home Net\" channel 36 (5180 MHz 11a ht/40+) bssid 66:77:88:99:aa:bb
\tregdomain FCC country US authmode WPA2/802.11i privacy ON
\tmedia: IEEE 802.11 Wireless Ethernet MCS mode 11na
\tstatus: associated
";

    #[test]
    fn freebsd_wifi_states() {
        assert_eq!(
            parse_freebsd_wifi(IFCONFIG_ASSOCIATED),
            Wifi::Connected("Home Net".into())
        );
        let plain = IFCONFIG_ASSOCIATED.replace("\"Home Net\"", "office");
        assert_eq!(parse_freebsd_wifi(&plain), Wifi::Connected("office".into()));
        let down = "wlan0: flags=8802<BROADCAST,SIMPLEX,MULTICAST>\n\tssid \"\" channel 1\n\tstatus: no carrier\n";
        assert_eq!(parse_freebsd_wifi(down), Wifi::Disconnected);
        assert_eq!(
            parse_freebsd_wifi("ifconfig: interface wlan0 does not exist"),
            Wifi::Off
        );
    }

    #[test]
    fn freebsd_battery() {
        assert_eq!(
            parse_freebsd_battery("87\n2\n"),
            Some(Battery {
                percent: 87,
                charging: true
            })
        );
        assert_eq!(
            parse_freebsd_battery("40\n1\n"),
            Some(Battery {
                percent: 40,
                charging: false
            })
        );
        assert_eq!(parse_freebsd_battery("-1\n7\n"), None, "배터리 없음");
        assert_eq!(parse_freebsd_battery(""), None);
    }

    #[test]
    fn freebsd_bluetooth_and_linux_wireless() {
        assert!(parse_freebsd_bt_scan(
            "Scan enable: Page and Inquiry scan enabled [0x3]"
        ));
        assert!(!parse_freebsd_bt_scan(
            "Scan enable: No scans enabled [0x0]"
        ));
        let header = "Inter-| sta-|   Quality        |   Discarded packets\n face | tus | link level noise |  nwid  crypt\n";
        assert_eq!(parse_linux_wireless(header), Wifi::Disconnected);
        assert_eq!(
            parse_linux_wireless(&format!("{header}wlp2s0: 0000   60.  -50.  -256\n")),
            Wifi::Connected(String::new())
        );
    }

    #[test]
    fn summary_line() {
        let s = DeviceStatus {
            battery: Some(Battery {
                percent: 80,
                charging: true,
            }),
            wifi: Wifi::Connected("home".into()),
            bluetooth: Some(false),
        };
        assert_eq!(s.summary(), "BAT 80%+ · WiFi home · BT off");
        assert_eq!(
            DeviceStatus::default().summary(),
            "BAT -- · WiFi -- · BT --"
        );
    }
}
