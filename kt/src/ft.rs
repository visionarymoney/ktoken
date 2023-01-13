use near_contract_standards::fungible_token::core::FungibleTokenCore;
use near_contract_standards::fungible_token::events::{FtBurn, FtTransfer};
use near_contract_standards::fungible_token::metadata::{
    FungibleTokenMetadata, FungibleTokenMetadataProvider,
};
use near_contract_standards::fungible_token::receiver::{ext_ft_receiver, FungibleTokenReceiver};
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::LookupMap;
use near_sdk::env::{self, log_str};
use near_sdk::json_types::U128;
use near_sdk::serde::{Deserialize, Serialize};
use near_sdk::{
    assert_one_yocto, ext_contract, near_bindgen, require, AccountId, Balance, IntoStorageKey,
    PromiseOrValue, PromiseResult,
};

use crate::oracle::ext_oracle;
use crate::price::ExpectedPrice;
use crate::treasury::AssetStatus;
use crate::{
    ext_self, Contract, ContractExt, GAS_FOR_BUY_WITH_PRICE, GAS_FOR_GET_EXCHANGE_PRICE,
    GAS_FOR_ON_TRANSFER, GAS_FOR_RESOLVE_TRANSFER, GAS_FOR_TRANSFER_CALL,
};

type Price = u128;

#[derive(BorshSerialize, BorshDeserialize, Default)]
pub struct AccountBalance {
    amount: Balance,
    price: Price, // Weighted mean
}

impl AccountBalance {
    pub fn checked_add(&self, amount: Balance, price: Price) -> Option<Self> {
        //  balance + amount
        let balance = self.amount.checked_add(amount)?;

        // Weighted arithmetic mean
        // https://en.wikipedia.org/wiki/Algorithms_for_calculating_variance
        // self.price + (amount / balance) * (price - self.price))
        let price = match self.price.cmp(&price) {
            std::cmp::Ordering::Equal => price,
            // self.price + amount * (price - self.price) / balance
            std::cmp::Ordering::Less => self.price.checked_add(
                amount
                    .checked_mul(price - self.price)?
                    .checked_div(balance)?,
            )?,
            // self.price - amount * (self.price - price) / balance
            std::cmp::Ordering::Greater => self.price.checked_sub(
                amount
                    .checked_mul(self.price - price)?
                    .checked_div(balance)?,
            )?,
        };

        Some(Self {
            amount: balance,
            price,
        })
    }

    pub fn checked_sub(&self, amount: Balance, price: Price) -> Option<Self> {
        //  balance - amount
        let balance = self.amount.checked_sub(amount)?;

        // Weighted arithmetic mean
        // https://en.wikipedia.org/wiki/Algorithms_for_calculating_variance
        // self.price - (amount / balance) * (price - self.price))
        let price = match self.price.cmp(&price) {
            std::cmp::Ordering::Equal => price,
            // self.price - amount * (price - self.price) / balance
            std::cmp::Ordering::Less => self.price.checked_sub(
                amount
                    .checked_mul(price - self.price)?
                    .checked_div(balance)?,
            )?,
            // self.price + amount * (self.price - price) / balance
            std::cmp::Ordering::Greater => self.price.checked_add(
                amount
                    .checked_mul(self.price - price)?
                    .checked_div(balance)?,
            )?,
        };

        Some(Self {
            amount: balance,
            price,
        })
    }
}

/// Implementation of a FungibleToken NEP-141 standard.
#[derive(BorshDeserialize, BorshSerialize)]
pub struct FungibleToken {
    /// AccountID -> Account balance.
    accounts: LookupMap<AccountId, AccountBalance>,
    /// Total supply of the all token.
    total_supply: Balance,
}

impl FungibleToken {
    pub fn new<S>(prefix: S) -> Self
    where
        S: IntoStorageKey,
    {
        Self {
            accounts: LookupMap::new(prefix),
            total_supply: 0,
        }
    }

    pub fn internal_unwrap_balance_of(&self, account_id: &AccountId) -> AccountBalance {
        self.accounts.get(account_id).unwrap_or_default()
    }

    pub fn internal_deposit(&mut self, account_id: &AccountId, amount: Balance, price: Price) {
        let balance = self.internal_unwrap_balance_of(account_id);
        if let Some(new_balance) = balance.checked_add(amount, price) {
            self.accounts.insert(account_id, &new_balance);
            self.total_supply = self
                .total_supply
                .checked_add(amount)
                .unwrap_or_else(|| env::panic_str("Total supply overflow"));
        } else {
            env::panic_str("Balance overflow");
        }
    }

    pub fn internal_withdraw(&mut self, account_id: &AccountId, amount: Balance, price: Price) {
        let balance = self.internal_unwrap_balance_of(account_id);
        if let Some(new_balance) = balance.checked_sub(amount, price) {
            self.accounts.insert(account_id, &new_balance);
            self.total_supply = self
                .total_supply
                .checked_sub(amount)
                .unwrap_or_else(|| env::panic_str("Total supply overflow"));
        } else {
            env::panic_str("The account doesn't have enough balance");
        }
    }

    pub fn internal_transfer(
        &mut self,
        sender_id: &AccountId,
        receiver_id: &AccountId,
        amount: Balance,
        price: Price,
        memo: Option<String>,
    ) {
        require!(
            sender_id != receiver_id,
            "Sender and receiver should be different"
        );
        require!(amount > 0, "The amount should be a positive number");
        self.internal_withdraw(sender_id, amount, price);
        self.internal_deposit(receiver_id, amount, price);
        FtTransfer {
            old_owner_id: sender_id,
            new_owner_id: receiver_id,
            amount: &U128(amount),
            memo: memo.as_deref(),
        }
        .emit();
    }
}

impl FungibleTokenCore for FungibleToken {
    fn ft_transfer(&mut self, receiver_id: AccountId, amount: U128, memo: Option<String>) {
        assert_one_yocto();
        let sender_id = env::predecessor_account_id();
        let amount: Balance = amount.into();
        let price = 0; // FIXME: Get price from Oracle.
        self.internal_transfer(&sender_id, &receiver_id, amount, price, memo);
    }

    fn ft_transfer_call(
        &mut self,
        receiver_id: AccountId,
        amount: U128,
        memo: Option<String>,
        msg: String,
    ) -> PromiseOrValue<U128> {
        assert_one_yocto();
        require!(
            env::prepaid_gas() > GAS_FOR_TRANSFER_CALL,
            "More gas is required"
        );
        let sender_id = env::predecessor_account_id();
        let amount: Balance = amount.into();
        let price = 0; // FIXME: Get price from Oracle.
        self.internal_transfer(&sender_id, &receiver_id, amount, price, memo);
        // Initiating receiver's call and the callback
        ext_ft_receiver::ext(receiver_id.clone())
            .with_static_gas(env::prepaid_gas() - GAS_FOR_TRANSFER_CALL)
            .ft_on_transfer(sender_id.clone(), amount.into(), msg)
            .then(
                ext_ft_resolver::ext(env::current_account_id())
                    .with_static_gas(GAS_FOR_RESOLVE_TRANSFER)
                    .ft_resolve_transfer(sender_id, receiver_id, amount.into(), price.into()),
            )
            .into()
    }

    fn ft_total_supply(&self) -> U128 {
        self.total_supply.into()
    }

    fn ft_balance_of(&self, account_id: AccountId) -> U128 {
        self.internal_unwrap_balance_of(&account_id).amount.into()
    }
}

impl FungibleToken {
    /// Internal method that returns the amount of burned tokens in a corner case when the sender
    /// has deleted (unregistered) their account while the `ft_transfer_call` was still in flight.
    /// Returns (Used token amount, Burned token amount)
    pub fn internal_ft_resolve_transfer(
        &mut self,
        sender_id: &AccountId,
        receiver_id: AccountId,
        amount: U128,
        price: U128,
    ) -> (u128, u128) {
        let amount: Balance = amount.into();
        let price: Price = price.into();

        // Get the unused amount from the `ft_on_transfer` call result.
        let unused_amount = match env::promise_result(0) {
            PromiseResult::NotReady => env::abort(),
            PromiseResult::Successful(value) => {
                if let Ok(unused_amount) = near_sdk::serde_json::from_slice::<U128>(&value) {
                    std::cmp::min(amount, unused_amount.0)
                } else {
                    amount
                }
            }
            PromiseResult::Failed => amount,
        };

        if unused_amount > 0 {
            let receiver_balance = self.internal_unwrap_balance_of(&receiver_id);
            if receiver_balance.amount > 0 {
                let refund_amount = std::cmp::min(receiver_balance.amount, unused_amount);
                if let Some(new_balance) = receiver_balance.checked_sub(refund_amount, price) {
                    self.accounts.insert(&receiver_id, &new_balance);
                }

                if let Some(sender_balance) = self.accounts.get(sender_id) {
                    if let Some(new_balance) =
                        sender_balance.checked_add(sender_balance.amount + refund_amount, price)
                    {
                        self.accounts.insert(sender_id, &new_balance);
                    }

                    FtTransfer {
                        old_owner_id: &receiver_id,
                        new_owner_id: sender_id,
                        amount: &U128(refund_amount),
                        memo: Some("refund"),
                    }
                    .emit();
                    return (amount - refund_amount, 0);
                } else {
                    // NOTE: this will only happen if we unregister accouns, e.g. when balance is 0.
                    // Sender's account was deleted, so we need to burn tokens.
                    self.total_supply -= refund_amount;
                    log_str("The account of the sender was deleted");
                    FtBurn {
                        owner_id: &receiver_id,
                        amount: &U128(refund_amount),
                        memo: Some("refund"),
                    }
                    .emit();
                    return (amount, refund_amount);
                }
            }
        }
        (amount, 0)
    }
}

#[near_bindgen]
impl FungibleTokenCore for Contract {
    #[payable]
    fn ft_transfer(&mut self, receiver_id: AccountId, amount: U128, memo: Option<String>) {
        self.token.ft_transfer(receiver_id, amount, memo)
    }
    #[payable]
    fn ft_transfer_call(
        &mut self,
        receiver_id: AccountId,
        amount: U128,
        memo: Option<String>,
        msg: String,
    ) -> PromiseOrValue<U128> {
        self.token.ft_transfer_call(receiver_id, amount, memo, msg)
    }
    fn ft_total_supply(&self) -> U128 {
        self.token.ft_total_supply()
    }
    fn ft_balance_of(&self, account_id: AccountId) -> U128 {
        self.token.ft_balance_of(account_id)
    }
}

#[ext_contract(ext_ft_resolver)]
trait FungibleTokenResolver {
    fn ft_resolve_transfer(
        &mut self,
        sender_id: AccountId,
        receiver_id: AccountId,
        amount: U128,
        price: U128,
    ) -> U128;
}

#[near_bindgen]
impl FungibleTokenResolver for Contract {
    #[private]
    fn ft_resolve_transfer(
        &mut self,
        sender_id: AccountId,
        receiver_id: AccountId,
        amount: U128,
        price: U128,
    ) -> U128 {
        let (used_amount, burned_amount) =
            self.token
                .internal_ft_resolve_transfer(&sender_id, receiver_id, amount, price);
        if burned_amount > 0 {
            self.on_tokens_burned(sender_id, burned_amount);
        }
        used_amount.into()
    }
}

#[near_bindgen]
impl FungibleTokenMetadataProvider for Contract {
    fn ft_metadata(&self) -> FungibleTokenMetadata {
        self.metadata.get().unwrap()
    }
}

// TODO: impl ft_data_to_msg for Contract

#[derive(Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
enum OnTransferMessage {
    Buy(Option<(U128, u8, U128)>),
    // TODO: Rebalance
}

impl TryFrom<&str> for OnTransferMessage {
    type Error = near_sdk::serde_json::Error;

    fn try_from(json: &str) -> Result<Self, Self::Error> {
        near_sdk::serde_json::from_str(json)
    }
}

#[near_bindgen]
impl FungibleTokenReceiver for Contract {
    fn ft_on_transfer(
        &mut self,
        sender_id: AccountId,
        amount: U128,
        msg: String,
    ) -> PromiseOrValue<U128> {
        require!(
            env::prepaid_gas() > GAS_FOR_ON_TRANSFER,
            "More gas is required"
        );

        let contract_id = env::current_account_id();
        let asset_id = env::predecessor_account_id();

        let msg = OnTransferMessage::try_from(msg.as_str())
            .unwrap_or_else(|_| env::panic_str(format!("Invalid message: {}", msg).as_ref()));

        match msg {
            OnTransferMessage::Buy(expected) => {
                let expected = expected.map(|(multiplier, decimals, slippage)| {
                    ExpectedPrice::new(multiplier, decimals, slippage)
                });

                self.treasury
                    .assert_asset_status(&asset_id, AssetStatus::Enabled);

                ext_oracle::ext(self.oracle_id.clone())
                    .with_static_gas(GAS_FOR_GET_EXCHANGE_PRICE)
                    .get_exchange_price(asset_id.clone())
                    .then(
                        ext_self::ext(contract_id)
                            .with_static_gas(GAS_FOR_BUY_WITH_PRICE)
                            .buy_with_price(sender_id, asset_id, amount, expected),
                    )
                    .into()
            }
        }
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use crate::ft::AccountBalance;

    #[test]
    fn test_account_balance() {
        let balance = AccountBalance::default();
        assert_eq!(balance.amount, 0);
        assert_eq!(balance.price, 0);

        // FIXME: wrong amount decimals, KT tokens have 18 decimals

        // (100 * 1) / 100 = 1
        let balance = balance
            .checked_add(100_000_000_000_000_000_000, 1_000_000)
            .unwrap();
        assert_eq!(balance.amount, 100_000_000_000_000_000_000);
        assert_eq!(balance.price, 1_000_000);

        // (100 * 1 + 200 * 1.5) / (100 + 200) = 1.333
        let balance = balance
            .checked_add(200_000_000_000_000_000_000, 1_500_000)
            .unwrap();
        assert_eq!(balance.amount, 300_000_000_000_000_000_000);
        assert_eq!(balance.price, 1_333_333);

        // (100 * 1 + 200 * 1.5 + 200 * 2) / (100 + 200 + 200) = 1.6
        // let balance = balance.checked_add(200, 2_000_000_000_000_000_000).unwrap();
        // assert_eq!(balance.amount, 500);
        // assert_eq!(balance.price, 1_599_999_999_999_999_999);

        // // (100 * 1 + 200 * 1.5 + 200 * 2 - 100 * 2) / (100 + 200 + 200 - 100) = 1.5
        // let balance = balance.checked_sub(100, 2_000_000_000_000_000_000).unwrap();
        // assert_eq!(balance.amount, 400);
        // assert_eq!(balance.price, 1_499_999_999_999_999_999);

        // let balance = AccountBalance::default();
        // assert_eq!(balance.amount, 0);
        // assert_eq!(balance.price, 0);

        // // (100 * 1) / 100 = 1
        // let balance = balance.checked_add(100, 1_000_000_000_000_000_000).unwrap();
        // assert_eq!(balance.amount, 100);
        // assert_eq!(balance.price, 1_000_000_000_000_000_000);

        // // (100 * 1 + 200 * 1.5) / (100 + 200) = 1.333
        // let balance = balance.checked_add(200, 1_500_000_000_000_000_000).unwrap();
        // assert_eq!(balance.amount, 300);
        // assert_eq!(balance.price, 1_333_333_333_333_333_333);

        // // (100 * 1 + 200 * 1.5 + 200 * .5) / (100 + 200 + 200) = 1
        // let balance = balance.checked_add(200, 500_000_000_000_000_000).unwrap();
        // assert_eq!(balance.amount, 500);
        // assert_eq!(balance.price, 1_000_000_000_000_000_000);

        // // (100 * 1 + 200 * 1.5 + 200 * .5 - 100 * 1.3) / (100 + 200 + 200 - 100) = 9.25
        // let balance = balance.checked_sub(100, 1_300_000_000_000_000_000).unwrap();
        // assert_eq!(balance.amount, 400);
        // assert_eq!(balance.price, 925_000_000_000_000_000);

        // Max amount and price
        assert!(AccountBalance::default()
            // amount: ~340quadrillion, price: $1K
            .checked_add(1_000_000_000_000_000_000_000_000_000, 1_000_000_000)
            .is_some());

        // Overflow
        assert!(AccountBalance::default()
            .checked_add(1, 0)
            .unwrap()
            .checked_add(u128::MAX, 0)
            .is_none());
        assert!(AccountBalance::default().checked_sub(1, 0).is_none());
        assert!(AccountBalance::default().checked_add(0, 1).is_none());
        // Amount overflow
        assert!(AccountBalance::default()
            .checked_add(1, 0)
            .unwrap()
            .checked_add(u128::MAX, 0)
            .is_none());
        // Price overflow
        assert!(AccountBalance::default()
            .checked_add(1, 1)
            .unwrap()
            .checked_add(2, u128::MAX)
            .is_none());
    }
}
