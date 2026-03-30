# ubgpd

A lightweight BGP daemon written in Rust. ubgpd implements the BGP-4 protocol with support for IPv4 and IPv6 address families, a gRPC management API, and Linux FIB integration via netlink.

## Implemented Features

- BGP-4 protocol with full finite state machine ([RFC 4271](https://datatracker.ietf.org/doc/html/rfc4271))
- Multiprotocol Extensions for IPv4 and IPv6 unicast ([RFC 4760](https://datatracker.ietf.org/doc/html/rfc4760))
- Capabilities advertisement with optional parameter negotiation ([RFC 5492](https://datatracker.ietf.org/doc/html/rfc5492))
- RIB with best path selection
- FIB integration with the Linux kernel routing table via netlink
- gRPC API for querying neighbors and RIB state

## State

ubgpd is a personal side project with the explicit purpose of serving a learning environment for the Rust programming language. It is not feature complete as of today and may never be.

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

## To dos

- Many FSM state transition are missing
- IGP metric comparison for iBGP paths not implemented
- Route deletion in FIB is not implemented
- gRPC RIB response only returns prefixes, not attributes
- Finish IPv6 MP_REACH_NLRI handling to enable transit fucntionality

## License

Licensed under either of

- Apache License, Version 2.0
  ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license
  ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

## Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.

See [CONTRIBUTING.md](CONTRIBUTING.md).
