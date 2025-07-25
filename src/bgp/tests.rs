// Include all test modules
#[cfg(test)]
mod attributes_tests {
    use super::super::attributes::*;
    use super::super::nlri::*;
    use super::super::types::*;
    use std::net::Ipv4Addr;
    include!("../bgp/attributes_tests.rs");
}

#[cfg(test)]
mod capabilities_tests {
    use super::super::capabilities::*;
    use super::super::types::*;
    include!("../bgp/capabilities_tests.rs");
}

#[cfg(test)]
mod messages_tests {
    use super::super::attributes::*;
    use super::super::messages::*;
    use super::super::nlri::*;
    use super::super::types::*;
    use crate::neighbor::Capabilities;
    use std::net::Ipv4Addr;
    include!("../bgp/messages_tests.rs");
}

#[cfg(test)]
mod nlri_tests {
    use super::super::nlri::*;
    use super::super::types::*;
    use ipnet::{IpNet, Ipv4Net, Ipv6Net};
    use std::hash::Hash;
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
    include!("../bgp/nlri_tests.rs");
}

#[cfg(test)]
mod types_tests {
    use super::super::types::*;
    use num_traits::FromPrimitive;
    use std::collections::HashSet;
    include!("../bgp/types_tests.rs");
}
