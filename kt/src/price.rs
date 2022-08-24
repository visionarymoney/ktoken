use near_sdk::Balance;

use crate::oracle::ExchangePrice;
use crate::KT_DECIMALS;

// TODO: handle arithmetic overflow

fn convert_decimals(amount: Balance, from: u8, to: u8) -> Balance {
    match from.cmp(&to) {
        std::cmp::Ordering::Equal => amount,
        std::cmp::Ordering::Less => amount * 10u128.pow(u32::from(to - from)),
        std::cmp::Ordering::Greater => amount / 10u128.pow(u32::from(from - to)),
    }
}

pub fn exchange_asset_to_kt(amount: Balance, decimals: u8, price: ExchangePrice) -> Balance {
    price.assert_valid(decimals);

    let amount = convert_decimals(amount, decimals, KT_DECIMALS);

    // amount / price
    // amount * 10**(price.decimals - decimals) / price.multiplier
    amount * 10u128.pow((price.decimals - decimals) as u32) / price.multiplier
}

pub fn exchange_kt_to_asset(amount: Balance, decimals: u8, price: ExchangePrice) -> Balance {
    price.assert_valid(decimals);

    // amount * price
    // amount * price.multiplier / 10**(price.decimals - decimals)
    let amount = amount * price.multiplier / 10u128.pow((price.decimals - decimals) as u32);

    convert_decimals(amount, KT_DECIMALS, decimals)
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use crate::{
        oracle::ExchangePrice,
        price::{convert_decimals, exchange_asset_to_kt, exchange_kt_to_asset},
    };

    #[test]
    fn test_convert_decimals() {
        assert_eq!(convert_decimals(529944008, 32, 28), 52994);
        assert_eq!(convert_decimals(52994, 28, 32), 529940000);
        assert_eq!(convert_decimals(52994, 28, 28), 52994);
    }

    #[test]
    fn test_exchange_asset_to_kt() {
        assert_eq!(
            exchange_asset_to_kt(1_000_000, 6, ExchangePrice::new(1, 6)),
            1_000_000_000_000_000_000
        );
        assert_eq!(
            exchange_asset_to_kt(1_000_000, 6, ExchangePrice::new(10000, 10)),
            1_000_000_000_000_000_000
        );
        assert_eq!(
            exchange_asset_to_kt(1_000_000, 6, ExchangePrice::new(20000, 10)),
            500_000_000_000_000_000
        );
        assert_eq!(
            exchange_asset_to_kt(1_000_000, 6, ExchangePrice::new(5000, 10)),
            2_000_000_000_000_000_000
        );
        assert_eq!(
            exchange_asset_to_kt(1_000_000, 6, ExchangePrice::new(10001, 10)),
            999_900_009_999_000_099
        );
        assert_eq!(
            exchange_asset_to_kt(1_000_000, 6, ExchangePrice::new(9999, 10)),
            1_000_100_010_001_000_100
        );
    }

    #[test]
    fn test_exchange_asset_to_kt_overflow() {
        // Overflow
        assert!(std::panic::catch_unwind(|| exchange_asset_to_kt(
            // 100 quadrillions USDC
            100_000_000_000_000_000_000_000,
            6,
            ExchangePrice::new(10000, 10)
        ))
        .is_err());
        assert!(std::panic::catch_unwind(|| exchange_asset_to_kt(
            // 100 quadrillions DAI
            100_000_000_000_000_000_000_000_000_000_000_000,
            18,
            ExchangePrice::new(10000, 22)
        ))
        .is_err());
    }

    #[test]
    fn test_exchange_kt_to_asset() {
        assert_eq!(
            exchange_kt_to_asset(1_000_000_000_000_000_000, 6, ExchangePrice::new(1, 6)),
            1_000_000
        );
        assert_eq!(
            exchange_kt_to_asset(1_000_000_000_000_000_000, 6, ExchangePrice::new(10000, 10)),
            1_000_000
        );
        assert_eq!(
            exchange_kt_to_asset(500_000_000_000_000_000, 6, ExchangePrice::new(20000, 10)),
            1_000_000
        );
        assert_eq!(
            exchange_kt_to_asset(2_000_000_000_000_000_000, 6, ExchangePrice::new(5000, 10)),
            1_000_000
        );
        assert_eq!(
            exchange_kt_to_asset(999_900_009_999_000_099, 6, ExchangePrice::new(10001, 10)),
            // Roudning error
            999_999
        );
        assert_eq!(
            exchange_kt_to_asset(1_000_100_010_001_000_100, 6, ExchangePrice::new(9990, 10)),
            // Roudning error
            999_099
        );
    }

    #[test]
    fn test_exchange_kt_to_asset_overflow() {
        // Overflow
        assert!(std::panic::catch_unwind(|| exchange_kt_to_asset(
            // 1 trillion KT
            1_000_000_000_000_000_000_000_000_000_000,
            // Asset -> USDC
            6,
            // oracle price -> 100_000 $
            ExchangePrice::new(1_000_000_000, 10)
        ))
        .is_err());
        assert!(std::panic::catch_unwind(|| exchange_kt_to_asset(
            // 1 trillion KT
            1_000_000_000_000_000_000_000_000_000_000,
            // Asset -> DAI
            18,
            // oracle price -> 100_000 $
            ExchangePrice::new(1_000_000_000, 22)
        ))
        .is_err());
    }
}
