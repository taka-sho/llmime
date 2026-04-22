use std::path::Path;

const SAFETY_FACTOR: f64 = 1.2;
const WARNING_FACTOR: f64 = 1.5;

#[derive(Debug, PartialEq)]
pub enum MemoryCheckResult {
    Ok,
    Warning { available_gb: f64, required_gb: f64 },
    Insufficient { available_gb: f64, required_gb: f64 },
}

pub fn check_memory_for_model(model_path: &Path) -> MemoryCheckResult {
    let required_gb = estimate_required_gb(model_path);
    let available_gb = match get_available_ram_gb() {
        Some(v) => v,
        None => return MemoryCheckResult::Ok,
    };

    if available_gb < required_gb {
        MemoryCheckResult::Insufficient {
            available_gb,
            required_gb,
        }
    } else if available_gb < required_gb * WARNING_FACTOR {
        MemoryCheckResult::Warning {
            available_gb,
            required_gb,
        }
    } else {
        MemoryCheckResult::Ok
    }
}

fn estimate_required_gb(model_path: &Path) -> f64 {
    match std::fs::metadata(model_path) {
        Ok(meta) => (meta.len() as f64 / 1_073_741_824.0) * SAFETY_FACTOR,
        Err(_) => 2.0, // conservative default when file not found
    }
}

#[cfg(target_os = "macos")]
fn get_available_ram_gb() -> Option<f64> {
    use std::process::Command;

    let page_size = get_page_size()?;

    let output = Command::new("vm_stat").output().ok()?;
    let text = String::from_utf8_lossy(&output.stdout);

    let mut free_pages: u64 = 0;
    let mut inactive_pages: u64 = 0;

    for line in text.lines() {
        if line.starts_with("Pages free:") {
            free_pages = parse_vm_stat_value(line)?;
        } else if line.starts_with("Pages inactive:") {
            inactive_pages = parse_vm_stat_value(line)?;
        }
    }

    let available_bytes = (free_pages + inactive_pages) * page_size;
    Some(available_bytes as f64 / 1_073_741_824.0)
}

#[cfg(target_os = "macos")]
fn get_page_size() -> Option<u64> {
    use std::process::Command;
    let output = Command::new("sysctl")
        .arg("-n")
        .arg("hw.pagesize")
        .output()
        .ok()?;
    let s = String::from_utf8_lossy(&output.stdout);
    s.trim().parse::<u64>().ok()
}

#[cfg(target_os = "macos")]
fn parse_vm_stat_value(line: &str) -> Option<u64> {
    // "Pages free:   12345." — strip trailing dot and whitespace
    line.split(':')
        .nth(1)?
        .trim()
        .trim_end_matches('.')
        .parse::<u64>()
        .ok()
}

#[cfg(not(target_os = "macos"))]
fn get_available_ram_gb() -> Option<f64> {
    None
}

pub fn show_memory_warning_dialog(available_gb: f64, required_gb: f64) -> bool {
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        let script = format!(
            "display dialog \"空きRAMが不足しています。\\n利用可能: {:.1} GB / 必要: {:.1} GB\\n\\nモデルのロードを続行しますか?\" \
             buttons {{\"キャンセル\", \"続行\"}} default button \"続行\" with icon caution",
            available_gb, required_gb
        );
        let output = Command::new("osascript").arg("-e").arg(&script).output();
        match output {
            Ok(out) => {
                let result = String::from_utf8_lossy(&out.stdout);
                result.contains("続行")
            }
            Err(_) => {
                // CLI fallback
                eprintln!(
                    "WARNING: 空きRAM不足 (利用可能: {:.1} GB, 必要: {:.1} GB)。続行します。",
                    available_gb, required_gb
                );
                true
            }
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        eprintln!(
            "WARNING: 空きRAM不足 (利用可能: {:.1} GB, 必要: {:.1} GB)。続行します。",
            available_gb, required_gb
        );
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn nonexistent_model_does_not_panic() {
        let result = check_memory_for_model(&PathBuf::from("/nonexistent/model.gguf"));
        // Should not panic; result depends on available RAM
        let _ = result;
    }

    #[test]
    fn insufficient_when_available_less_than_required() {
        let result = classify(0.5, 2.0);
        assert!(matches!(result, MemoryCheckResult::Insufficient { .. }));
    }

    #[test]
    fn warning_when_available_less_than_required_times_1_5() {
        let result = classify(2.5, 2.0);
        assert!(matches!(result, MemoryCheckResult::Warning { .. }));
    }

    #[test]
    fn ok_when_sufficient() {
        let result = classify(10.0, 2.0);
        assert_eq!(result, MemoryCheckResult::Ok);
    }

    fn classify(available_gb: f64, required_gb: f64) -> MemoryCheckResult {
        if available_gb < required_gb {
            MemoryCheckResult::Insufficient {
                available_gb,
                required_gb,
            }
        } else if available_gb < required_gb * WARNING_FACTOR {
            MemoryCheckResult::Warning {
                available_gb,
                required_gb,
            }
        } else {
            MemoryCheckResult::Ok
        }
    }
}
