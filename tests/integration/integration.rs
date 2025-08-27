use anyhow::Context;
use std::process::Command;
use std::sync::Once;

#[tokio::test]
async fn ubgp_receives_gobgp_routes() -> anyhow::Result<()> {
    // Wait for BGP sessions to establish and exchange routes
    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

    // These are the actual prefixes being advertised (from test output)
    let gobgp_prefixes = [
        "10.66.0.0/16",
        "66.66.66.0/24",
    ];

    let resp = Command::new("docker")
        .args(&["exec", "integration_ubgp_1", "ubgpc", "--server", "127.0.0.1", "rib", "-a", "1", "-s", "1"])
        .output()
        .context("Failed to query ubgp RIB via ubgpc")?;

    if !resp.status.success() {
        panic!(
            "ubgpc command failed with exit code {:?}\nstderr: {}",
            resp.status.code(),
            String::from_utf8_lossy(&resp.stderr)
        );
    }

    let output = String::from_utf8(resp.stdout)
        .context("Invalid UTF-8 in ubgpc output")?;

    println!("ubgp RIB contents:\n{}", output);

    for prefix in gobgp_prefixes {
        assert!(
            output.contains(prefix),
            "Missing expected route {} in ubgp RIB. Full RIB:\n{}",
            prefix,
            output
        );
    }
    
    Ok(())
}

#[tokio::test] 
async fn bgp_sessions_established() -> anyhow::Result<()> {
    // Wait for BGP sessions to establish
    tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

    // Test that ubgp has established BGP sessions with both peers
    let resp = Command::new("docker")
        .args(&["exec", "integration_ubgp_1", "ubgpc", "--server", "127.0.0.1", "neighbors"])
        .output()
        .context("Failed to query ubgp neighbors")?;

    if !resp.status.success() {
        panic!(
            "ubgpc neighbor command failed with exit code {:?}\nstderr: {}",
            resp.status.code(),
            String::from_utf8_lossy(&resp.stderr)
        );
    }

    let output = String::from_utf8(resp.stdout)
        .context("Invalid UTF-8 in ubgpc neighbor output")?;

    println!("ubgp neighbor status:\n{}", output);

    // Check that we have established sessions
    assert!(
        output.contains("Established") || output.contains("established"), 
        "Expected established BGP sessions in neighbor output:\n{}", 
        output
    );
    
    Ok(())
}
