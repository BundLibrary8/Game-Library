//! "Can I run it?": this PC's specs, and a game's system requirements.
//!
//! Requirements come from the Steam Store as free text written by each
//! publisher, so only the parts that are reliably numbers (memory and
//! disk space) are checked automatically. Processor and graphics card
//! are shown side by side for you to compare by eye, because there is no
//! dependable way to rank one chip against another from its name.

use std::path::Path;

use anyhow::{Context, Result};

/// What this PC has.
#[derive(Debug, Clone, PartialEq)]
pub struct PcSpecs {
    pub os: String,
    pub cpu: String,
    pub cores: usize,
    pub ram_gb: f64,
    /// The graphics card's name, where it could be found (Windows only).
    pub gpu: Option<String>,
}

const GB: f64 = 1024.0 * 1024.0 * 1024.0;

/// Read this PC's specs. Takes a moment, so do it once and keep the result.
pub fn detect_specs() -> Result<PcSpecs> {
    let mut system = sysinfo::System::new().context("couldn't read this PC's specs")?;
    system.refresh_memory();
    system.refresh_cpu_all();
    let cpu = system
        .cpus()
        .first()
        .map(|cpu| cpu.brand().trim().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "Unknown processor".to_string());
    Ok(PcSpecs {
        os: sysinfo::System::long_os_version().unwrap_or_else(|_| "Unknown".to_string()),
        cpu,
        cores: system.cpus().len(),
        ram_gb: system.total_memory() as f64 / GB,
        gpu: detect_gpu(),
    })
}

/// Ask Windows for the graphics card's name.
#[cfg(windows)]
fn detect_gpu() -> Option<String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000; // don't flash a console window
    let output = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            "(Get-CimInstance Win32_VideoController | Select-Object -First 1 -ExpandProperty Name)",
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;
    let name = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!name.is_empty()).then_some(name)
}

#[cfg(not(windows))]
fn detect_gpu() -> Option<String> {
    None
}

/// Free space on the drive that holds `path`, in GB.
pub fn free_space_gb(path: &Path) -> Option<f64> {
    let disks = sysinfo::Disks::new_with_refreshed_list().ok()?;
    let path = crate::paths::real_path(path);
    // The drive whose mount point is the longest match for the path.
    disks
        .iter()
        .filter(|disk| crate::paths::is_inside(&path, disk.mount_point()))
        .max_by_key(|disk| disk.mount_point().as_os_str().len())
        .map(|disk| disk.available_space() as f64 / GB)
}

/// Turn the store's HTML requirements into plain lines of text.
pub fn html_to_text(html: &str) -> String {
    let mut text = String::new();
    let mut in_tag = false;
    let mut tag = String::new();
    for c in html.chars() {
        match c {
            '<' => {
                in_tag = true;
                tag.clear();
            }
            '>' if in_tag => {
                in_tag = false;
                let name = tag
                    .trim_start_matches('/')
                    .split_whitespace()
                    .next()
                    .unwrap_or("")
                    .to_lowercase();
                // These tags end a line.
                if matches!(name.as_str(), "br" | "br/" | "li" | "p" | "ul" | "div") {
                    text.push('\n');
                }
            }
            c if in_tag => tag.push(c),
            c => text.push(c),
        }
    }
    let text = text
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&nbsp;", " ")
        .replace('\u{a0}', " ");
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// The line of a requirements text that starts with one of these labels,
/// e.g. `line_for(text, &["Memory"])` → "8 GB RAM".
pub fn line_for<'a>(requirements: &'a str, labels: &[&str]) -> Option<&'a str> {
    requirements.lines().find_map(|line| {
        let (label, value) = line.split_once(':')?;
        let label = label.trim().to_lowercase();
        labels
            .iter()
            .any(|wanted| label == wanted.to_lowercase())
            .then_some(value.trim())
    })
}

/// Read an amount like "8 GB", "8GB RAM", "512 MB" or "1.5 TB" as GB.
pub fn parse_gb(text: &str) -> Option<f64> {
    let lower = text.to_lowercase();
    let chars: Vec<char> = lower.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i].is_ascii_digit() {
            let start = i;
            while i < chars.len()
                && (chars[i].is_ascii_digit() || chars[i] == '.' || chars[i] == ',')
            {
                i += 1;
            }
            let number: String = chars[start..i].iter().collect::<String>().replace(',', ".");
            let rest: String = chars[i..].iter().collect::<String>();
            let unit = rest.trim_start();
            let scale = if unit.starts_with("gb") || unit.starts_with("gib") {
                Some(1.0)
            } else if unit.starts_with("mb") || unit.starts_with("mib") {
                Some(1.0 / 1024.0)
            } else if unit.starts_with("tb") {
                Some(1024.0)
            } else {
                None
            };
            if let (Ok(value), Some(scale)) = (number.parse::<f64>(), scale) {
                return Some(value * scale);
            }
        } else {
            i += 1;
        }
    }
    None
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Pass,
    Fail,
    /// Can't be judged automatically: compare the two by eye.
    Compare,
}

/// One row of the "can I run it" table.
#[derive(Debug, Clone, PartialEq)]
pub struct Check {
    pub part: &'static str,
    pub needed: String,
    pub yours: String,
    pub verdict: Verdict,
}

/// Compare a game's requirements text with this PC. `free_gb` is the free
/// space on the drive the game would use, if known. Only rows the
/// requirements actually mention are returned.
pub fn check_requirements(
    requirements: &str,
    specs: &PcSpecs,
    free_gb: Option<f64>,
    installed: bool,
) -> Vec<Check> {
    let mut checks = Vec::new();
    if let Some(needed) = line_for(requirements, &["OS", "OS *", "Operating System"]) {
        checks.push(Check {
            part: "System",
            needed: needed.to_string(),
            yours: specs.os.clone(),
            verdict: Verdict::Compare,
        });
    }
    if let Some(needed) = line_for(requirements, &["Processor", "CPU"]) {
        checks.push(Check {
            part: "Processor",
            needed: needed.to_string(),
            yours: format!("{} ({} threads)", specs.cpu, specs.cores),
            verdict: Verdict::Compare,
        });
    }
    if let Some(needed) = line_for(requirements, &["Memory", "RAM"]) {
        // PCs report slightly less than they have fitted (a 16 GB machine
        // says about 15.8), so allow a little slack.
        let verdict = match parse_gb(needed) {
            Some(gb) if specs.ram_gb + 0.6 >= gb => Verdict::Pass,
            Some(_) => Verdict::Fail,
            None => Verdict::Compare,
        };
        checks.push(Check {
            part: "Memory",
            needed: needed.to_string(),
            yours: format!("{:.0} GB RAM", specs.ram_gb.round()),
            verdict,
        });
    }
    if let Some(needed) = line_for(requirements, &["Graphics", "Video Card", "GPU"]) {
        checks.push(Check {
            part: "Graphics",
            needed: needed.to_string(),
            yours: specs
                .gpu
                .clone()
                .unwrap_or_else(|| "Not detected".to_string()),
            verdict: Verdict::Compare,
        });
    }
    if let Some(needed) = line_for(requirements, &["Storage", "Hard Drive", "Hard Disk Space"]) {
        let (yours, verdict) = if installed {
            ("Already installed".to_string(), Verdict::Pass)
        } else {
            match (parse_gb(needed), free_gb) {
                (Some(gb), Some(free)) => (
                    format!("{free:.0} GB free"),
                    if free >= gb {
                        Verdict::Pass
                    } else {
                        Verdict::Fail
                    },
                ),
                (_, Some(free)) => (format!("{free:.0} GB free"), Verdict::Compare),
                _ => ("Unknown".to_string(), Verdict::Compare),
            }
        };
        checks.push(Check {
            part: "Storage",
            needed: needed.to_string(),
            yours,
            verdict,
        });
    }
    checks
}

#[cfg(test)]
mod tests {
    use super::*;

    const HTML: &str = "<strong>Minimum:</strong><br><ul class=\"bb_ul\"><li>Requires a 64-bit processor and operating system<br></li><li><strong>OS:</strong> Windows 10 64-bit<br></li><li><strong>Processor:</strong> Intel Core i5-6600K &amp; up<br></li><li><strong>Memory:</strong> 8 GB RAM<br></li><li><strong>Graphics:</strong> NVIDIA GeForce GTX 1060<br></li><li><strong>Storage:</strong> 70 GB available space</li></ul>";

    fn specs(ram_gb: f64) -> PcSpecs {
        PcSpecs {
            os: "Windows 11".into(),
            cpu: "AMD Ryzen 5 5600X".into(),
            cores: 12,
            ram_gb,
            gpu: Some("NVIDIA GeForce RTX 3060".into()),
        }
    }

    #[test]
    fn html_becomes_plain_lines() {
        let text = html_to_text(HTML);
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines[0], "Minimum:");
        assert!(
            lines.contains(&"Processor: Intel Core i5-6600K & up"),
            "{lines:?}"
        );
        assert!(lines.contains(&"Memory: 8 GB RAM"));
        assert!(!text.contains('<'));
    }

    #[test]
    fn finds_the_line_for_a_part() {
        let text = html_to_text(HTML);
        assert_eq!(line_for(&text, &["Memory"]), Some("8 GB RAM"));
        assert_eq!(
            line_for(&text, &["Storage", "Hard Drive"]),
            Some("70 GB available space")
        );
        assert_eq!(line_for(&text, &["Sound Card"]), None);
    }

    #[test]
    fn reads_amounts() {
        assert_eq!(parse_gb("8 GB RAM"), Some(8.0));
        assert_eq!(parse_gb("16GB"), Some(16.0));
        assert_eq!(parse_gb("512 MB RAM"), Some(0.5));
        assert_eq!(parse_gb("1.5 TB available space"), Some(1536.0));
        assert_eq!(parse_gb("4 GB RAM (8 GB recommended)"), Some(4.0));
        assert_eq!(
            parse_gb("DirectX 11 with 2 GB VRAM"),
            Some(2.0),
            "skips numbers with no unit"
        );
        assert_eq!(parse_gb("Lots"), None);
    }

    #[test]
    fn memory_and_storage_are_checked_and_the_rest_is_for_comparing() {
        let text = html_to_text(HTML);
        let checks = check_requirements(&text, &specs(15.8), Some(120.0), false);
        let verdict = |part: &str| checks.iter().find(|c| c.part == part).unwrap().verdict;
        assert_eq!(checks.len(), 5);
        assert_eq!(verdict("Memory"), Verdict::Pass);
        assert_eq!(verdict("Storage"), Verdict::Pass);
        assert_eq!(verdict("Processor"), Verdict::Compare);
        assert_eq!(verdict("Graphics"), Verdict::Compare);

        let small = check_requirements(&text, &specs(4.0), Some(20.0), false);
        let verdict = |part: &str| small.iter().find(|c| c.part == part).unwrap().verdict;
        assert_eq!(verdict("Memory"), Verdict::Fail);
        assert_eq!(verdict("Storage"), Verdict::Fail);
    }

    #[test]
    fn a_sixteen_gb_pc_reporting_slightly_less_still_passes() {
        let checks = check_requirements("Memory: 16 GB RAM", &specs(15.6), None, false);
        assert_eq!(checks[0].verdict, Verdict::Pass);
        assert_eq!(checks[0].yours, "16 GB RAM");
    }

    #[test]
    fn an_installed_game_needs_no_more_space() {
        let checks = check_requirements(
            "Storage: 500 GB available space",
            &specs(16.0),
            Some(1.0),
            true,
        );
        assert_eq!(checks[0].verdict, Verdict::Pass);
    }

    #[test]
    fn this_pc_has_specs() {
        let specs = detect_specs().unwrap();
        assert!(specs.cores >= 1);
        assert!(specs.ram_gb > 0.1);
        assert!(!specs.cpu.is_empty());
        assert!(free_space_gb(Path::new("/")).is_some() || cfg!(windows));
    }
}
