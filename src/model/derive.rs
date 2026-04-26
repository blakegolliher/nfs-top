use crate::model::types::MountCounters;

pub fn host_from_device(device: &str) -> Option<&str> {
    device.split(':').next().filter(|s| !s.is_empty())
}

pub fn read_write_calls(m: &MountCounters) -> (u64, u64) {
    let read = m.ops.get("READ").map(|o| o.calls).unwrap_or(0);
    let write = m.ops.get("WRITE").map(|o| o.calls).unwrap_or(0);
    (read, write)
}
