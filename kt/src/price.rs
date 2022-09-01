use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::json_types::U128;
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::{require, Balance};

use crate::oracle::ExchangePrice;
use crate::KT_DECIMALS;

#[derive(BorshSerialize, BorshDeserialize, Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct ExpectedPrice {
    multiplier: U128,
    decimals: u8,
    slippage: U128,
}

impl ExpectedPrice {
    pub fn new(multiplier: U128, decimals: u8, slippage: U128) -> Self {
        Self {
            multiplier,
            decimals,
            slippage,
        }
    }

    pub fn assert_price(&self, price: ExchangePrice) {
        require!(
            self.decimals == price.decimals,
            "Slippage error: different decimals"
        );

        let min = self.multiplier.0.saturating_sub(self.slippage.0);
        let max = self.multiplier.0.saturating_add(self.slippage.0);
        require!(
            (min..=max).contains(&price.multiplier),
            format!(
                "Slippage error: price {} is out of range [{}, {}]",
                price.multiplier, min, max
            )
        );
    }
}

fn convert_decimals(amount: Balance, from: u8, to: u8) -> Option<Balance> {
    match from.cmp(&to) {
        std::cmp::Ordering::Equal => Some(amount),
        std::cmp::Ordering::Less => amount.checked_mul(10u128.pow(u32::from(to - from))),
        std::cmp::Ordering::Greater => amount.checked_div(10u128.pow(u32::from(from - to))),
    }
}

pub fn exchange_asset_to_kt(
    asset_amount: Balance,
    asset_decimals: u8,
    price: ExchangePrice,
) -> Option<Balance> {
    let amount = convert_decimals(asset_amount, asset_decimals, KT_DECIMALS)?;

    // amount / price
    // amount * 10^(price.decimals - asset_decimals) / price.multiplier
    let diff = price.decimals.checked_sub(asset_decimals)?;
    amount
        .checked_mul(10u128.pow(u32::from(diff)))?
        .checked_div(price.multiplier)
}

pub fn exchange_kt_to_asset(
    amount: Balance,
    asset_decimals: u8,
    price: ExchangePrice,
) -> Option<Balance> {
    // amount * price
    // amount * price.multiplier / 10^(price.decimals - asset_decimals)
    let diff = price.decimals.checked_sub(asset_decimals)?;
    let amount = amount
        .checked_mul(price.multiplier)?
        .checked_div(10u128.pow(diff as u32))?;

    convert_decimals(amount, KT_DECIMALS, asset_decimals)
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use near_sdk::json_types::U128;

    use crate::{
        oracle::ExchangePrice,
        price::{convert_decimals, exchange_asset_to_kt, exchange_kt_to_asset},
    };

    use super::ExpectedPrice;

    #[test]
    fn test_assert_price() {
        let price = ExchangePrice::new(10001, 10);
        let expected = ExpectedPrice::new(U128::from(10001), 10, U128::from(0));
        expected.assert_price(price);
    }

    #[test]
    fn test_assert_price_slippage() {
        let price = ExchangePrice::new(10001, 10);
        let expected = ExpectedPrice::new(U128::from(9999), 10, U128::from(10));
        expected.assert_price(price);
    }

    #[test]
    #[should_panic(expected = "Slippage error: different decimals")]
    fn test_assert_price_wrong_decimals() {
        let price = ExchangePrice::new(10001, 10);
        let expected = ExpectedPrice::new(U128::from(9999), 6, U128::from(0));
        expected.assert_price(price);
    }

    #[test]
    #[should_panic(expected = "Slippage error: price 10001 is out of range [9998, 10000]")]
    fn test_assert_price_out_of_range() {
        let price = ExchangePrice::new(10001, 10);
        let expected = ExpectedPrice::new(U128::from(9999), 10, U128::from(1));
        expected.assert_price(price);
    }

    #[test]
    fn test_convert_decimals() {
        assert_eq!(convert_decimals(529944008, 32, 28), Some(52994));
        assert_eq!(convert_decimals(52994, 28, 32), Some(529940000));
        assert_eq!(convert_decimals(52994, 28, 28), Some(52994));
    }

    #[test]
    fn test_exchange_asset_to_kt() {
        assert_eq!(
            exchange_asset_to_kt(1_000_000, 6, ExchangePrice::new(1, 6)),
            Some(1_000_000_000_000_000_000)
        );
        assert_eq!(
            exchange_asset_to_kt(1_000_000, 6, ExchangePrice::new(10000, 10)),
            Some(1_000_000_000_000_000_000)
        );
        assert_eq!(
            exchange_asset_to_kt(1_000_000, 6, ExchangePrice::new(20000, 10)),
            Some(500_000_000_000_000_000)
        );
        assert_eq!(
            exchange_asset_to_kt(1_000_000, 6, ExchangePrice::new(5000, 10)),
            Some(2_000_000_000_000_000_000)
        );
        assert_eq!(
            exchange_asset_to_kt(1_000_000, 6, ExchangePrice::new(10001, 10)),
            Some(999_900_009_999_000_099)
        );
        assert_eq!(
            exchange_asset_to_kt(1_000_000, 6, ExchangePrice::new(9999, 10)),
            Some(1_000_100_010_001_000_100)
        );
        // Overflow
        assert!(exchange_asset_to_kt(
            // 100 quadrillions USDC
            100_000_000_000_000_000_000_000,
            6,
            ExchangePrice::new(10000, 10)
        )
        .is_none());
        assert!(exchange_asset_to_kt(
            // 100 quadrillions DAI
            100_000_000_000_000_000_000_000_000_000_000_000,
            18,
            ExchangePrice::new(10000, 22)
        )
        .is_none());
    }

    #[test]
    fn test_exchange_kt_to_asset() {
        assert_eq!(
            exchange_kt_to_asset(1_000_000_000_000_000_000, 6, ExchangePrice::new(1, 6)),
            Some(1_000_000)
        );
        assert_eq!(
            exchange_kt_to_asset(1_000_000_000_000_000_000, 6, ExchangePrice::new(10000, 10)),
            Some(1_000_000)
        );
        assert_eq!(
            exchange_kt_to_asset(500_000_000_000_000_000, 6, ExchangePrice::new(20000, 10)),
            Some(1_000_000)
        );
        assert_eq!(
            exchange_kt_to_asset(2_000_000_000_000_000_000, 6, ExchangePrice::new(5000, 10)),
            Some(1_000_000)
        );
        assert_eq!(
            exchange_kt_to_asset(999_900_009_999_000_099, 6, ExchangePrice::new(10001, 10)),
            // Roudning error
            Some(999_999)
        );
        assert_eq!(
            exchange_kt_to_asset(1_000_100_010_001_000_100, 6, ExchangePrice::new(9990, 10)),
            // Roudning error
            Some(999_099)
        );
        // Overflow
        assert!(exchange_kt_to_asset(
            // 1 trillion KT
            1_000_000_000_000_000_000_000_000_000_000,
            // Asset -> USDC
            6,
            // oracle price -> 100_000 $
            ExchangePrice::new(1_000_000_000, 10)
        )
        .is_none());
        assert!(exchange_kt_to_asset(
            // 1 trillion KT
            1_000_000_000_000_000_000_000_000_000_000,
            // Asset -> DAI
            18,
            // oracle price -> 100_000 $
            ExchangePrice::new(1_000_000_000, 22)
        )
        .is_none());
    }
}
