use crate::*;
use near_contract_standards::upgrade::Ownable;

impl Ownable for Contract {
    fn get_owner(&self) -> AccountId {
        self.owner_id.clone()
    }

    fn set_owner(&mut self, owner_id: AccountId) {
        self.assert_owner();
        self.owner_id = owner_id;
    }
}
#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use near_contract_standards::upgrade::Ownable;
    use near_sdk::{
        test_utils::{accounts, VMContextBuilder},
        testing_env,
    };

    use crate::Contract;

    #[test]
    #[should_panic(expected = "Owner must be predecessor")]
    fn test_assert_owner() {
        let mut context = VMContextBuilder::new();
        context
            .current_account_id(accounts(0))
            .signer_account_id(accounts(1))
            .predecessor_account_id(accounts(1));
        testing_env!(context.build());
        let contract = Contract::new(accounts(2), accounts(3));

        testing_env!(context.predecessor_account_id(accounts(1)).build());
        contract.assert_owner();
    }

    #[test]
    fn test_get_owner() {
        let mut context = VMContextBuilder::new();
        context
            .current_account_id(accounts(0))
            .signer_account_id(accounts(1))
            .predecessor_account_id(accounts(1));
        testing_env!(context.build());
        let contract = Contract::new(accounts(2), accounts(3));

        testing_env!(context.predecessor_account_id(accounts(2)).build());
        assert_eq!(contract.get_owner(), accounts(2));
    }

    #[test]
    fn test_set_owner() {
        let mut context = VMContextBuilder::new();
        context
            .current_account_id(accounts(0))
            .signer_account_id(accounts(1))
            .predecessor_account_id(accounts(1));
        testing_env!(context.build());
        let mut contract = Contract::new(accounts(2), accounts(3));

        testing_env!(context.predecessor_account_id(accounts(2)).build());
        contract.set_owner(accounts(4));
        assert_eq!(contract.owner_id, accounts(4));
    }
}
