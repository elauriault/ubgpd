// Include all test modules
#[cfg(test)]
mod attributes_tests {
    use super::super::attributes::*;
    include!("../bgp/attributes_tests.rs");
}

#[cfg(test)]
mod capabilities_tests {
    use super::super::capabilities::*;
    include!("../bgp/capabilities_tests.rs");
}

#[cfg(test)]
mod messages_tests {
    use super::super::messages::*;
    include!("../bgp/messages_tests.rs");
}

#[cfg(test)]
mod nlri_tests {
    use super::super::nlri::*;
    include!("../bgp/nlri_tests.rs");
}

#[cfg(test)]
mod types_tests {
    use super::super::types::*;
    include!("../bgp/types_tests.rs");
}
