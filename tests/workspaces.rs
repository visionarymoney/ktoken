use near_sdk::json_types::U128;
use near_units::{parse_gas, parse_near};
use serde_json::json;
use workspaces::network::Sandbox;
use workspaces::prelude::*;
use workspaces::{Account, AccountId, Contract, Worker};

const FT_TOTAL_SUPPLY: u128 = parse_near!("1,000,000,000 N");

async fn register_account(
    worker: &Worker<Sandbox>,
    contract: &Contract,
    account_id: &AccountId,
) -> anyhow::Result<()> {
    let res = contract
        .call(worker, "storage_deposit")
        .args_json((account_id, Option::<bool>::None))?
        .deposit(parse_near!("30 mN"))
        .transact()
        .await?;
    assert!(res.is_success());

    Ok(())
}

async fn balance_of(
    worker: &Worker<Sandbox>,
    contract_id: &AccountId,
    account_id: &AccountId,
) -> anyhow::Result<U128> {
    worker
        .view(
            contract_id,
            "ft_balance_of",
            json!({
                "account_id": account_id,
            })
            .to_string()
            .into_bytes(),
        )
        .await?
        .json::<U128>()
}

/// Create our own custom Fungible Token contract and setup the initial state.
async fn create_custom_ft(
    worker: &Worker<Sandbox>,
    initial_balance: U128,
) -> anyhow::Result<(Contract, Account, Account)> {
    let ft = worker.dev_deploy(include_bytes!("../res/ft.wasm")).await?;

    // Create accounts.
    let owner = worker.dev_create_account().await?;
    let user = worker.dev_create_account().await?;

    // Initialize our FT contract with owner and total supply available
    // to be traded and transfered into KT contract.
    ft.call(worker, "new")
        .args_json(json!({
            "owner_id": owner.id(),
            "total_supply": FT_TOTAL_SUPPLY.to_string(),
        }))?
        .transact()
        .await?;

    // Add initial balance to the user account.
    register_account(worker, &ft, user.id()).await?;
    owner
        .call(worker, ft.id(), "ft_transfer")
        .args_json(json!({
            "receiver_id": user.id(),
            "amount": initial_balance,
            "memo": "",
        }))?
        .deposit(1)
        .transact()
        .await?;

    Ok((ft, owner, user))
}

/// Create the KT contract and setup the initial state.
async fn create_kt(worker: &Worker<Sandbox>) -> anyhow::Result<(Contract, Account)> {
    let kt = worker.dev_deploy(include_bytes!("../res/kt.wasm")).await?;

    let owner = worker.dev_create_account().await?;

    kt.call(worker, "new")
        .args_json(json!({"owner_id": owner.id()}))?
        .transact()
        .await?;

    Ok((kt, owner))
}

async fn init(
    worker: &Worker<Sandbox>,
    initial_balance: U128,
) -> anyhow::Result<(Contract, Account, Contract, Account)> {
    let (ft, _, user) = create_custom_ft(worker, initial_balance).await?;
    let (kt, owner) = create_kt(worker).await?;

    // KT contract must be registered as a FT account.
    register_account(worker, &ft, kt.id()).await?;

    // Register FT as a supported asset in KT contract.
    owner
        .call(worker, kt.id(), "add_asset")
        .args_json(json!({
            "asset_id": ft.id(),
            "decimals": 20,
        }))?
        .transact()
        .await?;

    Ok((ft, user, kt, owner))
}

#[tokio::test]
async fn test_buy() -> anyhow::Result<()> {
    let initial_balance = U128::from(parse_near!("10000 N"));
    let transfer_amount = U128::from(parse_near!("100 N"));
    let worker = workspaces::sandbox().await?;
    let (ft, user, kt, _) = init(&worker, initial_balance).await?;

    // Buy KT tokens.
    let res = user
        .call(&worker, ft.id(), "ft_transfer_call")
        .args_json(json!({
            "receiver_id": kt.id(),
            "amount": transfer_amount,
            "msg": "",
        }))?
        .gas(parse_gas!("200 Tgas") as u64)
        .deposit(1)
        .transact()
        .await?;
    assert!(res.is_success());
    assert!(res.outcome().gas_burnt as u128 <= parse_gas!("30 Tgas"));

    let kt_balance = balance_of(&worker, kt.id(), user.id()).await?;
    assert_eq!(kt_balance, transfer_amount);

    let ft_balance = balance_of(&worker, ft.id(), user.id()).await?;
    assert_eq!(ft_balance.0, initial_balance.0 - transfer_amount.0);

    Ok(())
}

#[tokio::test]
async fn test_sell() -> anyhow::Result<()> {
    let initial_balance = U128::from(parse_near!("10000 N"));
    let transfer_amount = U128::from(parse_near!("100 N"));
    let worker = workspaces::sandbox().await?;
    let (ft, user, kt, _) = init(&worker, initial_balance).await?;

    // Buy KT tokens.
    user.call(&worker, ft.id(), "ft_transfer_call")
        .args_json(json!({
            "receiver_id": kt.id(),
            "amount": transfer_amount,
            "msg": "",
        }))?
        .gas(parse_gas!("200 Tgas") as u64)
        .deposit(1)
        .transact()
        .await?;

    // Sell KT tokens.
    let res = user
        .call(&worker, kt.id(), "sell")
        .args_json(json!({
        "asset_id": ft.id(),
         "amount": transfer_amount,
         }))?
        .gas(parse_gas!("200 Tgas") as u64)
        .deposit(1)
        .transact()
        .await?;
    assert!(res.is_success());
    assert!(res.outcome().gas_burnt as u128 <= parse_gas!("2.45 Tgas"));

    let kt_balance = balance_of(&worker, kt.id(), user.id()).await?;
    assert_eq!(kt_balance, U128::from(0));

    let ft_balance = balance_of(&worker, ft.id(), user.id()).await?;
    assert_eq!(ft_balance, initial_balance);

    Ok(())
}

#[tokio::test]
async fn test_sell_refund() -> anyhow::Result<()> {
    let worker = workspaces::sandbox().await?;
    let initial_balance = U128::from(parse_near!("10000 N"));
    let transfer_amount = U128::from(parse_near!("100 N"));
    let (ft, user, kt, _) = init(&worker, initial_balance).await?;

    // Buy KT tokens.
    user.call(&worker, ft.id(), "ft_transfer_call")
        .args_json(json!({
            "receiver_id": kt.id(),
            "amount": transfer_amount,
            "msg": "",
        }))?
        .gas(parse_gas!("200 Tgas") as u64)
        .deposit(1)
        .transact()
        .await?;

    // Transfer funds back to FT so the cross contract transfer call fails on sell.
    kt.as_account()
        .call(&worker, ft.id(), "ft_transfer")
        .args_json(json!({
           "receiver_id": user.id(),
           "amount": transfer_amount,
        }))?
        .gas(parse_gas!("200 Tgas") as u64)
        .deposit(1)
        .transact()
        .await?;

    // Sell KT tokens.
    let res = user
        .call(&worker, kt.id(), "sell")
        .args_json(json!({
           "asset_id": ft.id(),
           "amount": transfer_amount,
        }))?
        .gas(parse_gas!("200 Tgas") as u64)
        .deposit(1)
        .transact()
        .await?;
    assert!(res.is_success());
    assert!(res.outcome().gas_burnt as u128 <= parse_gas!("2.45 Tgas"));

    let kt_balance = balance_of(&worker, kt.id(), user.id()).await?;
    assert_eq!(kt_balance, transfer_amount);

    Ok(())
}
