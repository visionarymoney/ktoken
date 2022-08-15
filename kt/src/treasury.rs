use near_contract_standards::upgrade::Ownable;
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::UnorderedMap;
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::{env, near_bindgen, require, AccountId, IntoStorageKey};

use crate::{Contract, ContractExt};

const MAX_U128_DECIMALS: u8 = 37;

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize, Debug, PartialEq)]
#[serde(crate = "near_sdk::serde")]
pub enum AssetStatus {
    Enabled,
    Disabled,
}

#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct AssetInfo {
    decimals: u8,
    status: AssetStatus,
}

impl AssetInfo {
    pub fn new(decimals: u8) -> Self {
        require!(
            decimals > 0 && decimals <= MAX_U128_DECIMALS,
            "Decimal value is out of bounds"
        );

        Self {
            decimals,
            status: AssetStatus::Enabled,
        }
    }
}

#[derive(BorshDeserialize, BorshSerialize)]
pub struct Treasury {
    assets: UnorderedMap<AccountId, AssetInfo>,
}

impl Treasury {
    pub fn new<S>(prefix: S) -> Self
    where
        S: IntoStorageKey,
    {
        Self {
            assets: UnorderedMap::new(prefix),
        }
    }

    pub fn assert_asset(&self, asset_id: &AccountId) {
        require!(
            self.assets.get(asset_id).is_some(),
            &format!("Asset {} is not supported", asset_id)
        );
    }

    pub fn assert_asset_status(&self, asset_id: &AccountId, status: AssetStatus) {
        if self.assets.get(asset_id).unwrap().status != status {
            env::panic_str(&format!("Asset {} is currently not {:?}", asset_id, status));
        }
    }

    pub fn assert_asset_enabled(&self, asset_id: &AccountId) {
        self.assert_asset(asset_id);
        self.assert_asset_status(asset_id, AssetStatus::Enabled)
    }

    pub fn switch_asset_status(&mut self, asset_id: &AccountId, status: AssetStatus) {
        let mut asset_info = self.assets.get(asset_id).unwrap();
        asset_info.status = status;
        self.assets.insert(asset_id, &asset_info);
    }

    pub fn add_asset(&mut self, asset_id: &AccountId, decimals: u8) {
        require!(
            self.assets.get(asset_id).is_none(),
            "Asset is already supported"
        );
        let asset_info = AssetInfo::new(decimals);
        self.assets.insert(asset_id, &asset_info);
    }

    pub fn disable_asset(&mut self, asset_id: &AccountId) {
        self.assert_asset_status(asset_id, AssetStatus::Enabled);
        self.switch_asset_status(asset_id, AssetStatus::Disabled)
    }

    pub fn enable_asset(&mut self, asset_id: &AccountId) {
        self.assert_asset_status(asset_id, AssetStatus::Disabled);
        self.switch_asset_status(asset_id, AssetStatus::Enabled)
    }

    fn supported_assets(&self) -> Vec<(AccountId, AssetInfo)> {
        self.assets.to_vec()
    }
}

pub trait AssetProvider {
    fn add_asset(&mut self, asset_id: &AccountId, decimals: u8);
    fn enable_asset(&mut self, asset_id: &AccountId);
    fn disable_asset(&mut self, asset_id: &AccountId);
    fn supported_assets(&self) -> Vec<(AccountId, AssetInfo)>;
}

#[near_bindgen]
impl AssetProvider for Contract {
    fn add_asset(&mut self, asset_id: &AccountId, decimals: u8) {
        self.assert_owner();
        self.treasury.add_asset(asset_id, decimals);
    }

    fn disable_asset(&mut self, asset_id: &AccountId) {
        self.assert_owner();
        self.treasury.disable_asset(asset_id);
    }

    fn enable_asset(&mut self, asset_id: &AccountId) {
        self.assert_owner();
        self.treasury.enable_asset(asset_id);
    }

    fn supported_assets(&self) -> Vec<(AccountId, AssetInfo)> {
        self.treasury.supported_assets()
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use crate::StorageKey;

    use super::*;
    use near_sdk::test_utils::accounts;

    #[test]
    fn test_assert_asset() {
        let asset_id = accounts(1);
        let mut treasury = Treasury::new(StorageKey::Treasury);
        treasury.add_asset(&asset_id, 20);
        treasury.assert_asset(&asset_id);
    }

    #[test]
    #[should_panic(expected = "Asset bob is not supported")]
    fn test_unsupported_asset() {
        let treasury = Treasury::new(StorageKey::Treasury);
        treasury.assert_asset(&accounts(1));
    }

    #[test]
    fn test_new() {
        let treasury = Treasury::new(StorageKey::Treasury);
        assert_eq!(treasury.assets.to_vec().len(), 0);
    }

    #[test]
    fn test_add_asset() {
        let asset_id = &accounts(1);
        let decimals = 20;
        let mut treasury = Treasury::new(StorageKey::Treasury);
        treasury.add_asset(asset_id, decimals);

        let asset = treasury.assets.get(asset_id).unwrap();
        assert_eq!(asset.status, AssetStatus::Enabled);
        assert_eq!(asset.decimals, decimals)
    }

    #[test]
    #[should_panic(expected = "Asset is already supported")]
    fn test_add_asset_twice() {
        let mut treasury = Treasury::new(StorageKey::Treasury);
        treasury.add_asset(&accounts(1), 20);
        assert!(treasury.assets.get(&accounts(1)).is_some());
        treasury.add_asset(&accounts(1), 20);
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
    fn test_enable_disable_assets() {
        let mut treasury = Treasury::new(StorageKey::Treasury);
        treasury.add_asset(&accounts(1), 20);
        assert_eq!(
            treasury.assets.get(&accounts(1)).unwrap().status,
            AssetStatus::Enabled
        );
        treasury.disable_asset(&accounts(1));
        assert_eq!(
            treasury.assets.get(&accounts(1)).unwrap().status,
            AssetStatus::Disabled
        );
        treasury.enable_asset(&accounts(1));
        assert_eq!(
            treasury.assets.get(&accounts(1)).unwrap().status,
            AssetStatus::Enabled
        );
    }

    #[test]
    #[should_panic(expected = "Asset bob is currently not Enabled")]
    fn test_disable_asset_twice() {
        let mut treasury = Treasury::new(StorageKey::Treasury);
        treasury.add_asset(&accounts(1), 20);
        assert_eq!(
            treasury.assets.get(&accounts(1)).unwrap().status,
            AssetStatus::Enabled
        );
        treasury.disable_asset(&accounts(1));
        assert_eq!(
            treasury.assets.get(&accounts(1)).unwrap().status,
            AssetStatus::Disabled
        );
        treasury.disable_asset(&accounts(1));
    }

    #[test]
    #[should_panic(expected = "Asset bob is currently not Disabled")]
    fn test_enable_asset_twice() {
        let mut treasury = Treasury::new(StorageKey::Treasury);
        treasury.add_asset(&accounts(1), 20);
        assert_eq!(
            treasury.assets.get(&accounts(1)).unwrap().status,
            AssetStatus::Enabled
        );
        treasury.enable_asset(&accounts(1));
    }

    #[test]
    fn test_supported_assets() {
        let mut treasury = Treasury::new(StorageKey::Treasury);
        treasury.add_asset(&accounts(1), 20);
        treasury.add_asset(&accounts(2), 20);
        treasury.add_asset(&accounts(3), 20);

        let supported_assets = treasury.supported_assets();
        assert_eq!(supported_assets.len(), 3);
        assert_eq!(supported_assets[0].0, accounts(1));
        assert_eq!(supported_assets[1].0, accounts(2));
        assert_eq!(supported_assets[2].0, accounts(3));
    }
}
