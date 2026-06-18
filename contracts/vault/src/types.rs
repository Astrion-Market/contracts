use soroban_sdk::{contracttype, Address, String};

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
