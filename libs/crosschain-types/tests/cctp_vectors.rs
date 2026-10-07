//! Runs `spec/fixtures/cctp/vectors.json` through the Rust CCTP codec. The
//! TypeScript SDK runs the same file (sdk/test/cctp.test.ts).

use std::{fs, path::PathBuf};

use astrion_crosschain_types::{
    cctp::{
        bytes32_to_contract, bytes32_to_evm_address, cctp_to_stellar, contract_to_bytes32,
        decode_message, evm_address_to_bytes32, evm_burn_to_stellar, forwarder_hook,
        inbound_to_stellar, parse_forwarder_hook, stellar_burn, stellar_to_cctp, CodecError,
    },
    strkey::{self, StrkeyKind},
};
use serde_json::Value;

fn vectors() -> Value {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../spec/fixtures/cctp/vectors.json");
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

fn unhex(s: &str) -> Vec<u8> {
    let s = s.strip_prefix("0x").unwrap_or(s);
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn arr32(s: &str) -> [u8; 32] {
    unhex(s).try_into().unwrap()
}

fn arr20(s: &str) -> [u8; 20] {
    unhex(s).try_into().unwrap()
}

fn u(v: &Value) -> u128 {
    v.as_str().unwrap().parse().unwrap()
}

fn err<T: std::fmt::Debug>(r: Result<T, CodecError>) -> String {
    r.expect_err("expected an error").to_string()
}

#[test]
fn strkeys_decode_and_reencode() {
    for v in vectors()["strkeys"].as_array().unwrap() {
        let s = v["strkey"].as_str().unwrap();
        let key = strkey::decode(s).unwrap();
        assert_eq!(hex(&key.key), v["hex"].as_str().unwrap(), "{s}");
        let kind = match v["kind"].as_str().unwrap() {
            "account" => StrkeyKind::Account,
            "contract" => StrkeyKind::Contract,
            _ => StrkeyKind::Muxed,
        };
        assert_eq!(key.kind, kind, "{s}");
        let id = v
            .get("id")
            .map(|id| id.as_str().unwrap().parse::<u64>().unwrap());
        assert_eq!(key.muxed_id, id, "{s}");
        assert_eq!(strkey::encode(kind, &key.key, id), s);
    }
}

#[test]
fn invalid_strkeys_are_rejected() {
    for v in vectors()["invalidStrkeys"].as_array().unwrap() {
        let s = v["strkey"].as_str().unwrap();
        assert_eq!(err(strkey::decode(s)), v["error"].as_str().unwrap(), "{s}");
    }
}

#[test]
fn contract_bytes32_round_trip_and_rejects_accounts() {
    for v in vectors()["strkeys"].as_array().unwrap() {
        let s = v["strkey"].as_str().unwrap();
        if v["kind"] == "contract" {
            let raw = contract_to_bytes32(s).unwrap();
            assert_eq!(hex(&raw), v["hex"].as_str().unwrap());
            assert_eq!(bytes32_to_contract(&raw), s);
        } else {
            assert_eq!(err(contract_to_bytes32(s)), "NotAContract", "{s}");
        }
    }
}

#[test]
fn amount_conversions() {
    let v = vectors();
    for case in v["amounts"]["stellarToCctp"].as_array().unwrap() {
        let input = u(&case["stellar"]);
        match case.get("error") {
            Some(e) => assert_eq!(err(stellar_to_cctp(input)), e.as_str().unwrap(), "{input}"),
            None => assert_eq!(
                stellar_to_cctp(input).unwrap(),
                (u(&case["burned"]), u(&case["dust"])),
                "{input}"
            ),
        }
    }
    for case in v["amounts"]["cctpToStellar"].as_array().unwrap() {
        let input = u(&case["cctp"]);
        match case.get("error") {
            Some(e) => assert_eq!(err(cctp_to_stellar(input)), e.as_str().unwrap(), "{input}"),
            None => assert_eq!(
                cctp_to_stellar(input).unwrap(),
                u(&case["stellar"]),
                "{input}"
            ),
        }
    }
}

#[test]
fn evm_bytes32() {
    let v = vectors();
    for case in v["evm"]["valid"].as_array().unwrap() {
        let addr = arr20(case["address"].as_str().unwrap());
        let b = evm_address_to_bytes32(&addr);
        assert_eq!(format!("0x{}", hex(&b)), case["bytes32"].as_str().unwrap());
        assert_eq!(bytes32_to_evm_address(&b).unwrap(), addr);
    }
    for case in v["evm"]["invalidBytes32"].as_array().unwrap() {
        let b = arr32(case["bytes32"].as_str().unwrap());
        assert_eq!(
            err(bytes32_to_evm_address(&b)),
            case["error"].as_str().unwrap()
        );
    }
}

#[test]
fn forwarder_hooks() {
    let v = vectors();
    for case in v["hooks"]["valid"].as_array().unwrap() {
        let recipient = case["recipient"].as_str().unwrap();
        let payload = unhex(case["payload"].as_str().unwrap());
        let built = forwarder_hook(recipient, &payload).unwrap();
        assert_eq!(format!("0x{}", hex(&built)), case["hex"].as_str().unwrap());
        let (parsed, rest) = parse_forwarder_hook(&built).unwrap();
        assert_eq!(parsed, recipient);
        assert_eq!(rest, payload.as_slice());
    }
    for case in v["hooks"]["invalid"].as_array().unwrap() {
        let bytes = unhex(case["hex"].as_str().unwrap());
        assert_eq!(
            err(parse_forwarder_hook(&bytes)),
            case["error"].as_str().unwrap(),
            "{}",
            case["why"]
        );
    }
}

#[test]
fn messages_decode() {
    let v = vectors();
    let forwarder = v["messages"]["forwarderTestnet"].as_str().unwrap();
    for case in v["messages"]["valid"].as_array().unwrap() {
        let m = decode_message(&unhex(case["hex"].as_str().unwrap())).unwrap();
        let d = &case["decoded"];
        assert_eq!(u64::from(m.version), d["version"].as_u64().unwrap());
        assert_eq!(
            u64::from(m.source_domain),
            d["sourceDomain"].as_u64().unwrap()
        );
        assert_eq!(
            u64::from(m.destination_domain),
            d["destinationDomain"].as_u64().unwrap()
        );
        assert_eq!(m.nonce, arr32(d["nonce"].as_str().unwrap()));
        assert_eq!(
            m.destination_caller,
            arr32(d["destinationCaller"].as_str().unwrap())
        );
        assert_eq!(
            u64::from(m.min_finality_threshold),
            d["minFinalityThreshold"].as_u64().unwrap()
        );
        assert_eq!(
            u64::from(m.finality_threshold_executed),
            d["finalityThresholdExecuted"].as_u64().unwrap()
        );
        let b = &d["burn"];
        assert_eq!(m.burn.burn_token, arr32(b["burnToken"].as_str().unwrap()));
        assert_eq!(
            m.burn.mint_recipient,
            arr32(b["mintRecipient"].as_str().unwrap())
        );
        assert_eq!(m.burn.amount, u(&b["amount"]));
        assert_eq!(
            m.burn.message_sender,
            arr32(b["messageSender"].as_str().unwrap())
        );
        assert_eq!(m.burn.max_fee, u(&b["maxFee"]));
        assert_eq!(m.burn.fee_executed, u(&b["feeExecuted"]));
        assert_eq!(m.burn.expiration_block, u(&b["expirationBlock"]));
        assert_eq!(m.burn.hook_data, unhex(b["hookData"].as_str().unwrap()));
        if let Some(inbound) = case.get("inboundToStellar") {
            let r = inbound_to_stellar(&m, forwarder).unwrap();
            assert_eq!(
                r.forward_recipient,
                inbound["forwardRecipient"].as_str().unwrap()
            );
            assert_eq!(r.minted_stellar_amount, u(&inbound["mintedStellarAmount"]));
        }
    }
    for case in v["messages"]["invalid"].as_array().unwrap() {
        let bytes = unhex(case["hex"].as_str().unwrap());
        assert_eq!(
            err(decode_message(&bytes)),
            case["error"].as_str().unwrap(),
            "{}",
            case["why"]
        );
    }
    for case in v["messages"]["invalidInbound"].as_array().unwrap() {
        let bytes = unhex(case["hex"].as_str().unwrap());
        let result = decode_message(&bytes).and_then(|m| inbound_to_stellar(&m, forwarder));
        assert_eq!(
            err(result),
            case["error"].as_str().unwrap(),
            "{}",
            case["why"]
        );
    }
}

#[test]
fn builders_fail_before_burn_on_bad_destinations() {
    let v = vectors();
    let forwarder = v["messages"]["forwarderTestnet"].as_str().unwrap();
    let g = "GAUHMCMUP5FZO5675W3ISZ6E6CNYJGXBUW5WANE2JR4TGAARYCTSCBKI";
    let account = [0x22u8; 20];

    let burn = stellar_burn(10_000_005, 6, &account, None, 100_000, 1000).unwrap();
    assert_eq!(burn.expected_burned, 1_000_000);
    assert_eq!(burn.retained_dust, 5);
    assert_eq!(burn.mint_recipient, evm_address_to_bytes32(&account));
    assert_eq!(burn.destination_caller, [0u8; 32]);
    assert_eq!(
        err(stellar_burn(10_000_000, 27, &account, None, 0, 1000)),
        "BadForwarderFields"
    );
    assert_eq!(
        err(stellar_burn(10_000_000, 6, &[0u8; 20], None, 0, 1000)),
        "NotAnEvmAddress"
    );
    assert_eq!(err(stellar_burn(9, 6, &account, None, 0, 1000)), "DustOnly");
    assert_eq!(
        err(stellar_burn(100, 6, &account, None, 100, 1000)),
        "FeeExceedsAmount"
    );

    let out = evm_burn_to_stellar(1_000_000, forwarder, g, 500, 2000).unwrap();
    let fwd = contract_to_bytes32(forwarder).unwrap();
    assert_eq!(out.destination_domain, 27);
    assert_eq!(out.mint_recipient, fwd);
    assert_eq!(out.destination_caller, fwd);
    assert_eq!(parse_forwarder_hook(&out.hook_data).unwrap().0, g);
    // Forwarder must be a contract; recipient must be a valid strkey.
    assert_eq!(
        err(evm_burn_to_stellar(1_000_000, g, g, 0, 2000)),
        "NotAContract"
    );
    let bad = format!("{}A", &g[..55]);
    assert_eq!(
        err(evm_burn_to_stellar(1_000_000, forwarder, &bad, 0, 2000)),
        "InvalidStrkey"
    );
    assert_eq!(
        err(evm_burn_to_stellar(500, forwarder, g, 500, 2000)),
        "FeeExceedsAmount"
    );
}
