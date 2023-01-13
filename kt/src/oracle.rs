use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::json_types::{U128, U64};
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::{env, ext_contract, require, Balance};

use crate::price::convert_decimals;
use crate::treasury::{AssetId, AssetInfo};

const PRICE_DECIMALS: u8 = 18;

type Timestamp = U64;

// From https://github.com/NearDeFi/price-oracle/blob/main/src/asset.rs
// Price USDC { multiplier: 10000, decimals: 10 }
// 5 USDC = 5 * 10**6 * 10000 / 10**(10 - 6) = 5 * 10**6

#[derive(BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct Price {
    pub multiplier: U128,
    pub decimals: u8,
}

#[cfg(test)]
impl Price {
    pub fn new(multiplier: u128, decimals: u8) -> Self {
        Self {
            multiplier: multiplier.into(),
            decimals,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct PriceData {
    pub expiration: Timestamp,
    pub price: Option<Price>,
}

#[cfg(test)]
impl PriceData {
    pub fn new(expired: bool, price: Option<Price>) -> Self {
        Self {
            expiration: match expired {
                // Note: env::block_timestamp() return 0 on tests
                true => U64::from(0),
                false => U64::from(1),
            },
            price,
        }
    }
}

#[ext_contract(ext_oracle)]
pub trait Oracle {
    fn get_exchange_price(&self, asset_id: AssetId) -> PriceData;
}

#[derive(BorshSerialize, BorshDeserialize, Serialize, Deserialize, Clone, Copy)]
#[cfg_attr(not(target_arch = "wasm32"), derive(Debug))]
#[serde(crate = "near_sdk::serde")]
pub struct ExchangePrice {
    pub multiplier: Balance,
    pub decimals: u8,
}

impl ExchangePrice {
    #[cfg(test)]
    pub fn new(multiplier: u128, decimals: u8) -> Self {
        Self {
            multiplier,
            decimals,
        }
    }

    pub fn from_price_data(asset: &AssetInfo, data: PriceData) -> Self {
        require!(
            env::block_timestamp() < data.expiration.0,
            "Oracle price is outdated",
        );

        let price = data
            .price
            .unwrap_or_else(|| env::panic_str("Oracle price is missing"));

        // price.decimals - asset.decimals
        let diff = price
            .decimals
            .checked_sub(asset.decimals)
            .unwrap_or_else(|| env::panic_str("Oracle price wrong decimals"));

        if price.multiplier.0 == 0 {
            env::panic_str("Oracle price is zero")
        }

        Self {
            multiplier: price.multiplier.into(),
            decimals: diff,
        }
    }

    pub fn to_decimals(self) -> u128 {
        // Stored in decimals due to more precise value
        convert_decimals(self.multiplier, self.decimals, PRICE_DECIMALS)
            .unwrap_or_else(|| env::panic_str("Oracle price to decimals overflow"))
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use crate::{oracle::ExchangePrice, treasury::AssetInfo};

    use super::{Price, PriceData};

    #[test]
    fn test_exchange_price() {
        let price = ExchangePrice::from_price_data(
            &AssetInfo::new(6),
            PriceData::new(false, Some(Price::new(10001, 10))),
        );
        assert_eq!(price.multiplier, 10001);
        assert_eq!(price.decimals, 4);
    }

    #[test]
    #[should_panic(expected = "Oracle price is outdated")]
    fn test_oudated_exchange_price() {
        ExchangePrice::from_price_data(
            &AssetInfo::new(6),
            PriceData::new(true, Some(Price::new(10001, 10))),
        );
    }

    #[test]
    #[should_panic(expected = "Oracle price is missing")]
    fn test_missing_exchange_price() {
        ExchangePrice::from_price_data(&AssetInfo::new(6), PriceData::new(false, None));
    }

    #[test]
    #[should_panic(expected = "Oracle price wrong decimals")]
    fn test_wrong_decimals_exchange_price() {
        ExchangePrice::from_price_data(
            &AssetInfo::new(10),
            PriceData::new(false, Some(Price::new(1, 6))),
        );
    }

    #[test]
    #[should_panic(expected = "Oracle price is zero")]
    fn test_zero_exchange_price() {
        ExchangePrice::from_price_data(
            &AssetInfo::new(6),
            PriceData::new(false, Some(Price::new(0, 10))),
        );
    }
}
