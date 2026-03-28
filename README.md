# ubgpd

BGP daemon in Rust. Handles basic peering, UPDATE processing, IPv4/IPv6 unicast, netlink FIB sync and gRPC/CLI.

Not production-ready -- missing parts of the FSM and full FIB deletion handling.

## Build

Needs Rust 1.94+ and `protoc`:

```sh
apt install protobuf-compiler
cargo build --release
```

Produces `ubgpd` (daemon) and `ubgpc` (CLI).

## Config

Default path: `./ubgpd.conf`. See the sample config in the repo for all options.

```toml
asn = 42
rid = "2.2.2.2"
port = 179

[[families]]
afi = "Ipv4"
safi = "NLRIUnicast"

[[neighbors]]
asn = 123
ip = "192.168.122.225"
port = 179
```

## Usage

```sh
RUST_LOG=info ubgpd --config ubgpd.conf
ubgpc neighbors
ubgpc rib --afi 1 --safi 1
```

## Tests

```sh
cargo test --all
```

Integration tests:

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
