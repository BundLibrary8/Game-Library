//! The device manager: the peripherals connected to this PC (keyboards,
//! mice, controllers, headsets, webcams, monitors, drives…) and the
//! Bluetooth devices paired with it, which can be connected and
//! disconnected.
//!
//! **Peripherals** come from Windows' own device list (`Get-PnpDevice`,
//! the same list Device Manager shows), keeping only devices that are
//! present now and are things you plug in, not the PC's internals.
//!
//! **Bluetooth** uses Windows' classic Bluetooth API
//! (`bluetoothapis.dll`): it lists paired devices and whether each is
//! connected. Windows has no single "connect" call for every kind of
//! device, so GameLib does what Settings does behind the scenes: it turns
//! the device's Bluetooth services (audio, hands-free, input…) off to
//! disconnect it, and back on to connect it. The device has to be switched
//! on and in range to connect. Bluetooth LE-only devices (some mice and
//! trackers) aren't on that list.
//!
//! Elsewhere than Windows, peripherals are read from Linux's input device
//! list and Bluetooth isn't supported.

use anyhow::Result;
use serde::Deserialize;

/// What sort of device it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Kind {
    Controller,
    Keyboard,
    Mouse,
    Audio,
    Camera,
    Display,
    Storage,
    Printer,
    Bluetooth,
    Other,
}

impl Kind {
    pub const ALL: [Kind; 10] = [
        Kind::Controller,
        Kind::Keyboard,
        Kind::Mouse,
        Kind::Audio,
        Kind::Camera,
        Kind::Display,
        Kind::Storage,
        Kind::Printer,
        Kind::Bluetooth,
        Kind::Other,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Kind::Controller => "Controllers",
            Kind::Keyboard => "Keyboards",
            Kind::Mouse => "Mice and pointing devices",
            Kind::Audio => "Audio",
            Kind::Camera => "Cameras",
            Kind::Display => "Monitors",
            Kind::Storage => "Drives",
            Kind::Printer => "Printers",
            Kind::Bluetooth => "Bluetooth adapters",
            Kind::Other => "Other",
        }
    }
}

/// One connected peripheral.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    pub name: String,
    pub kind: Kind,
    /// "OK", "Error", "Degraded"… as Windows reports it.
    pub status: String,
    pub manufacturer: Option<String>,
    /// Windows' instance id, like `USB\VID_046D&PID_C52B\…`.
    pub id: String,
    /// Plugged in by USB, connected over Bluetooth, or other.
    pub bus: Bus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bus {
    Usb,
    Bluetooth,
    Other,
}

impl Device {
    pub fn is_ok(&self) -> bool {
        self.status.eq_ignore_ascii_case("ok")
    }
}

/// A paired Bluetooth device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BtDevice {
    pub name: String,
    /// The device's address, like `00:1A:7D:DA:71:13`, as a number.
    pub address: u64,
    pub connected: bool,
    /// What Windows says it is (headphones, mouse…), from its class code.
    pub kind: &'static str,
}

impl BtDevice {
    pub fn address_text(&self) -> String {
        format_address(self.address)
    }
}

pub fn format_address(address: u64) -> String {
    (0..6)
        .rev()
        .map(|i| format!("{:02X}", (address >> (i * 8)) & 0xff))
        .collect::<Vec<_>>()
        .join(":")
}

/// What a Bluetooth class-of-device code means, roughly.
pub fn bluetooth_kind(class_of_device: u32) -> &'static str {
    let major = (class_of_device >> 8) & 0x1f;
    let minor = (class_of_device >> 2) & 0x3f;
    match major {
        1 => "Computer",
        2 => "Phone",
        4 => match minor {
            1 | 2 => "Headset",
            5 | 6 => "Speaker",
            _ => "Audio",
        },
        5 => match minor & 0x30 {
            0x10 => "Keyboard",
            0x20 => "Mouse",
            0x30 => "Keyboard and mouse",
            _ if minor & 0x0f == 1 || minor & 0x0f == 2 => "Controller",
            _ => "Input device",
        },
        6 => "Printer or camera",
        7 => "Wearable",
        8 => "Toy",
        9 => "Health device",
        _ => "Device",
    }
}

// --- Peripherals ---------------------------------------------------------

/// One row of `Get-PnpDevice | ConvertTo-Json`.
#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct PnpRow {
    friendly_name: Option<String>,
    class: Option<String>,
    status: Option<String>,
    manufacturer: Option<String>,
    instance_id: Option<String>,
}

/// Windows device classes, and what they are here. Classes not listed
/// (system devices, processors, disk controllers…) are the PC's insides
/// and are left out.
fn kind_of_class(class: &str) -> Option<Kind> {
    Some(match class.to_ascii_lowercase().as_str() {
        "keyboard" => Kind::Keyboard,
        "mouse" => Kind::Mouse,
        "xnacomposite" | "xboxcomposite" | "hidclass" => Kind::Controller,
        "audioendpoint" | "media" => Kind::Audio,
        "camera" | "image" => Kind::Camera,
        "monitor" => Kind::Display,
        "diskdrive" | "wpd" | "cdrom" => Kind::Storage,
        "printer" | "printqueue" => Kind::Printer,
        "bluetooth" => Kind::Bluetooth,
        _ => return None,
    })
}

/// Names that are plumbing rather than something you'd recognise.
fn is_plumbing(name: &str, class: &str) -> bool {
    let lower = name.to_lowercase();
    let generic = [
        "hid-compliant vendor-defined device",
        "hid-compliant system controller",
        "hid-compliant consumer control device",
        "usb input device",
        "microsoft bluetooth enumerator",
        "microsoft bluetooth le enumerator",
        "bluetooth device (rfcomm protocol tdi)",
        "bluetooth device (personal area network)",
        "generic pnp monitor",
    ];
    if generic.iter().any(|g| lower == *g) {
        return true;
    }
    // In the Bluetooth class, only the adapters matter; paired devices are
    // listed separately with connect buttons.
    if class.eq_ignore_ascii_case("bluetooth") {
        return !(lower.contains("adapter")
            || lower.contains("radio")
            || lower.contains("wireless"));
    }
    // HID game controllers are worth showing; other HID odds and ends aren't.
    if class.eq_ignore_ascii_case("hidclass") {
        return !(lower.contains("game")
            || lower.contains("controller")
            || lower.contains("gamepad")
            || lower.contains("joystick"));
    }
    false
}

/// Turn Windows' JSON device list into devices. (PowerShell writes a
/// single object instead of a list when there's only one.)
pub fn parse_pnp_json(json: &str) -> Result<Vec<Device>> {
    let json = json.trim();
    if json.is_empty() {
        return Ok(Vec::new());
    }
    let rows: Vec<PnpRow> = if json.starts_with('[') {
        serde_json::from_str(json)?
    } else {
        vec![serde_json::from_str(json)?]
    };
    let mut devices: Vec<Device> = rows
        .into_iter()
        .filter_map(|row| {
            let class = row.class.unwrap_or_default();
            let name = row.friendly_name.filter(|n| !n.trim().is_empty())?;
            let kind = kind_of_class(&class)?;
            if is_plumbing(&name, &class) {
                return None;
            }
            let id = row.instance_id.unwrap_or_default();
            let upper = id.to_uppercase();
            let bus = if upper.starts_with("USB") || upper.contains("VID_") {
                Bus::Usb
            } else if upper.starts_with("BTH")
                || upper.contains("BTHENUM")
                || upper.contains("BTHLE")
            {
                Bus::Bluetooth
            } else {
                Bus::Other
            };
            Some(Device {
                name: name.trim().to_string(),
                kind,
                status: row.status.unwrap_or_else(|| "Unknown".into()),
                manufacturer: row
                    .manufacturer
                    .filter(|m| !m.trim().is_empty() && !m.starts_with('(')),
                id,
                bus,
            })
        })
        .collect();
    // The same name listed twice (one device, several interfaces) is shown once.
    devices.sort_by(|a, b| {
        a.kind
            .cmp(&b.kind)
            .then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    devices.dedup_by(|a, b| a.kind == b.kind && a.name.eq_ignore_ascii_case(&b.name));
    Ok(devices)
}

/// The peripherals connected right now.
pub fn list_devices() -> Result<Vec<Device>> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        use std::process::Command;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let script = "Get-PnpDevice -PresentOnly | \
            Select-Object FriendlyName,Class,Status,Manufacturer,InstanceId | \
            ConvertTo-Json -Compress";
        let output = Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", script])
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .map_err(|err| anyhow::anyhow!("couldn't ask Windows for its devices: {err}"))?;
        anyhow::ensure!(
            output.status.success(),
            "Windows couldn't list its devices: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
        parse_pnp_json(&String::from_utf8_lossy(&output.stdout))
    }
    #[cfg(not(windows))]
    {
        Ok(linux_input_devices())
    }
}

/// Linux: the input devices the kernel knows about.
#[cfg(not(windows))]
fn linux_input_devices() -> Vec<Device> {
    let Ok(entries) = std::fs::read_dir("/sys/class/input") else {
        return Vec::new();
    };
    let mut devices: Vec<Device> = entries
        .filter_map(Result::ok)
        .filter(|e| e.file_name().to_string_lossy().starts_with("input"))
        .filter_map(|e| {
            let name = std::fs::read_to_string(e.path().join("name")).ok()?;
            let name = name.trim().to_string();
            let lower = name.to_lowercase();
            let kind = if lower.contains("keyboard") {
                Kind::Keyboard
            } else if lower.contains("mouse") || lower.contains("touchpad") {
                Kind::Mouse
            } else if lower.contains("controller")
                || lower.contains("gamepad")
                || lower.contains("joystick")
            {
                Kind::Controller
            } else {
                Kind::Other
            };
            Some(Device {
                name,
                kind,
                status: "OK".into(),
                manufacturer: None,
                id: e.file_name().to_string_lossy().into_owned(),
                bus: Bus::Other,
            })
        })
        .collect();
    devices.sort_by(|a, b| a.kind.cmp(&b.kind).then(a.name.cmp(&b.name)));
    devices.dedup_by(|a, b| a.name == b.name);
    devices
}

// --- Bluetooth -----------------------------------------------------------

/// Bluetooth devices paired with this PC.
pub fn bluetooth_devices() -> Result<Vec<BtDevice>> {
    #[cfg(windows)]
    {
        windows_bt::list()
    }
    #[cfg(not(windows))]
    {
        anyhow::bail!("Bluetooth control is only available on Windows")
    }
}

/// Connect (`true`) or disconnect (`false`) a paired Bluetooth device.
pub fn set_bluetooth_connected(address: u64, connect: bool) -> Result<()> {
    #[cfg(windows)]
    {
        windows_bt::set_connected(address, connect)
    }
    #[cfg(not(windows))]
    {
        let _ = (address, connect);
        anyhow::bail!("Bluetooth control is only available on Windows")
    }
}

#[cfg(windows)]
mod windows_bt {
    use anyhow::{Result, bail};
    use windows_sys::Win32::Devices::Bluetooth::*;
    use windows_sys::Win32::Foundation::{CloseHandle, ERROR_SUCCESS, HANDLE};
    use windows_sys::core::GUID;

    use super::BtDevice;

    /// The PC's Bluetooth radios. Closed when dropped.
    struct Radios(Vec<HANDLE>);

    impl Drop for Radios {
        fn drop(&mut self) {
            for handle in &self.0 {
                unsafe { CloseHandle(*handle) };
            }
        }
    }

    fn radios() -> Radios {
        let params = BLUETOOTH_FIND_RADIO_PARAMS {
            dwSize: std::mem::size_of::<BLUETOOTH_FIND_RADIO_PARAMS>() as u32,
        };
        let mut handles = Vec::new();
        let mut radio: HANDLE = std::ptr::null_mut();
        unsafe {
            let find = BluetoothFindFirstRadio(&params, &mut radio);
            if find.is_null() {
                return Radios(handles);
            }
            handles.push(radio);
            while BluetoothFindNextRadio(find, &mut radio) != 0 {
                handles.push(radio);
            }
            BluetoothFindRadioClose(find);
        }
        Radios(handles)
    }

    fn name_of(info: &BLUETOOTH_DEVICE_INFO) -> String {
        let end = info
            .szName
            .iter()
            .position(|c| *c == 0)
            .unwrap_or(info.szName.len());
        String::from_utf16_lossy(&info.szName[..end])
    }

    /// Every paired device on one radio, as Windows' own records.
    fn paired(radio: HANDLE) -> Vec<BLUETOOTH_DEVICE_INFO> {
        let params = BLUETOOTH_DEVICE_SEARCH_PARAMS {
            dwSize: std::mem::size_of::<BLUETOOTH_DEVICE_SEARCH_PARAMS>() as u32,
            fReturnAuthenticated: 1,
            fReturnRemembered: 1,
            fReturnUnknown: 0,
            fReturnConnected: 1,
            fIssueInquiry: 0,
            cTimeoutMultiplier: 0,
            hRadio: radio,
        };
        let mut found = Vec::new();
        let mut info = BLUETOOTH_DEVICE_INFO {
            dwSize: std::mem::size_of::<BLUETOOTH_DEVICE_INFO>() as u32,
            ..Default::default()
        };
        unsafe {
            let find = BluetoothFindFirstDevice(&params, &mut info);
            if find.is_null() {
                return found;
            }
            loop {
                found.push(info);
                info = BLUETOOTH_DEVICE_INFO {
                    dwSize: std::mem::size_of::<BLUETOOTH_DEVICE_INFO>() as u32,
                    ..Default::default()
                };
                if BluetoothFindNextDevice(find, &mut info) == 0 {
                    break;
                }
            }
            BluetoothFindDeviceClose(find);
        }
        found
    }

    pub fn list() -> Result<Vec<BtDevice>> {
        let radios = radios();
        if radios.0.is_empty() {
            bail!("this PC has no Bluetooth adapter, or Bluetooth is switched off");
        }
        let mut devices: Vec<BtDevice> = Vec::new();
        for radio in &radios.0 {
            for info in paired(*radio) {
                let address = unsafe { info.Address.Anonymous.ullLong };
                if devices.iter().any(|d| d.address == address) {
                    continue;
                }
                devices.push(BtDevice {
                    name: name_of(&info),
                    address,
                    connected: info.fConnected != 0,
                    kind: super::bluetooth_kind(info.ulClassofDevice),
                });
            }
        }
        devices.sort_by(|a, b| {
            b.connected
                .cmp(&a.connected)
                .then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        Ok(devices)
    }

    pub fn set_connected(address: u64, connect: bool) -> Result<()> {
        let radios = radios();
        for radio in &radios.0 {
            let Some(info) = paired(*radio)
                .into_iter()
                .find(|i| unsafe { i.Address.Anonymous.ullLong } == address)
            else {
                continue;
            };
            // The services the device offers (audio, hands-free, input…).
            let mut services = [GUID::from_u128(0); 32];
            let mut count = services.len() as u32;
            let result = unsafe {
                BluetoothEnumerateInstalledServices(
                    *radio,
                    &info,
                    &mut count,
                    services.as_mut_ptr(),
                )
            };
            if result != ERROR_SUCCESS || count == 0 {
                bail!(
                    "Windows didn't list any services for {}, so it can't be {} from here. Try Windows' Bluetooth settings.",
                    name_of(&info),
                    if connect { "connected" } else { "disconnected" }
                );
            }
            let services = &services[..count.min(32) as usize];
            let mut failed = 0;
            for service in services {
                // To connect, turn each service off and on again: turning
                // it on makes Windows reach out to the device.
                if connect {
                    unsafe {
                        BluetoothSetServiceState(*radio, &info, service, BLUETOOTH_SERVICE_DISABLE)
                    };
                }
                let flag = if connect {
                    BLUETOOTH_SERVICE_ENABLE
                } else {
                    BLUETOOTH_SERVICE_DISABLE
                };
                if unsafe { BluetoothSetServiceState(*radio, &info, service, flag) }
                    != ERROR_SUCCESS
                {
                    failed += 1;
                }
            }
            if failed == services.len() {
                bail!(
                    "Windows refused to {} {}. Make sure it's switched on and nearby.",
                    if connect { "connect" } else { "disconnect" },
                    name_of(&info)
                );
            }
            return Ok(());
        }
        bail!("that device isn't paired with this PC any more")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_device_lists_are_read_and_tidied() {
        let json = r#"[
            {"FriendlyName":"Logitech G502","Class":"Mouse","Status":"OK","Manufacturer":"Logitech","InstanceId":"HID\\VID_046D&PID_C08B\\7"},
            {"FriendlyName":"HID Keyboard Device","Class":"Keyboard","Status":"OK","Manufacturer":"(Standard keyboards)","InstanceId":"HID\\VID_1532&PID_0227\\8"},
            {"FriendlyName":"Xbox Wireless Controller","Class":"XnaComposite","Status":"OK","Manufacturer":"Microsoft","InstanceId":"USB\\VID_045E&PID_0B12\\9"},
            {"FriendlyName":"HID-compliant vendor-defined device","Class":"HIDClass","Status":"OK","Manufacturer":null,"InstanceId":"HID\\X"},
            {"FriendlyName":"HID-compliant game controller","Class":"HIDClass","Status":"OK","Manufacturer":null,"InstanceId":"HID\\Y"},
            {"FriendlyName":"Intel(R) Wireless Bluetooth(R)","Class":"Bluetooth","Status":"OK","Manufacturer":"Intel","InstanceId":"USB\\VID_8087"},
            {"FriendlyName":"WH-1000XM4","Class":"Bluetooth","Status":"OK","Manufacturer":"Sony","InstanceId":"BTHENUM\\DEV_X"},
            {"FriendlyName":"Headphones (WH-1000XM4)","Class":"AudioEndpoint","Status":"OK","Manufacturer":"Microsoft","InstanceId":"SWD\\MMDEVAPI\\{0.0.0}"},
            {"FriendlyName":"Intel(R) Core(TM) i7","Class":"Processor","Status":"OK","Manufacturer":"Intel","InstanceId":"ACPI\\CPU"},
            {"FriendlyName":"Logitech G502","Class":"Mouse","Status":"OK","Manufacturer":"Logitech","InstanceId":"HID\\VID_046D&PID_C08B\\8"},
            {"FriendlyName":"USB Camera","Class":"Camera","Status":"Error","Manufacturer":"Acme","InstanceId":"USB\\VID_1"},
            {"FriendlyName":null,"Class":"Mouse","Status":"OK","Manufacturer":null,"InstanceId":"X"}
        ]"#;
        let devices = parse_pnp_json(json).unwrap();
        let names: Vec<&str> = devices.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "HID-compliant game controller",
                "Xbox Wireless Controller",
                "HID Keyboard Device",
                "Logitech G502",
                "Headphones (WH-1000XM4)",
                "USB Camera",
                "Intel(R) Wireless Bluetooth(R)",
            ]
        );
        let mouse = devices.iter().find(|d| d.kind == Kind::Mouse).unwrap();
        assert_eq!(mouse.bus, Bus::Usb);
        assert_eq!(mouse.manufacturer.as_deref(), Some("Logitech"));
        let keyboard = devices.iter().find(|d| d.kind == Kind::Keyboard).unwrap();
        assert_eq!(
            keyboard.manufacturer, None,
            "\"(Standard keyboards)\" isn't a maker"
        );
        assert!(
            !devices
                .iter()
                .find(|d| d.kind == Kind::Camera)
                .unwrap()
                .is_ok()
        );
    }

    #[test]
    fn a_single_device_and_nothing_at_all() {
        let one = r#"{"FriendlyName":"Dell U2720Q","Class":"Monitor","Status":"OK","Manufacturer":"Dell","InstanceId":"DISPLAY\\DEL"}"#;
        assert_eq!(parse_pnp_json(one).unwrap()[0].kind, Kind::Display);
        assert!(parse_pnp_json("").unwrap().is_empty());
        assert!(parse_pnp_json("nonsense").is_err());
    }

    #[test]
    fn bluetooth_details_read_nicely() {
        assert_eq!(format_address(0x001A_7DDA_7113), "00:1A:7D:DA:71:13");
        assert_eq!(bluetooth_kind(0x240404), "Headset");
        assert_eq!(bluetooth_kind(0x240418), "Speaker");
        assert_eq!(bluetooth_kind(0x002540), "Keyboard");
        assert_eq!(bluetooth_kind(0x002580), "Mouse");
        assert_eq!(bluetooth_kind(0x002508), "Controller");
        assert_eq!(bluetooth_kind(0x5a020c), "Phone");
    }
}
