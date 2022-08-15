mod ft;
mod owner;
mod treasury;

use near_contract_standards::fungible_token::events::FtMint;
use near_contract_standards::fungible_token::metadata::{FungibleTokenMetadata, FT_METADATA_SPEC};
use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::LazyOption;
use near_sdk::json_types::U128;
use near_sdk::{
    assert_one_yocto, env, ext_contract, log, near_bindgen, require, AccountId, Balance,
    BorshStorageKey, Gas, PanicOnDefault, Promise, PromiseResult,
};

use crate::ft::*;
use crate::treasury::*;

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
pub struct Contract {
    owner_id: AccountId,
    token: FungibleToken,
    metadata: LazyOption<FungibleTokenMetadata>,
    treasury: Treasury,
}

const DATA_IMAGE_SVG_NEAR_ICON: &str = "data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 288 288'%3E%3Cg id='l' data-name='l'%3E%3Cpath d='M187.58,79.81l-30.1,44.69a3.2,3.2,0,0,0,4.75,4.2L191.86,103a1.2,1.2,0,0,1,2,.91v80.46a1.2,1.2,0,0,1-2.12.77L102.18,77.93A15.35,15.35,0,0,0,90.47,72.5H87.34A15.34,15.34,0,0,0,72,87.84V201.16A15.34,15.34,0,0,0,87.34,216.5h0a15.35,15.35,0,0,0,13.08-7.31l30.1-44.69a3.2,3.2,0,0,0-4.75-4.2L96.14,186a1.2,1.2,0,0,1-2-.91V104.61a1.2,1.2,0,0,1,2.12-.77l89.55,107.23a15.35,15.35,0,0,0,11.71,5.43h3.13A15.34,15.34,0,0,0,216,201.16V87.84A15.34,15.34,0,0,0,200.66,72.5h0A15.35,15.35,0,0,0,187.58,79.81Z'/%3E%3C/g%3E%3C/svg%3E";

const GAS_FOR_FT_TRANSFER: Gas = Gas(450_000_000_000);
const GAS_FOR_RESOLVE_SELL: Gas = Gas(2_000_000_000_000);

#[derive(BorshSerialize, BorshStorageKey)]
enum StorageKey {
    FungibleToken,
    Metadata,
    Treasury,
}

#[near_bindgen]
impl Contract {
    /// Initializes the contract owned by the given `owner_id`
    #[init]
    pub fn new(owner_id: AccountId) -> Self {
        require!(!env::state_exists(), "Already initialized");

        Self {
            owner_id,
            token: FungibleToken::new(StorageKey::FungibleToken),
            metadata: LazyOption::new(
                StorageKey::Metadata,
                Some(&FungibleTokenMetadata {
                    spec: FT_METADATA_SPEC.to_string(),
                    name: "K fungible token".to_string(),
                    symbol: "KTK".to_string(),
                    icon: Some(DATA_IMAGE_SVG_NEAR_ICON.to_string()),
                    reference: None,
                    reference_hash: None,
                    decimals: 24,
                }),
            ),
            treasury: Treasury::new(StorageKey::Treasury),
        }
    }

    pub(crate) fn on_tokens_burned(&mut self, account_id: AccountId, amount: Balance) {
        log!("Account @{} burned {}", account_id, amount);
    }

    pub(crate) fn internal_buy(&mut self, account_id: &AccountId, amount: Balance) {
        self.token.internal_deposit(account_id, amount);
        FtMint {
            owner_id: account_id,
            amount: &U128(amount),
            memo: None,
        }
        .emit();
    }

    #[payable]
    pub fn sell(&mut self, asset_id: AccountId, amount: U128) -> Promise {
        assert_one_yocto();
        require!(
            env::prepaid_gas() > GAS_FOR_FT_TRANSFER + GAS_FOR_RESOLVE_SELL,
            "More gas is required"
        );
        self.treasury.assert_asset_enabled(&asset_id);

        let account_id = env::predecessor_account_id();
        self.token.internal_withdraw(&account_id, amount.into());
        ext_ft_transfer::ext(asset_id)
            .with_static_gas(GAS_FOR_FT_TRANSFER)
            .with_attached_deposit(1)
            .ft_transfer(account_id.clone(), amount, None)
            .then(
                ext_self::ext(env::current_account_id())
                    .with_static_gas(GAS_FOR_RESOLVE_SELL)
                    .resolve_sell(account_id, amount),
            )
    }
}

#[ext_contract(ext_self)]
pub trait ContractResolver {
    fn resolve_sell(&mut self, account_id: AccountId, amount: U128);
}

#[near_bindgen]
impl ContractResolver for Contract {
    fn resolve_sell(&mut self, account_id: AccountId, amount: U128) {
        match env::promise_result(0) {
            PromiseResult::NotReady => env::abort(),
            PromiseResult::Successful(_) => {
                FtMint {
                    owner_id: &account_id,
                    amount: &amount,
                    memo: None,
                }
                .emit();
            }
            PromiseResult::Failed => {
                self.token.internal_deposit(&account_id, amount.into());
                env::log_str(
                    format!(
                        "Refunding {} KTK to {} after error transferring stable tokens",
                        u128::from(amount),
                        account_id,
                    )
                    .as_str(),
                );
            }
        }
    }
}

#[ext_contract(ext_ft_transfer)]
pub trait FungibleTokenTransfer {
    fn ft_transfer(&mut self, receiver_id: AccountId, amount: U128, memo: Option<String>);
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use near_contract_standards::fungible_token::core::FungibleTokenCore;
    use near_sdk::test_utils::{accounts, VMContextBuilder};
    use near_sdk::{testing_env, Balance};

    use super::*;

    const AMOUNT: Balance = 3_000_000_000_000_000_000_000_000;

    fn get_context(predecessor_account_id: AccountId) -> VMContextBuilder {
        let mut builder = VMContextBuilder::new();
        builder
            .current_account_id(accounts(0))
            .signer_account_id(predecessor_account_id.clone())
            .predecessor_account_id(predecessor_account_id);
        builder
    }

    #[test]
    fn test_new() {
        let mut context = get_context(accounts(0));
        testing_env!(context.build());
        let contract = Contract::new(accounts(1));
        testing_env!(context.is_view(true).build());
        assert_eq!(contract.owner_id, accounts(1));
        assert_eq!(contract.ft_total_supply().0, 0);
        assert_eq!(contract.ft_balance_of(accounts(1)).0, 0);
    }

    #[test]
    #[should_panic(expected = "The contract is not initialized")]
    fn test_default() {
        let context = get_context(accounts(0));
        testing_env!(context.build());
        let _contract = Contract::default();
    }

    #[test]
    fn test_transfer() {
        let mut context = get_context(accounts(0));
        testing_env!(context.build());
        let mut contract = Contract::new(accounts(1));
        contract.token.internal_deposit(&accounts(2), AMOUNT);

        testing_env!(context
            .storage_usage(env::storage_usage())
            .attached_deposit(1)
            .predecessor_account_id(accounts(2))
            .build());
        let transfer_amount = AMOUNT / 3;
        contract.ft_transfer(accounts(3), transfer_amount.into(), None);

        testing_env!(context
            .storage_usage(env::storage_usage())
            .account_balance(env::account_balance())
            .is_view(true)
            .attached_deposit(0)
            .build());
        assert_eq!(
            contract.ft_balance_of(accounts(2)).0,
            (AMOUNT - transfer_amount)
        );
        assert_eq!(contract.ft_balance_of(accounts(3)).0, transfer_amount);
    }

    #[test]
    fn test_buy() {
        const AMOUNT: Balance = 3_000_000_000_000_000_000_000_000;
        let mut context = get_context(accounts(0));
        testing_env!(context.build());
        let mut contract = Contract::new(accounts(1));

        testing_env!(context
            .storage_usage(env::storage_usage())
            .attached_deposit(1)
            .predecessor_account_id(accounts(2))
            .build());
        contract.internal_buy(&accounts(1), AMOUNT);
        assert_eq!(contract.ft_balance_of(accounts(1)).0, AMOUNT);
    }

    #[test]
    fn test_sell() {
        let mut context = get_context(accounts(0));
        testing_env!(context.build());
        let mut contract = Contract::new(accounts(1));
        contract.token.internal_deposit(&accounts(2), AMOUNT);

        testing_env!(context
            .storage_usage(env::storage_usage())
            .predecessor_account_id(accounts(1))
            .build());
        let asset_id = accounts(3);
        contract.add_asset(&asset_id, 20);

        testing_env!(context
            .storage_usage(env::storage_usage())
            .attached_deposit(1)
            .predecessor_account_id(accounts(2))
            .build());
        contract.sell(asset_id, AMOUNT.into());
        assert_eq!(contract.ft_balance_of(accounts(2)).0, 0);
    }
}
