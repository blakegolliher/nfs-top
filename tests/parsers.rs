use nfs_top::sampler::mountstats::parse_mountstats;
use nfs_top::util::parse::parse_tcp_hex_endpoint;

#[test]
fn mountstats_fixture_parses() {
    let s = include_str!("fixtures/mountstats_v41.txt");
    let mounts = parse_mountstats(s).expect("parse");
    assert_eq!(mounts.len(), 1);
    assert_eq!(mounts[0].nconnect, Some(4));
}

#[test]
fn tcp4_fixture_parses() {
    let (_, p) = parse_tcp_hex_endpoint("0201010A", "0801", false).expect("parse");
    assert_eq!(p, 2049);
}

#[test]
fn tcp6_fixture_parses() {
    // ::1 as printed by /proc/net/tcp6 on little-endian x86_64
    let (ip, p) = parse_tcp_hex_endpoint("00000000000000000000000001000000", "4E51", true).expect("parse");
    assert_eq!(ip.to_string(), "::1");
    assert_eq!(p, 20049);
}
