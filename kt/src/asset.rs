use near_contract_standards::upgrade::Ownable;
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::{near_bindgen, require, AccountId, Balance};

use crate::{Contract, ContractExt, MAX_U128_DECIMALS};

pub type AssetId = AccountId;

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Debug, PartialEq, Eq)]
#[serde(crate = "near_sdk::serde")]
pub enum AssetStatus {
    Enabled,
    Disabled,
}

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
#[cfg_attr(not(target_arch = "wasm32"), derive(Debug))]
pub struct AssetInfo {
    pub decimals: u8,
    pub balance: Balance,
    pub status: AssetStatus,
}

impl AssetInfo {
    pub fn new(decimals: u8) -> Self {
        require!(
            decimals > 0 && decimals <= MAX_U128_DECIMALS,
            "Decimal value is out of bounds"
        );

        Self {
            decimals,
            balance: 0,
            status: AssetStatus::Enabled,
        }
    }
}

#[near_bindgen]
impl Contract {
    pub fn add_asset(&mut self, asset_id: &AccountId, decimals: u8) {
        self.assert_owner();
        self.treasury.add_asset(asset_id, decimals);
    }

    pub fn disable_asset(&mut self, asset_id: &AccountId) {
        self.assert_owner();
        self.treasury.disable_asset(asset_id);
    }

    pub fn enable_asset(&mut self, asset_id: &AccountId) {
        self.assert_owner();
        self.treasury.enable_asset(asset_id);
    }

    pub fn supported_assets(&self) -> Vec<(AccountId, AssetInfo)> {
        self.treasury.supported_assets()
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use near_sdk::test_utils::accounts;

    use crate::asset::AssetStatus;
    use crate::treasury::Treasury;
    use crate::{StorageKey, MAX_U128_DECIMALS};

    #[test]
    fn test_assert_asset() {
        let asset_id = &accounts(1);
        let mut treasury = Treasury::new(StorageKey::Treasury);
        treasury.add_asset(asset_id, 20);
        let asset = treasury.assert_asset(asset_id);
        assert_eq!(asset.decimals, 20);
        assert_eq!(asset.balance, 0);
        assert_eq!(asset.status, AssetStatus::Enabled);
    }

    #[test]
    #[should_panic(expected = "Asset bob is not supported")]
    fn test_unsupported_asset() {
        let treasury = Treasury::new(StorageKey::Treasury);
        treasury.assert_asset(&accounts(1));
    }

    #[test]
    fn test_assert_asset_status() {
        let asset_id = &accounts(1);
        let mut treasury = Treasury::new(StorageKey::Treasury);
        treasury.add_asset(asset_id, 20);
        treasury.assert_asset_status(asset_id, AssetStatus::Enabled);
        treasury.disable_asset(asset_id);
        treasury.assert_asset_status(asset_id, AssetStatus::Disabled);
    }

    #[test]
    fn set_asset_status() {
        let asset_id = &accounts(1);
        let mut treasury = Treasury::new(StorageKey::Treasury);
        treasury.add_asset(asset_id, 20);
        treasury.set_asset_status(asset_id, AssetStatus::Disabled);
        treasury.assert_asset_status(asset_id, AssetStatus::Disabled);
        treasury.set_asset_status(asset_id, AssetStatus::Enabled);
        treasury.assert_asset_status(asset_id, AssetStatus::Enabled);
    }

    #[test]
    fn test_enable_disable_assets() {
        let asset_id = &accounts(1);
        let mut treasury = Treasury::new(StorageKey::Treasury);
        treasury.add_asset(asset_id, 20);

        assert_eq!(
            treasury.supported_assets()[0].1.status,
            AssetStatus::Enabled
        );
        treasury.disable_asset(asset_id);
        assert_eq!(
            treasury.supported_assets()[0].1.status,
            AssetStatus::Disabled
        );
        treasury.enable_asset(asset_id);
        assert_eq!(
            treasury.supported_assets()[0].1.status,
            AssetStatus::Enabled
        );
    }

    #[test]
    #[should_panic(expected = "Asset bob is currently not Disabled")]
    fn test_enable_asset_twice() {
        let asset_id = &accounts(1);
        let mut treasury = Treasury::new(StorageKey::Treasury);
        treasury.add_asset(asset_id, 20);
        assert_eq!(
            treasury.supported_assets()[0].1.status,
            AssetStatus::Enabled
        );
        treasury.enable_asset(asset_id);
    }

    #[test]
    #[should_panic(expected = "Asset bob is currently not Enabled")]
    fn test_disable_asset_twice() {
        let asset_id = &accounts(1);
        let mut treasury = Treasury::new(StorageKey::Treasury);
        treasury.add_asset(asset_id, 20);
        assert_eq!(
            treasury.supported_assets()[0].1.status,
            AssetStatus::Enabled
        );
        treasury.disable_asset(asset_id);
        assert_eq!(
            treasury.supported_assets()[0].1.status,
            AssetStatus::Disabled
        );
        treasury.disable_asset(asset_id);
    }

    #[test]
    fn test_add_asset() {
        let asset_id = &accounts(1);
        let decimals = 20;
        let mut treasury = Treasury::new(StorageKey::Treasury);
        treasury.add_asset(asset_id, decimals);

        let (asset_id, info) = &treasury.supported_assets()[0];
        assert_eq!(asset_id, asset_id);
        assert_eq!(info.status, AssetStatus::Enabled);
        assert_eq!(info.decimals, decimals)
    }

    #[test]
    #[should_panic(expected = "Asset is already supported")]
    fn test_add_asset_twice() {
        let asset_id = &accounts(1);
        let decimals = 20;
        let mut treasury = Treasury::new(StorageKey::Treasury);
        treasury.add_asset(asset_id, decimals);
        assert_eq!(treasury.supported_assets().len(), 1);
        treasury.add_asset(asset_id, decimals);
    }

    #[test]
    #[should_panic(expected = "Decimal value is out of bounds")]
    fn test_add_asset_with_zero_decimals() {
        let mut treasury = Treasury::new(StorageKey::Treasury);
        treasury.add_asset(&accounts(1), 0);
    }

    #[test]
    #[should_panic(expected = "Decimal value is out of bounds")]
    fn test_add_asset_with_exceeded_decimals() {
        let mut treasury = Treasury::new(StorageKey::Treasury);
        treasury.add_asset(&accounts(1), MAX_U128_DECIMALS + 1);
    }

    #[test]
    fn test_supported_assets() {
        let mut treasury = Treasury::new(StorageKey::Treasury);
        treasury.add_asset(&accounts(1), 20);
        treasury.add_asset(&accounts(2), 20);
        treasury.add_asset(&accounts(3), 20);

        let assets = treasury.supported_assets();
        assert_eq!(assets.len(), 3);
        assert_eq!(assets[0].0, accounts(1));
        assert_eq!(assets[1].0, accounts(2));
        assert_eq!(assets[2].0, accounts(3));
    }
}
