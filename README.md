# ubgpd

A lightweight BGP daemon written in Rust. ubgpd implements the BGP-4 protocol with support for IPv4 and IPv6 address families, a gRPC management API, and Linux FIB integration via netlink.

## Features

- BGP-4 protocol with full finite state machine ([RFC 4271](https://datatracker.ietf.org/doc/html/rfc4271))
  - OPEN, UPDATE, NOTIFICATION, KEEPALIVE messages
  - Path attributes: ORIGIN, AS_PATH, NEXT_HOP, LOCAL_PREF, MED, ATOMIC_AGGREGATE, AGGREGATOR
  - Hold timer and keepalive timer management
  - AS loop detection and best path selection
  - Notification error codes and subcodes
- Multiprotocol Extensions for IPv4 and IPv6 unicast ([RFC 4760](https://datatracker.ietf.org/doc/html/rfc4760))
  - MP_REACH_NLRI and MP_UNREACH_NLRI path attributes
- Capabilities advertisement with optional parameter negotiation ([RFC 5492](https://datatracker.ietf.org/doc/html/rfc5492))
  - Multiprotocol, 4-octet ASN, and other capability codes recognized
- RIB with best path selection
- FIB integration with the Linux kernel routing table via netlink
- gRPC API for querying neighbors and RIB state
- CLI client (`ubgpc`) for interacting with the daemon
- Configurable connection retry with exponential backoff and jitter
- TOML-based configuration

## Building

Requires the Rust toolchain (1.94+) and the Protocol Buffer compiler.

On Debian/Ubuntu:

```sh
apt install protobuf-compiler
```

Then build the project:

```sh
cargo build --release
```

This produces two binaries: `ubgpd` (the daemon) and `ubgpc` (the CLI client).

## Configuration

ubgpd reads a TOML configuration file (default: `./ubgpd.conf`).

```toml
asn = 42
rid = "2.2.2.2"
port = 179
localips = ["192.168.122.1"]

[[families]]
    afi = "Ipv4"
    safi = "NLRIUnicast"

[[families]]
    afi = "Ipv6"
    safi = "NLRIUnicast"

[[neighbors]]
    asn = 123
    ip = "192.168.122.225"
    port = 179
    hold_time = 3
    connect_retry = 5
    keepalive_interval = 1
```

### Global settings

| Parameter   | Description                          | Default        |
|-------------|--------------------------------------|----------------|
| `asn`       | Local Autonomous System Number       | required       |
| `rid`       | Router ID (IPv4 address format)      | required       |
| `port`      | BGP listen port                      | `179`          |
| `localips`  | Local IP addresses to bind           | `[::]:0`       |
| `hold_time` | BGP hold time in seconds             | `3`            |
| `families`  | Address families to support          | IPv4 Unicast   |

### Neighbor settings

| Parameter              | Description                           | Default  |
|------------------------|---------------------------------------|----------|
| `asn`                  | Peer ASN                              | required |
| `ip`                   | Peer IP address                       | required |
| `port`                 | Peer BGP port                         | required |
| `hold_time`            | Hold time override                    | global   |
| `families`             | Address families override             | global   |
| `connect_retry`        | Connection retry interval (seconds)   | `120`    |
| `keepalive_interval`   | Keepalive interval (seconds)          | `60`     |
| `max_retry_count`      | Max retry attempts before giving up   | unlimited|
| `exponential_backoff`  | Enable exponential backoff on retries | `false`  |

## Usage

Start the daemon:

```sh
ubgpd --config /path/to/ubgpd.conf
```

Logging is controlled via the `RUST_LOG` environment variable:

```sh
RUST_LOG=info ubgpd --config ubgpd.conf
```

### CLI client

The `ubgpc` client connects to the daemon's gRPC API (default: `127.0.0.1:50051`).

List neighbors:

```sh
ubgpc neighbors
ubgpc neighbors --address 192.168.122.225
```

Query the RIB:

```sh
ubgpc rib --afi 1 --safi 1    # IPv4 Unicast
ubgpc rib --afi 2 --safi 1    # IPv6 Unicast
```

Specify a remote server:

```sh
ubgpc --server 10.0.0.1 --port 50051 neighbors
```

## Testing

### Unit tests

```sh
cargo test --all
```

### Integration tests

Integration tests use Docker Compose to set up a topology with ubgpd, GoBGP, and FRRouting peers.

```sh
cargo build
docker-compose -f tests/integration/docker-compose-dev.yml up -d
tests/integration/integration_test.sh
docker-compose -f tests/integration/docker-compose-dev.yml down --volumes --remove-orphans
```

## License

Licensed under either of

 * Apache License, Version 2.0
   ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license
   ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

## Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.

See [CONTRIBUTING.md](CONTRIBUTING.md).
