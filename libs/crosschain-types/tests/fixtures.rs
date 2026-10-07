//! Runs every shared vector in `spec/fixtures/` and checks the network
//! registry against `spec/networks.json`.

use std::{fs, path::PathBuf};

use astrion_crosschain_types::{
    network, validate_batch, validate_receipt, Environment, NetworkKind, NETWORKS,
};
use serde_json::Value;

fn spec_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../spec")
}

fn fixtures(sub: &str) -> Vec<(String, Value)> {
    let mut out = Vec::new();
    let root = spec_dir().join("fixtures").join(sub);
    let mut dirs = vec![root];
    while let Some(dir) = dirs.pop() {
        for entry in fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                dirs.push(path);
            } else if path.extension().is_some_and(|e| e == "json") {
                let value: Value =
                    serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
                out.push((path.display().to_string(), value));
            }
        }
    }
    assert!(!out.is_empty(), "no fixtures under {sub}");
    out
}

fn outcome<T, E: ToString>(r: Result<T, E>) -> String {
    match r {
        Ok(_) => "ok".to_string(),
        Err(e) => e.to_string(),
    }
}

#[test]
fn intent_fixtures_match_expectations() {
    for (path, fixture) in fixtures("intents") {
        let expect = fixture["expect"].as_str().unwrap();
        let intents = fixture["intents"].as_array().unwrap();
        assert_eq!(outcome(validate_batch(intents)), expect, "{path}");
    }
}

#[test]
fn receipt_fixtures_match_expectations() {
    for (path, fixture) in fixtures("receipts") {
        let expect = fixture["expect"].as_str().unwrap();
        assert_eq!(
            outcome(validate_receipt(&fixture["receipt"])),
            expect,
            "{path}"
        );
    }
}

#[test]
fn registry_matches_spec_networks_json() {
    let json: Value =
        serde_json::from_str(&fs::read_to_string(spec_dir().join("networks.json")).unwrap())
            .unwrap();
    let entries = json["networks"].as_array().unwrap();
    assert_eq!(entries.len(), NETWORKS.len());
    for entry in entries {
        let n = network(entry["id"].as_str().unwrap()).expect("network in registry");
        let kind = match n.kind {
            NetworkKind::Evm => "evm",
            NetworkKind::Stellar => "stellar",
        };
        let env = match n.environment {
            Environment::Mainnet => "mainnet",
            Environment::Testnet => "testnet",
        };
        assert_eq!(entry["kind"], kind);
        assert_eq!(entry["environment"], env);
        assert_eq!(entry["cctpDomain"].as_u64(), Some(u64::from(n.cctp_domain)));
        assert_eq!(
            entry["usdcDecimals"].as_u64(),
            Some(u64::from(n.usdc_decimals))
        );
        assert_eq!(entry["chainId"].as_u64(), n.chain_id);
        assert_eq!(entry["networkPassphrase"].as_str(), n.network_passphrase);
    }
}

#[test]
fn chain_ids_and_domains_are_distinct_fields() {
    // Base: chain 8453, domain 6. Ethereum: chain 1, domain 0.
    assert_eq!(network("base").unwrap().chain_id, Some(8453));
    assert_eq!(network("base").unwrap().cctp_domain, 6);
    assert_eq!(network("ethereum").unwrap().chain_id, Some(1));
    assert_eq!(network("ethereum").unwrap().cctp_domain, 0);
    assert_eq!(network("stellar").unwrap().cctp_domain, 27);
}
