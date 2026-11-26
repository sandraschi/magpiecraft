//! Drive health: which physical disks the libraries sit on, and whether any
//! of them look sick. The probe is the Windows Storage API through
//! `magpiecraft-tools` (invoked, never linked). USB enclosures usually report
//! no counters; those read as unknown, never as zero. smartctl is used when
//! present in a later slice; its absence is reported, not hidden.

use serde::{Deserialize, Serialize};

/// One physical disk from the storage probe.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DriveInfo {
    pub id: String,
    pub name: String,
    pub media: String,
    pub size_bytes: Option<u64>,
    pub health: String,
    pub status: String,
    pub wear_percent: Option<u64>,
    pub temp_c: Option<i64>,
    pub temp_max_c: Option<i64>,
    pub read_errors: Option<u64>,
    pub write_errors: Option<u64>,
    pub power_on_hours: Option<u64>,
    /// Null when the enclosure reports no counters at all.
    pub counters_known: bool,
}

/// One mounted volume from the storage probe.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VolumeInfo {
    pub letter: String,
    pub label: String,
    pub size_bytes: Option<u64>,
    pub free_bytes: Option<u64>,
    pub disk: Option<String>,
}

/// A library root resolved to its volume and drive, or left unmapped.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LibraryDrive {
    pub library: String,
    pub root: String,
    pub volume: Option<String>,
    pub drive: Option<String>,
}

/// The full report: drives, volumes, library mapping and warnings.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DriveHealthReport {
    pub drives: Vec<DriveInfo>,
    pub volumes: Vec<VolumeInfo>,
    pub libraries: Vec<LibraryDrive>,
    pub warnings: Vec<String>,
    pub source: String,
}

fn unsigned(value: &serde_json::Value, key: &str) -> Option<u64> {
    value.get(key)?.as_u64()
}

fn signed(value: &serde_json::Value, key: &str) -> Option<i64> {
    value.get(key)?.as_i64()
}

fn text(value: &serde_json::Value, key: &str) -> String {
    value
        .get(key)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

/// Parse the storage probe JSON. Pure: unit-tested on recorded output.
pub fn parse_layout(
    layout: &serde_json::Value,
    libraries: &[(String, String)],
    source: &str,
) -> DriveHealthReport {
    let mut drives = Vec::new();
    if let Some(list) = layout.get("disks").and_then(|d| d.as_array()) {
        for d in list {
            let counters_known = d.get("wear").is_some_and(|v| !v.is_null())
                || d.get("temp").is_some_and(|v| !v.is_null())
                || d.get("hours").is_some_and(|v| !v.is_null());
            drives.push(DriveInfo {
                id: text(d, "id"),
                name: text(d, "name"),
                media: text(d, "media"),
                size_bytes: unsigned(d, "size"),
                health: text(d, "health"),
                status: text(d, "status"),
                wear_percent: unsigned(d, "wear"),
                temp_c: signed(d, "temp"),
                temp_max_c: signed(d, "temp_max"),
                read_errors: unsigned(d, "read_err"),
                write_errors: unsigned(d, "write_err"),
                power_on_hours: unsigned(d, "hours"),
                counters_known,
            });
        }
    }
    let mut volumes = Vec::new();
    if let Some(list) = layout.get("volumes").and_then(|v| v.as_array()) {
        for v in list {
            volumes.push(VolumeInfo {
                letter: text(v, "letter"),
                label: text(v, "label"),
                size_bytes: unsigned(v, "size"),
                free_bytes: unsigned(v, "free"),
                disk: v
                    .get("disk")
                    .and_then(|d| d.as_u64())
                    .map(|n| n.to_string()),
            });
        }
    }
    drives.sort_by(|a, b| a.id.cmp(&b.id));
    volumes.sort_by(|a, b| a.letter.cmp(&b.letter));
    let libraries = libraries
        .iter()
        .map(|(name, root)| {
            let letter = drive_letter(root);
            let disk = letter.as_ref().and_then(|l| {
                volumes
                    .iter()
                    .find(|v| v.letter.eq_ignore_ascii_case(l))
                    .and_then(|v| v.disk.clone())
            });
            let drive = disk
                .as_ref()
                .and_then(|id| drives.iter().find(|d| &d.id == id).map(|d| d.name.clone()));
            LibraryDrive {
                library: name.clone(),
                root: root.clone(),
                volume: letter,
                drive,
            }
        })
        .collect::<Vec<_>>();
    let mut warnings = Vec::new();
    for drive in &drives {
        if !drive.health.eq_ignore_ascii_case("healthy") || !drive.status.eq_ignore_ascii_case("OK")
        {
            warnings.push(format!(
                "{} reports {} / {}",
                drive.name, drive.health, drive.status
            ));
        }
        if let Some(wear) = drive.wear_percent {
            if wear >= 90 {
                warnings.push(format!(
                    "{} is at {wear}% wear; replace it before it fails",
                    drive.name
                ));
            }
        }
        if let Some(temp) = drive.temp_c {
            if temp >= 60 {
                warnings.push(format!("{} is at {temp}C; check cooling", drive.name));
            }
        }
        for (label, count) in [("read", drive.read_errors), ("write", drive.write_errors)] {
            if let Some(n) = count {
                if n > 0 {
                    warnings.push(format!(
                        "{} reports {n} {label} errors; verify its libraries",
                        drive.name
                    ));
                }
            }
        }
    }
    for volume in &volumes {
        if let (Some(size), Some(free)) = (volume.size_bytes, volume.free_bytes) {
            if let Some(pct) = free
                .checked_mul(100)
                .and_then(|f| f.checked_div(size.max(1)))
            {
                if pct < 5 || free < 10_000_000_000 {
                    let label = if volume.label.is_empty() {
                        volume.letter.clone()
                    } else {
                        format!("{} ({})", volume.label, volume.letter)
                    };
                    let full = if free < 2_000_000_000 {
                        "nearly full"
                    } else {
                        "getting full"
                    };
                    warnings.push(format!(
                        "volume {label} is {full} ({:.1} GB free of {:.1} GB)",
                        free as f64 / 1_000_000_000.0,
                        size as f64 / 1_000_000_000.0
                    ));
                }
            }
        }
    }
    for lib in libraries.iter().filter(|l| l.drive.is_none()) {
        warnings.push(format!(
            "library {} sits outside any known drive ({})",
            lib.library, lib.root
        ));
    }
    DriveHealthReport {
        drives,
        volumes,
        libraries,
        warnings,
        source: source.to_string(),
    }
}

/// `E:\Media` to `E`. UNC paths and relative roots have no drive letter.
fn drive_letter(root: &str) -> Option<String> {
    let mut chars = root.chars();
    let letter = chars.next()?;
    if !letter.is_ascii_alphabetic() {
        return None;
    }
    if chars.next() == Some(':') {
        Some(letter.to_ascii_uppercase().to_string())
    } else {
        None
    }
}

/// Probe the machine and map library roots to drives.
pub fn drive_health(
    libraries: &[(String, String)],
) -> Result<DriveHealthReport, crate::HealthError> {
    let source = if magpiecraft_tools::which("smartctl").is_some() {
        "windows-storage-api (smartctl present, used in a later slice)"
    } else {
        "windows-storage-api (smartctl absent)"
    };
    let layout = magpiecraft_tools::storage_layout()
        .map_err(|e| crate::HealthError::Manifest(format!("storage probe: {e}")))?;
    Ok(parse_layout(&layout, libraries, source))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    /// Recorded live output (Goliath, 2026-11-26), trimmed to two disks.
    const RECORDED: &str = r#"{"volumes":[{"letter":"E","label":"Anime","size":5033164795904,"free":19587534848,"disk":2},{"letter":"C","label":"","size":1999516987392,"free":99023724544,"disk":1}],"disks":[{"id":"2","name":"WD Elements 25A3","media":"HDD","size":18000174383104,"health":"Healthy","status":"OK","wear":null,"temp":null,"temp_max":null,"read_err":null,"write_err":null,"hours":null},{"id":"1","name":"CT2000P3PSSD8","media":"SSD","size":2000398934016,"health":"Unhealthy","status":"OK","wear":95,"temp":64,"temp_max":70,"read_err":12,"write_err":0,"hours":18000}]}"#;

    fn recorded() -> serde_json::Value {
        serde_json::from_str(RECORDED).unwrap()
    }

    #[test]
    fn recorded_layout_parses_and_maps() {
        let report = parse_layout(
            &recorded(),
            &[("Anime".to_string(), "E:\\Anime".to_string())],
            "test",
        );
        assert_eq!(report.drives.len(), 2);
        assert_eq!(report.volumes.len(), 2);
        let usb = report.drives.iter().find(|d| d.id == "2").unwrap();
        assert!(!usb.counters_known);
        assert_eq!(usb.wear_percent, None);
        let lib = &report.libraries[0];
        assert_eq!(lib.volume.as_deref(), Some("E"));
        assert_eq!(lib.drive.as_deref(), Some("WD Elements 25A3"));
    }

    #[test]
    fn warnings_fire_on_health_wear_heat_errors_and_full_disks() {
        let report = parse_layout(&recorded(), &[], "test");
        let joined = report.warnings.join("\n");
        assert!(joined.contains("Unhealthy"), "health: {joined}");
        assert!(joined.contains("95% wear"), "wear: {joined}");
        assert!(joined.contains("64C"), "heat: {joined}");
        assert!(joined.contains("12 read errors"), "errors: {joined}");
        assert!(joined.contains("getting full"), "space: {joined}");
        // No false positive on the healthy USB disk itself.
        assert!(!joined.contains("WD Elements"));
        // Under 2 GB free escalates to nearly full.
        let tiny = serde_json::json!({
            "volumes": [{"letter": "Z", "label": "", "size": 5000000000u64, "free": 100000000u64, "disk": 9}],
            "disks": []
        });
        let tight = parse_layout(&tiny, &[], "test");
        assert!(
            tight.warnings.iter().any(|w| w.contains("nearly full")),
            "tiny: {}",
            tight.warnings.join("\n")
        );
    }

    #[test]
    fn unmapped_roots_warn_instead_of_silence() {
        let report = parse_layout(
            &recorded(),
            &[("Odd".to_string(), "\\\\nas\\share".to_string())],
            "test",
        );
        assert!(report.libraries[0].drive.is_none());
        assert!(report.warnings.iter().any(|w| w.contains("Odd")));
    }

    #[test]
    fn drive_letter_parsing_is_strict() {
        assert_eq!(drive_letter("E:\\Media").as_deref(), Some("E"));
        assert_eq!(drive_letter("e:/x").as_deref(), Some("E"));
        assert_eq!(drive_letter("\\\\nas\\share"), None);
        assert_eq!(drive_letter("relative/path"), None);
        assert_eq!(drive_letter(""), None);
    }
}
