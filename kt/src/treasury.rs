use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::UnorderedMap;
use near_sdk::{env, require, AccountId, Balance, IntoStorageKey};

use super::asset::{AssetId, AssetInfo, AssetStatus};

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

    pub fn assert_asset(&self, asset_id: &AssetId) -> AssetInfo {
        self.assets.get(asset_id).unwrap_or_else(|| {
            env::panic_str(format!("Asset {} is not supported", asset_id).as_str())
        })
    }

    pub fn assert_asset_status(&self, asset_id: &AssetId, status: AssetStatus) -> AssetInfo {
        let asset = self.assert_asset(asset_id);
        require!(
            asset.status == status,
            format!("Asset {} is currently not {:?}", asset_id, status)
        );
        asset
    }

    pub fn set_asset_status(&mut self, asset_id: &AssetId, status: AssetStatus) {
        let mut asset = self.assets.get(asset_id).unwrap();
        asset.status = status;
        self.assets.insert(asset_id, &asset);
    }

    pub fn enable_asset(&mut self, asset_id: &AssetId) {
        self.assert_asset_status(asset_id, AssetStatus::Disabled);
        self.set_asset_status(asset_id, AssetStatus::Enabled)
    }

    pub fn disable_asset(&mut self, asset_id: &AssetId) {
        self.assert_asset_status(asset_id, AssetStatus::Enabled);
        self.set_asset_status(asset_id, AssetStatus::Disabled)
    }

    pub fn add_asset(&mut self, asset_id: &AssetId, decimals: u8) {
        require!(
            self.assets.get(asset_id).is_none(),
            "Asset is already supported"
        );
        let asset = AssetInfo::new(decimals);
        self.assets.insert(asset_id, &asset);
    }

    pub fn supported_assets(&self) -> Vec<(AssetId, AssetInfo)> {
        self.assets.to_vec()
    }

    pub fn internal_deposit(&mut self, asset_id: &AssetId, amount: Balance) {
        let mut asset = self.assets.get(asset_id).unwrap();
        if let Some(new_balance) = asset.balance.checked_add(amount) {
            asset.balance = new_balance;
            self.assets.insert(asset_id, &asset);
        } else {
            env::panic_str("Treasury balance overflow");
        }
    }

    pub fn internal_withdraw(&mut self, asset_id: &AssetId, amount: Balance) {
        let mut asset = self.assets.get(asset_id).unwrap();
        if let Some(new_balance) = asset.balance.checked_sub(amount) {
            asset.balance = new_balance;
            self.assets.insert(asset_id, &asset);
        } else {
            env::panic_str("The treasury doesn't have enough balance");
        }
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use near_sdk::test_utils::accounts;

    use crate::treasury::Treasury;
    use crate::StorageKey;

    #[test]
    fn test_new() {
        let treasury = Treasury::new(StorageKey::Treasury);
        assert_eq!(treasury.assets.to_vec().len(), 0);
    }

    #[test]
    fn test_internal_deposit() {
        let asset_id = &accounts(1);
        let amount = 100;
        let mut treasury = Treasury::new(StorageKey::Treasury);
        treasury.add_asset(asset_id, 20);
        treasury.internal_deposit(asset_id, amount);
        assert_eq!(treasury.assets.to_vec().len(), 1);
        assert_eq!(treasury.assets.get(asset_id).unwrap().balance, amount);
    }

    #[test]
    #[should_panic(expected = "Treasury balance overflow")]
    fn test_internal_deposit_balance_overflow() {
        let asset_id = &accounts(1);
        let mut treasury = Treasury::new(StorageKey::Treasury);
        treasury.add_asset(asset_id, 20);
        treasury.internal_deposit(asset_id, 1);
        treasury.internal_deposit(asset_id, u128::MAX);
    }

    #[test]
    fn test_internal_withdraw() {
        let asset_id = &accounts(1);
        let amount = 100;
        let mut treasury = Treasury::new(StorageKey::Treasury);
        treasury.add_asset(asset_id, 20);
        treasury.internal_deposit(asset_id, amount);
        treasury.internal_withdraw(asset_id, amount);
        assert_eq!(treasury.assets.to_vec().len(), 1);
        assert_eq!(treasury.assets.get(asset_id).unwrap().balance, 0);
    }

    #[test]
    #[should_panic(expected = "The treasury doesn't have enough balance")]
    fn test_internal_withdraw_no_balance() {
        let asset_id = &accounts(1);
        let mut treasury = Treasury::new(StorageKey::Treasury);
        treasury.add_asset(asset_id, 20);
        treasury.internal_withdraw(asset_id, 1);
    }
}
