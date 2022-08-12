use near_units::{parse_gas, parse_near};
use serde_json::json;
use workspaces::network::Sandbox;
use workspaces::prelude::*;
use workspaces::{Account, AccountId, Contract, Worker};

const FT_TOTAL_SUPPLY: u128 = parse_near!("1,000,000,000 N");
const AMOUNT: u128 = parse_near!("1N");

/// Create our own custom Fungible Token contract and setup the initial state.
async fn create_custom_ft(owner: &Account, worker: &Worker<Sandbox>) -> anyhow::Result<Contract> {
    let ft = worker.dev_deploy(include_bytes!("../res/ft.wasm")).await?;

    // Initialize our FT contract with owner and total supply available
    // to be traded and transfered into other contracts such as KT.
    ft.call(worker, "new")
        .args_json(json!({
            "owner_id": owner.id(),
            "total_supply": FT_TOTAL_SUPPLY.to_string(),
        }))?
        .transact()
        .await?;

    Ok(ft)
}

/// Register account on the FT contract.
async fn ft_register_account(
    ft: &Contract,
    worker: &Worker<Sandbox>,
    account: &AccountId,
) -> anyhow::Result<()> {
    ft.call(worker, "storage_deposit")
        .args_json(json!({
            "account_id": account.to_string(),
            "registration_only": true,
        }))?
        .deposit(parse_near!("30 mN"))
        .transact()
        .await?;
    Ok(())
}

/// Create the KT contract and setup the initial state.
async fn create_kt(
    owner: &Account,
    worker: &Worker<Sandbox>,
    ft: &Contract,
) -> anyhow::Result<Contract> {
    let kt = worker.dev_deploy(include_bytes!("../res/kt.wasm")).await?;

    kt.call(worker, "new")
        .args_json(json!({
            "owner_id": owner.id(),
            "stable_ft_id": ft.id(),
        }))?
        .transact()
        .await?;

    Ok(kt)
}

#[tokio::test]
async fn tests() -> anyhow::Result<()> {
    let worker = workspaces::sandbox().await?;
    let owner = worker.root_account()?;

    ///////////////////////////////////////////////////////////////////////////
    // Stage 1: Deploy relevant contracts.
    ///////////////////////////////////////////////////////////////////////////

    let ft = create_custom_ft(&owner, &worker).await?;
    let kt = create_kt(&owner, &worker, &ft).await?;

    ///////////////////////////////////////////////////////////////////////////
    // Stage 2: Buy KT tokens.
    ///////////////////////////////////////////////////////////////////////////

    ft_register_account(&ft, &worker, kt.id()).await?;
    owner
        .call(&worker, ft.id(), "ft_transfer_call")
        .args_json(json!({
            "receiver_id": kt.id(),
            "amount": AMOUNT.to_string(),
            "msg": "",
        }))?
        .gas(parse_gas!("200 Tgas") as u64)
        .deposit(1)
        .transact()
        .await?;

    let kt_balance: String = worker
        .view(
            kt.id(),
            "ft_balance_of",
            json!({
                "account_id": owner.id(),
            })
            .to_string()
            .into_bytes(),
        )
        .await?
        .json()?;
    assert_eq!(kt_balance, AMOUNT.to_string());

    let ft_balance: String = worker
        .view(
            ft.id(),
            "ft_balance_of",
            json!({
                "account_id": kt.id(),
            })
            .to_string()
            .into_bytes(),
        )
        .await?
        .json()?;
    assert_eq!(ft_balance, AMOUNT.to_string());

    ///////////////////////////////////////////////////////////////////////////
    // Stage 4: Sell KT toekns.
    ///////////////////////////////////////////////////////////////////////////

    owner
        .call(&worker, kt.id(), "sell")
        .args_json(json!({ "amount": AMOUNT.to_string() }))?
        .gas(parse_gas!("200 Tgas") as u64)
        .deposit(1)
        .transact()
        .await?;

    let kt_balance: String = worker
        .view(
            kt.id(),
            "ft_balance_of",
            json!({
                "account_id": owner.id(),
            })
            .to_string()
            .into_bytes(),
        )
        .await?
        .json()?;
    assert_eq!(kt_balance, "0");

    let ft_balance: String = worker
        .view(
            ft.id(),
            "ft_balance_of",
            json!({
                "account_id": owner.id(),
            })
            .to_string()
            .into_bytes(),
        )
        .await?
        .json()?;

    assert_eq!(ft_balance, FT_TOTAL_SUPPLY.to_string());

    ///////////////////////////////////////////////////////////////////////////
    // Stage 5: Validate sell refund.
    ///////////////////////////////////////////////////////////////////////////

    owner
        .call(&worker, ft.id(), "ft_transfer_call")
        .args_json(json!({
            "receiver_id": kt.id(),
            "amount": AMOUNT.to_string(),
            "msg": "",
        }))?
        .gas(parse_gas!("200 Tgas") as u64)
        .deposit(1)
        .transact()
        .await?;

    // Transfer funds back to FT so the cross contract transfer call on sell fails.
    kt.as_account()
        .call(&worker, ft.id(), "ft_transfer")
        .args_json(json!({"receiver_id": owner.id(), "amount": AMOUNT.to_string() }))?
        .gas(parse_gas!("200 Tgas") as u64)
        .deposit(1)
        .transact()
        .await?;

    owner
        .call(&worker, kt.id(), "sell")
        .args_json(json!({ "amount": AMOUNT.to_string() }))?
        .gas(parse_gas!("200 Tgas") as u64)
        .deposit(1)
        .transact()
        .await?;

    let kt_balance: String = worker
        .view(
            kt.id(),
            "ft_balance_of",
            json!({
                "account_id": owner.id(),
            })
            .to_string()
            .into_bytes(),
        )
        .await?
        .json()?;
    assert_eq!(kt_balance, AMOUNT.to_string());

    Ok(())
}
