//! Network registry. Must stay identical to `spec/networks.json` (tested).
//! EVM chain IDs and CCTP domains are deliberately separate fields.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NetworkKind {
    Evm,
    Stellar,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Environment {
    Mainnet,
    Testnet,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Network {
    pub id: &'static str,
    pub kind: NetworkKind,
    pub environment: Environment,
    pub chain_id: Option<u64>,
    pub network_passphrase: Option<&'static str>,
    pub cctp_domain: u32,
    pub usdc_decimals: u8,
}

pub const NETWORKS: [Network; 6] = [
    Network {
        id: "ethereum",
        kind: NetworkKind::Evm,
        environment: Environment::Mainnet,
        chain_id: Some(1),
        network_passphrase: None,
        cctp_domain: 0,
        usdc_decimals: 6,
    },
    Network {
        id: "base",
        kind: NetworkKind::Evm,
        environment: Environment::Mainnet,
        chain_id: Some(8453),
        network_passphrase: None,
        cctp_domain: 6,
        usdc_decimals: 6,
    },
    Network {
        id: "stellar",
        kind: NetworkKind::Stellar,
        environment: Environment::Mainnet,
        chain_id: None,
        network_passphrase: Some("Public Global Stellar Network ; September 2015"),
        cctp_domain: 27,
        usdc_decimals: 7,
    },
    Network {
        id: "ethereum-sepolia",
        kind: NetworkKind::Evm,
        environment: Environment::Testnet,
        chain_id: Some(11155111),
        network_passphrase: None,
        cctp_domain: 0,
        usdc_decimals: 6,
    },
    Network {
        id: "base-sepolia",
        kind: NetworkKind::Evm,
        environment: Environment::Testnet,
        chain_id: Some(84532),
        network_passphrase: None,
        cctp_domain: 6,
        usdc_decimals: 6,
    },
    Network {
        id: "stellar-testnet",
        kind: NetworkKind::Stellar,
        environment: Environment::Testnet,
        chain_id: None,
        network_passphrase: Some("Test SDF Network ; September 2015"),
        cctp_domain: 27,
        usdc_decimals: 7,
    },
];

/// Look up a network by canonical id.
pub fn network(id: &str) -> Option<&'static Network> {
    NETWORKS.iter().find(|n| n.id == id)
}
