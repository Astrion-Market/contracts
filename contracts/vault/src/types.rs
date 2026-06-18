use soroban_sdk::{contracttype, Address, BytesN, String, Vec};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdapterChange {
    pub ids: Vec<BytesN<32>>,
    pub change: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Caps {
    pub allocation: i128,
    pub absolute_cap: i128,
    pub relative_cap: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GateConfig {
    pub receive_shares: Option<Address>,
    pub send_shares: Option<Address>,
    pub receive_assets: Option<Address>,
    pub send_assets: Option<Address>,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VaultConfig {
    pub owner: Address,
    pub curator: Address,
    pub asset: Address,
    pub name: String,
    pub symbol: String,
    pub decimals: u32,
    pub virtual_shares: i128,
    pub performance_fee: i128,
    pub performance_fee_recipient: Address,
    pub management_fee: i128,
    pub management_fee_recipient: Address,
    /// Maximum share-price growth rate per second, WAD-scaled.
    pub max_rate: i128,
    pub gates: GateConfig,
    /// Optional adapter registry. When present, newly enabled adapters must be
    /// registered there; `None` means the vault opts out.
    pub adapter_registry: Option<Address>,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VaultState {
    pub total_assets: i128,
    pub total_shares: i128,
    pub last_update_timestamp: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AccrualPreview {
    pub new_total_assets: i128,
    pub performance_fee_shares: i128,
    pub management_fee_shares: i128,
}
