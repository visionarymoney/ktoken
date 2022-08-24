use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::json_types::{U128, U64};
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::{env, ext_contract, require, Balance};

use crate::asset::AssetId;

type Timestamp = U64;

// From https://github.com/NearDeFi/price-oracle/blob/main/src/asset.rs
// Price USDC { multiplier: 10000, decimals: 10 }
// 5 USDC = 5 * 10**6 * 10000 / 10**(10 - 6) = 5 * 10**6

#[derive(BorshSerialize, BorshDeserialize, Serialize, Deserialize, Clone, Copy)]
#[serde(crate = "near_sdk::serde")]
pub struct Price {
    pub multiplier: U128,
    pub decimals: u8,
}

#[derive(Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct PriceData {
    pub timestamp: Timestamp,
    pub expiration: Timestamp,
    pub price: Option<Price>,
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
    pub fn assert_valid(&self, decimals: u8) {
        require!(
            self.decimals >= decimals,
            "Oracle price decimals do not match asset decimals"
        );
    }
}

#[cfg(test)]
impl ExchangePrice {
    pub fn new(multiplier: u128, decimals: u8) -> Self {
        Self {
            multiplier,
            decimals,
        }
    }
}

impl From<PriceData> for ExchangePrice {
    fn from(data: PriceData) -> Self {
        require!(
            env::block_timestamp() < data.expiration.0,
            "Oracle price is outdated",
        );

        let price = data
            .price
            .unwrap_or_else(|| env::panic_str("Oracle price is missing"));

        if price.multiplier.0 == 0 {
            env::panic_str("Oracle price is zero")
        }

        Self {
            multiplier: price.multiplier.into(),
            decimals: price.decimals,
        }
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use near_sdk::{env, json_types::U64};

    use crate::oracle::ExchangePrice;

    use super::{Price, PriceData};

    fn new_price_data(multiplier: u128, decimals: u8, outdated: bool, missing: bool) -> PriceData {
        // Note: env::block_timestamp() return 0 on tests
        let timestamp = env::block_timestamp().into();
        PriceData {
            timestamp,
            expiration: match outdated {
                true => timestamp,
                false => U64::from(1),
            },
            price: match missing {
                true => None,
                false => Some(Price {
                    multiplier: multiplier.into(),
                    decimals,
                }),
            },
        }
    }

    #[test]
    fn test_exchange_price() {
        let multiplier = 1001;
        let decimals = 10;
        let price: ExchangePrice = new_price_data(multiplier, decimals, false, false).into();
        assert_eq!(price.multiplier, multiplier);
        assert_eq!(price.decimals, decimals);
    }

    #[test]
    #[should_panic(expected = "Oracle price is outdated")]
    fn test_oudated_exchange_price() {
        let _ = ExchangePrice::from(new_price_data(1001, 10, true, false));
    }

    #[test]
    #[should_panic(expected = "Oracle price is zero")]
    fn test_zero_exchange_price() {
        let _ = ExchangePrice::from(new_price_data(0, 0, false, false));
    }

    #[test]
    #[should_panic(expected = "Oracle price is missing")]
    fn test_missing_exchange_price() {
        let _ = ExchangePrice::from(new_price_data(1001, 10, false, true));
    }

    #[test]
    fn test_assert_valid_exchange_price() {
        let price = ExchangePrice::new(1, 6);
        price.assert_valid(6);
        let price = ExchangePrice::new(10000, 10);
        price.assert_valid(6);
    }

    #[test]
    #[should_panic(expected = "Oracle price decimals do not match asset decimals")]
    fn test_assert_invalid_exchange_price() {
        ExchangePrice::new(1, 6).assert_valid(10);
    }
}
