pub fn host_from_device(device: &str) -> Option<&str> {
    device.split(':').next().filter(|s| !s.is_empty())
}
