//! Guards on `deployments/crosschain/*.json`.

use std::{collections::BTreeSet, fs, path::PathBuf};

use astrion_crosschain_types::{network, Environment};
use serde_json::Value;

const PROTOCOLS: [&str; 3] = ["aave-v3", "morpho-blue", "compound-v3"];
const USDBC_BASE: &str = "0xd9aaec86b65d86f6a7b5b1b0c42ffa531710b6ca";

fn manifest(name: &str) -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../deployments/crosschain")
        .join(format!("{name}.json"));
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

/// Every string anywhere in the document.
fn strings(v: &Value, out: &mut Vec<String>) {
    match v {
        Value::String(s) => out.push(s.clone()),
        Value::Array(a) => a.iter().for_each(|x| strings(x, out)),
        Value::Object(o) => o.values().for_each(|x| strings(x, out)),
        _ => {}
    }
}

fn check_common(m: &Value, env: Environment, evm_networks: [&str; 2]) {
    // All six protocol/chain combinations exist, and none is enabled by default.
    let routes = m["routes"].as_array().unwrap();
    let ids: BTreeSet<String> = routes
        .iter()
        .map(|r| r["id"].as_str().unwrap().to_string())
        .collect();
    for p in PROTOCOLS {
        for n in evm_networks {
            assert!(ids.contains(&format!("{p}:{n}")), "missing route {p}:{n}");
        }
    }
    assert_eq!(routes.len(), 6, "exactly six combinations");
    for r in routes {
        assert_eq!(
            r["enabled"], false,
            "{} must be disabled by default",
            r["id"]
        );
        let status = r["status"].as_str().unwrap();
        assert!(
            ["pending-verification", "unavailable", "verified"].contains(&status),
            "{}",
            r["id"]
        );
        assert!(
            r["reason"].as_str().is_some_and(|s| !s.is_empty()),
            "{} needs a visible reason",
            r["id"]
        );
        let net = network(r["network"].as_str().unwrap()).expect("route network in registry");
        assert_eq!(net.environment, env, "{} crosses environments", r["id"]);
    }

    // Every network entry agrees with the registry (chain id / CCTP domain).
    for (id, entry) in m["networks"].as_object().unwrap() {
        let net = network(id).expect("network in registry");
        assert_eq!(net.environment, env, "{id} in wrong manifest");
        assert_eq!(
            entry["cctpDomain"].as_u64(),
            Some(u64::from(net.cctp_domain))
        );
        assert_eq!(entry["chainId"].as_u64(), net.chain_id);
        assert_eq!(
            entry["usdc"]["decimals"].as_u64(),
            Some(u64::from(net.usdc_decimals))
        );
    }

    // Every verification record has a source and a check time; "verified"
    // requires a recorded code hash.
    let mut stack = vec![m.clone()];
    while let Some(v) = stack.pop() {
        match v {
            Value::Object(o) => {
                if let Some(ver) = o.get("verification").filter(|x| !x.is_null()) {
                    assert!(ver["sourceUrl"]
                        .as_str()
                        .is_some_and(|s| s.starts_with("https://")));
                    assert!(ver["checkedAt"].as_str().is_some());
                    if ver["status"] == "bytecode-verified" {
                        assert!(
                            ver["codeHash"].as_str().is_some(),
                            "verified without code hash"
                        );
                    }
                }
                stack.extend(o.into_iter().map(|(_, x)| x));
            }
            Value::Array(a) => stack.extend(a),
            _ => {}
        }
    }
}

#[test]
fn mainnet_manifest_covers_six_disabled_routes() {
    check_common(
        &manifest("mainnet"),
        Environment::Mainnet,
        ["base", "ethereum"],
    );
}

#[test]
fn testnet_manifest_covers_six_disabled_routes() {
    check_common(
        &manifest("testnet"),
        Environment::Testnet,
        ["base-sepolia", "ethereum-sepolia"],
    );
}

#[test]
fn native_usdc_and_usdbc_cannot_be_confused() {
    let m = manifest("mainnet");
    let base = &m["networks"]["base"];
    let usdc = base["usdc"]["address"].as_str().unwrap().to_lowercase();
    let usdbc = base["nonCanonicalUsdc"][0]["address"]
        .as_str()
        .unwrap()
        .to_lowercase();
    assert_eq!(base["nonCanonicalUsdc"][0]["symbol"], "USDbC");
    assert_eq!(usdbc, USDBC_BASE);
    assert_ne!(usdc, usdbc);
    // USDbC may appear only in the nonCanonicalUsdc list, never in a route.
    let mut route_strings = Vec::new();
    strings(&m["routes"], &mut route_strings);
    assert!(!route_strings
        .iter()
        .any(|s| s.to_lowercase().contains(&USDBC_BASE[2..])));
}

#[test]
fn testnet_manifest_cannot_resolve_mainnet_write_targets() {
    let main = manifest("mainnet");
    let test = manifest("testnet");
    let mut mainnet_values = Vec::new();
    strings(&main["networks"], &mut mainnet_values);
    strings(&main["routes"], &mut mainnet_values);
    let addresses: BTreeSet<String> = mainnet_values
        .into_iter()
        .filter(|s| s.starts_with("0x") && s.len() == 42 || s.len() == 56 && s.starts_with('C'))
        .map(|s| s.to_lowercase())
        .collect();

    let mut testnet_values = Vec::new();
    strings(&test, &mut testnet_values);
    for s in testnet_values {
        assert!(
            !addresses.contains(&s.to_lowercase()),
            "testnet manifest contains mainnet-only address {s}"
        );
    }

    let mut ids: Vec<&str> = test["networks"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    ids.extend(
        test["routes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["network"].as_str().unwrap()),
    );
    for id in ids {
        assert_eq!(
            network(id).unwrap().environment,
            Environment::Testnet,
            "testnet manifest references mainnet network {id}"
        );
    }
}
