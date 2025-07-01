#[tokio::test]
async fn gobgp_routes_reachable_from_frr() {
    let client = reqwest::Client::new();

    let gobgp_prefixes = [
        "11.11.11.0/24",
        "2001:100::/24",
        "2001:123:123:2::/64"
    ];

    let resp = Command::new("docker")
        .args(&["exec", "ubgp", "ubgpc", "--server", "127.0.0.1", "rib", "-a", "1", "-s", "1"])
        .output()
        .expect("failed to run ubgpc");

    let output = String::from_utf8(resp.stdout).unwrap();

    for prefix in gobgp_prefixes {
        assert!(
            output.contains(prefix),
            "Missing advertised route {} in RIB",
            prefix
        );
    }
}
