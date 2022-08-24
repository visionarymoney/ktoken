use near_sdk::json_types::{U128, U64};
use near_units::{parse_gas, parse_near};
use serde_json::json;
use workspaces::network::Sandbox;
use workspaces::prelude::*;
use workspaces::{Account, AccountId, Contract, Worker};

async fn set_exchange_price(
    worker: &Worker<Sandbox>,
    contract: &Contract,
    asset_id: &AccountId,
    multiplier: U128,
    decimals: u8,
) -> anyhow::Result<()> {
    assert!(contract
        .call(worker, "set_exchange_price")
        .args_json(json!({
            "asset_id": asset_id,
            "price": {
                "multiplier": multiplier,
                "decimals": decimals,
            }
        }))?
        .transact()
        .await?
        .is_success());

    Ok(())
}

/// Create our own custom Oracle contract and setup the initial state.
async fn create_custom_oracle(
    worker: &Worker<Sandbox>,
    recency_duration: U64,
) -> anyhow::Result<Contract> {
    let oracle = worker
        .dev_deploy(include_bytes!("../res/oracle.wasm"))
        .await?;

    // Initialize our Oracle contract .
    oracle
        .call(worker, "new")
        .args_json(json!({
            "recency_duration": recency_duration,
        }))?
        .transact()
        .await?;

    Ok(oracle)
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
) -> anyhow::Result<(Contract, Account)> {
    let ft = worker.dev_deploy(include_bytes!("../res/ft.wasm")).await?;

    // Create accounts.
    let owner = worker.dev_create_account().await?;

    // Initialize our FT contract with owner and total supply available
    // to be traded and transfered into KT contract.
    ft.call(worker, "new")
        .args_json(json!({
            "owner_id": owner.id(),
            "total_supply": initial_balance,
        }))?
        .transact()
        .await?;

    Ok((ft, owner))
}

/// Create the KT contract and setup the initial state.
async fn create_kt(
    worker: &Worker<Sandbox>,
    oracle_id: &AccountId,
) -> anyhow::Result<(Contract, Account)> {
    let kt = worker.dev_deploy(include_bytes!("../res/kt.wasm")).await?;

    let owner = worker.dev_create_account().await?;

    kt.call(worker, "new")
        .args_json(json!({"owner_id": owner.id(), "oracle_id": oracle_id}))?
        .transact()
        .await?;

    Ok((kt, owner))
}

async fn init(
    worker: &Worker<Sandbox>,
) -> anyhow::Result<(Contract, Contract, Account, Contract, Account)> {
    let recency_duration = U64::from(60_000_000_000); // 5 mintues
    let initial_balance = U128::from(1_000_000_000_000_000_000);

    let oracle = create_custom_oracle(worker, recency_duration).await?;
    let (ft, user) = create_custom_ft(worker, initial_balance).await?;
    let (kt, owner) = create_kt(worker, oracle.id()).await?;

    // KT contract must be registered as a FT account.
    assert!(ft
        .call(worker, "storage_deposit")
        .args_json((kt.id(), Option::<bool>::None))?
        .deposit(parse_near!("30 mN"))
        .transact()
        .await?
        .is_success());

    // Register FT as a supported asset in KT contract.
    owner
        .call(worker, kt.id(), "add_asset")
        .args_json(json!({
            "asset_id": ft.id(),
            "decimals": 6,
        }))?
        .transact()
        .await?;

    Ok((oracle, ft, user, kt, owner))
}

#[tokio::test]
async fn test_buy() -> anyhow::Result<()> {
    let ft_amount = U128::from(1_000_000);
    let kt_amount = U128::from(1_000_000_000_000_000_000);
    let worker = workspaces::sandbox().await?;
    let (oracle, ft, user, kt, _) = init(&worker).await?;

    // Set Oracle price
    set_exchange_price(&worker, &oracle, ft.id(), U128::from(10000), 10).await?;

    let user_ft_balance = balance_of(&worker, ft.id(), user.id()).await?;

    // Buy KT tokens.
    let res = user
        .call(&worker, ft.id(), "ft_transfer_call")
        .args_json(json!({
            "receiver_id": kt.id(),
            "amount": ft_amount,
            "msg": "",
        }))?
        .gas(parse_gas!("200 Tgas") as u64)
        .deposit(1)
        .transact()
        .await?;
    assert!(res.is_success());
    assert!(res.outcome().gas_burnt as u128 <= parse_gas!("30 Tgas"));

    let kt_balance = balance_of(&worker, kt.id(), user.id()).await?;
    assert_eq!(kt_balance, kt_amount);

    let ft_balance = balance_of(&worker, ft.id(), kt.id()).await?;
    assert_eq!(ft_balance, ft_amount);

    let user_ft_balance = user_ft_balance.0 - balance_of(&worker, ft.id(), user.id()).await?.0;
    assert_eq!(user_ft_balance, ft_amount.0);

    Ok(())
}

#[tokio::test]
async fn test_sell() -> anyhow::Result<()> {
    let ft_amount = U128::from(1_000_000);
    let kt_amount = U128::from(1_000_000_000_000_000_000);
    let worker = workspaces::sandbox().await?;
    let (oracle, ft, user, kt, _) = init(&worker).await?;

    // Set Oracle price
    set_exchange_price(&worker, &oracle, ft.id(), U128::from(10000), 10).await?;

    // Buy KT tokens.
    assert!(user
        .call(&worker, ft.id(), "ft_transfer_call")
        .args_json(json!({
            "receiver_id": kt.id(),
            "amount": ft_amount,
            "msg": "",
        }))?
        .gas(parse_gas!("200 Tgas") as u64)
        .deposit(1)
        .transact()
        .await?
        .is_success());

    let user_ft_balance = balance_of(&worker, ft.id(), user.id()).await?;

    // Sell KT tokens.
    let res = user
        .call(&worker, kt.id(), "sell")
        .args_json(json!({
           "asset_id": ft.id(),
           "amount": kt_amount,
        }))?
        .gas(parse_gas!("200 Tgas") as u64)
        .deposit(1)
        .transact()
        .await?;
    assert!(res.is_success());
    assert!(res.outcome().gas_burnt as u128 <= parse_gas!("2.45 Tgas"));

    let kt_balance = balance_of(&worker, kt.id(), user.id()).await?;
    assert_eq!(kt_balance, U128::from(0));

    let ft_balance = balance_of(&worker, ft.id(), kt.id()).await?;
    assert_eq!(ft_balance, U128::from(0));

    let user_ft_balance = balance_of(&worker, ft.id(), user.id()).await?.0 - user_ft_balance.0;
    assert_eq!(user_ft_balance, ft_amount.0);

    Ok(())
}

#[tokio::test]
async fn test_sell_refund() -> anyhow::Result<()> {
    let ft_amount = U128::from(1_000_000);
    let kt_amount = U128::from(1_000_000_000_000_000_000);
    let worker = workspaces::sandbox().await?;
    let (oracle, ft, user, kt, _) = init(&worker).await?;

    // Set Oracle price
    set_exchange_price(&worker, &oracle, ft.id(), U128::from(10000), 10).await?;

    // Buy KT tokens.
    user.call(&worker, ft.id(), "ft_transfer_call")
        .args_json(json!({
            "receiver_id": kt.id(),
            "amount": ft_amount,
            "msg": "",
        }))?
        .gas(parse_gas!("200 Tgas") as u64)
        .deposit(1)
        .transact()
        .await?;

    // Transfer assets back so the cross contract transfer call fails on sell.
    kt.as_account()
        .call(&worker, ft.id(), "ft_transfer")
        .args_json(json!({
           "receiver_id": user.id(),
           "amount": ft_amount,
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
           "amount": kt_amount,
        }))?
        .gas(parse_gas!("200 Tgas") as u64)
        .deposit(1)
        .transact()
        .await?;
    assert!(res.is_success());
    assert!(res.outcome().gas_burnt as u128 <= parse_gas!("2.45 Tgas"));

    let kt_balance = balance_of(&worker, kt.id(), user.id()).await?;
    assert_eq!(kt_balance, kt_amount);

    Ok(())
}
