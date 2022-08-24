use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::UnorderedMap;
use near_sdk::json_types::{U128, U64};
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::{env, near_bindgen, BorshStorageKey, PanicOnDefault, Timestamp};

type AssetId = String;

#[derive(BorshSerialize, BorshStorageKey)]
enum StorageKey {
    Assets,
}

#[derive(BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct Asset {
    pub timestamp: Timestamp,
    pub price: Price,
}

#[derive(BorshSerialize, BorshDeserialize, Serialize, Deserialize, Clone, Copy)]
#[serde(crate = "near_sdk::serde")]
pub struct Price {
    pub multiplier: U128,
    pub decimals: u8,
}

#[derive(Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct PriceData {
    pub asset_id: String,
    pub timestamp: U64,
    pub expiration: U64,
    pub price: Option<Price>,
}

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct Contract {
    pub assets: UnorderedMap<AssetId, Asset>,
    pub recency_duration: Timestamp,
}

#[near_bindgen]
impl Contract {
    #[init]
    pub fn new(recency_duration: U64) -> Self {
        Self {
            assets: UnorderedMap::new(StorageKey::Assets),
            recency_duration: recency_duration.into(),
        }
    }

    pub fn get_exchange_price(&self, asset_id: AssetId) -> PriceData {
        let timestamp = env::block_timestamp();
        PriceData {
            asset_id: asset_id.clone(),
            timestamp: timestamp.into(),
            expiration: (timestamp + self.recency_duration).into(),
            price: self.assets.get(&asset_id).map(|asset| asset.price),
        }
    }

    pub fn set_exchange_price(&mut self, asset_id: AssetId, price: Price) {
        let timestamp = env::block_timestamp();
        self.assets.insert(&asset_id, &Asset { timestamp, price });
    }
}
