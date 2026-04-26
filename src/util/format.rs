use crate::model::types::UnitsMode;

pub fn fmt_rate(bytes_per_sec: f64, mode: UnitsMode) -> String {
    if !bytes_per_sec.is_finite() || bytes_per_sec < 0.0 {
        return "-".to_string();
    }
    let mib = bytes_per_sec / (1024.0 * 1024.0);
    let gib = bytes_per_sec / (1024.0 * 1024.0 * 1024.0);
    let tib = bytes_per_sec / (1024.0 * 1024.0 * 1024.0 * 1024.0);
    match mode {
        UnitsMode::MiB => format!("{mib:.1} MiB/s"),
        UnitsMode::GiB => format!("{gib:.2} GiB/s"),
        UnitsMode::TiB => format!("{tib:.3} TiB/s"),
        UnitsMode::Auto => {
            if tib >= 1.0 {
                format!("{tib:.3} TiB/s")
            } else if gib >= 1.0 {
                format!("{gib:.2} GiB/s")
            } else {
                format!("{mib:.1} MiB/s")
            }
        }
    }
}

pub fn fmt_ms(v: Option<f64>) -> String {
    match v {
        Some(ms) if ms.is_finite() => format!("{ms:.2}"),
        _ => "-".to_string(),
    }
}

pub fn fmt_bytes(bytes: f64) -> String {
    if !bytes.is_finite() || bytes < 0.0 {
        return "-".to_string();
    }
    let kib = bytes / 1024.0;
    let mib = bytes / (1024.0 * 1024.0);
    let gib = bytes / (1024.0 * 1024.0 * 1024.0);
    let tib = bytes / (1024.0 * 1024.0 * 1024.0 * 1024.0);
    if tib >= 1.0 {
        format!("{tib:.2} TiB")
    } else if gib >= 1.0 {
        format!("{gib:.2} GiB")
    } else if mib >= 1.0 {
        format!("{mib:.1} MiB")
    } else if kib >= 1.0 {
        format!("{kib:.1} KiB")
    } else {
        format!("{bytes:.0} B")
    }
}
