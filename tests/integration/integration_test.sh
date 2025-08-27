#!/bin/bash

set -e
sleep 5

expected_prefixes=("10.66.0.0/16" "66.66.66.0/24")

# Test ubgp receives routes
output=$(docker exec integration_ubgp_1 ubgpc --server 127.0.0.1 rib -a 1 -s 1)
for prefix in "${expected_prefixes[@]}"; do
    echo "$output" | grep -q "$prefix" || { echo "ERROR: Missing $prefix in ubgp RIB"; exit 1; }
done

# Test routes from each peer
echo "$output" | grep -q "AS 123" || { echo "ERROR: No routes from gobgp"; exit 1; }
echo "$output" | grep -q "AS 666" || { echo "ERROR: No routes from frr"; exit 1; }

# Test BGP sessions
neighbor_output=$(docker exec integration_ubgp_1 ubgpc --server 127.0.0.1 neighbors)
echo "$neighbor_output" | grep -qi "established" || { echo "ERROR: BGP sessions not established"; exit 1; }

# Test gobgp receives frr routes
gobgp_rib=$(docker exec integration_gobgp_1 gobgp global rib)
for prefix in "${expected_prefixes[@]}"; do
    echo "$gobgp_rib" | grep -q "$prefix" || { echo "ERROR: Missing $prefix in gobgp RIB"; exit 1; }
done
echo "$gobgp_rib" | grep -q "666" || { echo "ERROR: No AS 666 routes in gobgp"; exit 1; }

# Test frr receives gobgp routes  
frr_rib=$(docker exec integration_frr_1 vtysh -c "show ip bgp")
for prefix in "${expected_prefixes[@]}"; do
    echo "$frr_rib" | grep -q "$prefix" || { echo "ERROR: Missing $prefix in frr RIB"; exit 1; }
done
echo "$frr_rib" | grep -q "123" || { echo "ERROR: No AS 123 routes in frr"; exit 1; }

echo "All tests passed"