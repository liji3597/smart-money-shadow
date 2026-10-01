//! pump.fun / PumpSwap buy instruction assembly.
//!
//! Ported from 0xfnzero/sol-trade-sdk @ main (2025 layout): pump.fun legacy
//! `buy_exact_sol_in` (18 accounts) and PumpSwap `buy_exact_quote_in`
//! (WSOL-quoted pools). Account orders are protocol-critical; the index
//! comments below mirror the reference implementation one to one.

use anyhow::{anyhow, bail, Context, Result};
use base64::Engine;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_instruction::{AccountMeta, Instruction};
use solana_pubkey::Pubkey;

// ---- shared programs ---------------------------------------------------

pub const SYSTEM_PROGRAM: Pubkey = Pubkey::from_str_const("11111111111111111111111111111111");
pub const TOKEN_PROGRAM: Pubkey =
    Pubkey::from_str_const("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");
pub const TOKEN_PROGRAM_2022: Pubkey =
    Pubkey::from_str_const("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb");
pub const ATA_PROGRAM: Pubkey =
    Pubkey::from_str_const("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL");
pub const WSOL_MINT: Pubkey =
    Pubkey::from_str_const("So11111111111111111111111111111111111111112");
pub const COMPUTE_BUDGET_PROGRAM: Pubkey =
    Pubkey::from_str_const("ComputeBudget111111111111111111111111111111");
pub const FEE_PROGRAM: Pubkey =
    Pubkey::from_str_const("pfeeUxB6jkeY1Hxd7CsFCAjcbHA9rWtchMGdZ6VojVZ");

const SYSTEM_PROGRAM_META: AccountMeta =
    AccountMeta { pubkey: SYSTEM_PROGRAM, is_signer: false, is_writable: false };
const ATA_PROGRAM_META: AccountMeta =
    AccountMeta { pubkey: ATA_PROGRAM, is_signer: false, is_writable: false };
const FEE_PROGRAM_META: AccountMeta =
    AccountMeta { pubkey: FEE_PROGRAM, is_signer: false, is_writable: false };

// ---- pump.fun bonding curve ---------------------------------------------

const PUMPFUN_PROGRAM: Pubkey =
    Pubkey::from_str_const("6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P");
const PUMPFUN_GLOBAL: Pubkey =
    Pubkey::from_str_const("4wTV1YmiEkRvAtNtsSGPtUrqRYQMe5SKy2uB4Jjaxnjf");
const PUMPFUN_FEE_RECIPIENT: Pubkey =
    Pubkey::from_str_const("62qc2CNXwrYqQScmEdiZFFAnJR262PxWEuNQtxfafNgV");
const PUMPFUN_EVENT_AUTHORITY: Pubkey =
    Pubkey::from_str_const("Ce6TQqeHC9p8KetsN6JsjHK7UTZk7nasjjnr7XxXp9F1");
const PUMPFUN_GLOBAL_VOLUME_ACCUMULATOR: Pubkey =
    Pubkey::from_str_const("Hq2wp8uJ9jCPsYgNHex8RtqdvMPfVGoYwjvF1ATiwn2Y");
const PUMPFUN_FEE_CONFIG: Pubkey =
    Pubkey::from_str_const("8Wf5TiAheLUqBrKXeYg2JtAFFMWtKdG2BSFgqUcPVwTt");

const PUMPFUN_GLOBAL_META: AccountMeta =
    AccountMeta { pubkey: PUMPFUN_GLOBAL, is_signer: false, is_writable: false };
const PUMPFUN_EVENT_AUTHORITY_META: AccountMeta =
    AccountMeta { pubkey: PUMPFUN_EVENT_AUTHORITY, is_signer: false, is_writable: false };
const PUMPFUN_PROGRAM_META: AccountMeta =
    AccountMeta { pubkey: PUMPFUN_PROGRAM, is_signer: false, is_writable: false };
const PUMPFUN_GVA_META: AccountMeta = AccountMeta {
    pubkey: PUMPFUN_GLOBAL_VOLUME_ACCUMULATOR,
    is_signer: false,
    is_writable: false,
};
const PUMPFUN_FEE_CONFIG_META: AccountMeta =
    AccountMeta { pubkey: PUMPFUN_FEE_CONFIG, is_signer: false, is_writable: false };

const MAYHEM_FEE_RECIPIENTS: [Pubkey; 8] = [
    Pubkey::from_str_const("GesfTA3X2arioaHp8bbKdjG9vJtskViWACZoYvxp4twS"),
    Pubkey::from_str_const("4budycTjhs9fD6xw62VBducVTNgMgJJ5BgtKq7mAZwn6"),
    Pubkey::from_str_const("8SBKzEQU4nLSzcwF4a74F2iaUDQyTfjGndn6qUWBnrpR"),
    Pubkey::from_str_const("4UQeTP1T39KZ9Sfxzo3WR5skgsaP6NZa87BAkuazLEKH"),
    Pubkey::from_str_const("8sNeir4QsLsJdYpc9RZacohhK1Y5FLU3nC5LXgYB4aa6"),
    Pubkey::from_str_const("Fh9HmeLNUMVCvejxCtCL2DbYaRyBFVJ5xrWkLnMH6fdk"),
    Pubkey::from_str_const("463MEnMeGyJekNZFQSTUABBEbLnvMTALbT6ZmsxAbAdq"),
    Pubkey::from_str_const("6AUH3WEHucYZyC61hqpqYUWVto5qA5hjHuNQ32GNnNxA"),
];
const PROTOCOL_EXTRA_FEE_RECIPIENTS: [Pubkey; 8] = [
    Pubkey::from_str_const("5YxQFdt3Tr9zJLvkFccqXVUwhdTWJQc1fFg2YPbxvxeD"),
    Pubkey::from_str_const("9M4giFFMxmFGXtc3feFzRai56WbBqehoSeRE5GK7gf7"),
    Pubkey::from_str_const("GXPFM2caqTtQYC2cJ5yJRi9VDkpsYZXzYdwYpGnLmtDL"),
    Pubkey::from_str_const("3BpXnfJaUTiwXnJNe7Ej1rcbzqTTQUvLShZaWazebsVR"),
    Pubkey::from_str_const("5cjcW9wExnJJiqgLjq7DEG75Pm6JBgE1hNv4B2vHXUW6"),
    Pubkey::from_str_const("EHAAiTxcdDwQ3U4bU6YcMsQGaekdzLS3B5SmYo46kJtL"),
    Pubkey::from_str_const("5eHhjP8JaYkz83CWwvGU2uMUXefd3AazWGx4gpcuEEYD"),
    Pubkey::from_str_const("A7hAgCzFw14fejgCp387JUJRMNyz4j89JKnhtKU8piqW"),
];

const PUMPFUN_BUY_EXACT_SOL_IN_DISCRIMINATOR: [u8; 8] = [56, 252, 116, 8, 158, 223, 205, 95];
const SHARING_CONFIG_DISCRIMINATOR: [u8; 8] = [216, 74, 9, 0, 56, 140, 93, 75];
const PUMPFUN_FEE_BASIS_POINTS: u128 = 95;
const PUMPFUN_CREATOR_FEE_BASIS_POINTS: u128 = 30;

// ---- PumpSwap AMM --------------------------------------------------------

const AMM_PROGRAM: Pubkey =
    Pubkey::from_str_const("pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA");
const PUMPSWAP_GLOBAL: Pubkey =
    Pubkey::from_str_const("ADyA8hdefvWN2dbGGWFotbzWxrAvLW83WG6QCVXvJKqw");
const PUMPSWAP_EVENT_AUTHORITY: Pubkey =
    Pubkey::from_str_const("GS4CU59F31iL7aR2Q8zVS8DRrcRnXX1yjQ66TqNVQnaR");
const PUMPSWAP_GLOBAL_VOLUME_ACCUMULATOR: Pubkey =
    Pubkey::from_str_const("C2aFPdENg4A2HQsmrd5rTw5TaYBX5Ku887cWjbFKtZpw");
const PUMPSWAP_FEE_CONFIG: Pubkey =
    Pubkey::from_str_const("5PHirr8joyTMp9JMm6nW7hNDVyEYdkzDqazxPD7RaTjx");
const PUMPSWAP_PROTOCOL_FEE_RECIPIENT: Pubkey =
    Pubkey::from_str_const("62qc2CNXwrYqQScmEdiZFFAnJR262PxWEuNQtxfafNgV");
const DEFAULT_COIN_CREATOR_VAULT_AUTHORITY: Pubkey =
    Pubkey::from_str_const("8N3GDaZ2iwN65oxVatKTLPNooAVUJTbfiVJ1ahyqwjSk");

const PUMPSWAP_GLOBAL_META: AccountMeta =
    AccountMeta { pubkey: PUMPSWAP_GLOBAL, is_signer: false, is_writable: false };
const PUMPSWAP_EVENT_AUTHORITY_META: AccountMeta =
    AccountMeta { pubkey: PUMPSWAP_EVENT_AUTHORITY, is_signer: false, is_writable: false };
const AMM_PROGRAM_META: AccountMeta =
    AccountMeta { pubkey: AMM_PROGRAM, is_signer: false, is_writable: false };
const PUMPSWAP_GVA_META: AccountMeta = AccountMeta {
    pubkey: PUMPSWAP_GLOBAL_VOLUME_ACCUMULATOR,
    is_signer: false,
    is_writable: false,
};
const PUMPSWAP_FEE_CONFIG_META: AccountMeta =
    AccountMeta { pubkey: PUMPSWAP_FEE_CONFIG, is_signer: false, is_writable: false };

const PUMPSWAP_BUY_EXACT_QUOTE_IN_DISCRIMINATOR: [u8; 8] = [198, 46, 21, 82, 180, 217, 232, 112];
const POOL_DISCRIMINATOR: [u8; 8] = [241, 154, 109, 4, 17, 177, 109, 188];
const PUMPSWAP_FEE_CONFIG_DISCRIMINATOR: [u8; 8] = [143, 52, 146, 187, 219, 123, 76, 155];

const LP_FEE_BPS: u64 = 25;
const PROTOCOL_FEE_BPS: u64 = 5;
const COIN_CREATOR_FEE_BPS: u64 = 5;

// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwapPlan {
    Pumpfun,
    Pumpswap,
    Unsupported,
}

pub fn plan_for_dex(dex: &str) -> SwapPlan {
    match dex {
        "pumpfun" => SwapPlan::Pumpfun,
        "pumpswap" => SwapPlan::Pumpswap,
        _ => SwapPlan::Unsupported,
    }
}

#[derive(Debug)]
pub struct BuyQuote {
    pub expected_tokens: u64,
    pub min_tokens_out: u64,
    pub lamports_in: u64,
    pub ixs: Vec<Instruction>,
}

/// Full buy construction for live mode.
pub async fn build_buy_ixs(
    rpc: &RpcClient,
    payer: &Pubkey,
    mint: &Pubkey,
    dex: &str,
    lamports_in: u64,
    slippage_bps: u64,
) -> Result<BuyQuote> {
    match plan_for_dex(dex) {
        SwapPlan::Pumpfun => pumpfun_buy(rpc, Some(payer), mint, lamports_in, slippage_bps).await,
        SwapPlan::Pumpswap => pumpswap_buy(rpc, Some(payer), mint, lamports_in, slippage_bps).await,
        SwapPlan::Unsupported => Err(unsupported(dex)),
    }
}

/// Read-only quote for dry-run logging: same curve/pool math, no instructions.
pub async fn quote_buy(
    rpc: &RpcClient,
    mint: &Pubkey,
    dex: &str,
    lamports_in: u64,
    slippage_bps: u64,
) -> Result<(u64, u64)> {
    let quote = match plan_for_dex(dex) {
        SwapPlan::Pumpfun => pumpfun_buy(rpc, None, mint, lamports_in, slippage_bps).await?,
        SwapPlan::Pumpswap => pumpswap_buy(rpc, None, mint, lamports_in, slippage_bps).await?,
        SwapPlan::Unsupported => return Err(unsupported(dex)),
    };
    Ok((quote.expected_tokens, quote.min_tokens_out))
}

pub fn unsupported(dex: &str) -> anyhow::Error {
    anyhow!("dex {dex} is not supported for buy construction")
}

// ---- shared helpers ------------------------------------------------------

fn pda(seeds: &[&[u8]], program: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(seeds, program).0
}

fn associated_token_address(owner: &Pubkey, mint: &Pubkey, token_program: &Pubkey) -> Pubkey {
    pda(&[owner.as_ref(), token_program.as_ref(), mint.as_ref()], &ATA_PROGRAM)
}

fn create_ata_idempotent_ix(payer: &Pubkey, mint: &Pubkey, token_program: &Pubkey) -> Instruction {
    let ata = associated_token_address(payer, mint, token_program);
    Instruction::new_with_bytes(
        ATA_PROGRAM,
        &[1],
        vec![
            AccountMeta::new(*payer, true),
            AccountMeta::new(ata, false),
            AccountMeta::new_readonly(*payer, false),
            AccountMeta::new_readonly(*mint, false),
            SYSTEM_PROGRAM_META,
            AccountMeta::new_readonly(*token_program, false),
        ],
    )
}

fn pick<const N: usize>(pool: &[Pubkey; N]) -> Pubkey {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as usize)
        .unwrap_or(0);
    pool[nanos % N]
}

fn u64_at(data: &[u8], offset: usize) -> Result<u64> {
    let bytes = data
        .get(offset..offset + 8)
        .ok_or_else(|| anyhow!("account data too short at offset {offset}"))?;
    Ok(u64::from_le_bytes(bytes.try_into().unwrap()))
}

fn pubkey_at(data: &[u8], offset: usize) -> Result<Pubkey> {
    let bytes = data
        .get(offset..offset + 32)
        .ok_or_else(|| anyhow!("account data too short at offset {offset}"))?;
    Ok(Pubkey::new_from_array(bytes.try_into().unwrap()))
}

fn apply_slippage(amount: u64, slippage_bps: u64) -> u64 {
    let bps = slippage_bps.min(9999);
    amount - (amount as u128 * bps as u128 / 10_000) as u64
}

fn compute_fee(amount: u128, fee_bps: u128) -> u128 {
    let whole = amount / 10_000 * fee_bps;
    let rem = (amount % 10_000) * fee_bps;
    whole + rem / 10_000 + u128::from(rem % 10_000 != 0)
}

// ---- pump.fun ------------------------------------------------------------

struct BondingCurve {
    virtual_token_reserves: u64,
    virtual_sol_reserves: u64,
    real_token_reserves: u64,
    complete: bool,
    creator: Pubkey,
    is_mayhem_mode: bool,
    is_cashback_coin: bool,
    quote_mint: Pubkey,
}

fn parse_bonding_curve(data: &[u8]) -> Result<BondingCurve> {
    Ok(BondingCurve {
        virtual_token_reserves: u64_at(data, 8)?,
        virtual_sol_reserves: u64_at(data, 16)?,
        real_token_reserves: u64_at(data, 24)?,
        complete: data.get(48).copied().unwrap_or(0) != 0,
        creator: pubkey_at(data, 49)?,
        is_mayhem_mode: data.get(81).copied().unwrap_or(0) != 0,
        is_cashback_coin: data.get(82).copied().unwrap_or(0) != 0,
        quote_mint: match data.get(83..115) {
            Some(bytes) => Pubkey::new_from_array(bytes.try_into().unwrap()),
            None => Pubkey::default(),
        },
    })
}

/// Tokens out for `lamports_in` SOL on the bonding curve (reference:
/// get_buy_token_amount_from_sol_amount, buy_exact_sol_in path).
fn pumpfun_expected_tokens(curve: &BondingCurve, lamports_in: u64) -> u64 {
    if lamports_in == 0 || curve.virtual_token_reserves == 0 {
        return 0;
    }
    let fee_bps = PUMPFUN_FEE_BASIS_POINTS
        + if curve.creator != Pubkey::default() { PUMPFUN_CREATOR_FEE_BASIS_POINTS } else { 0 };
    let input = lamports_in as u128 * 10_000 / (10_000 + fee_bps);
    let curve_in = input.saturating_sub(1);
    if curve_in == 0 {
        return 0;
    }
    let denominator = curve.virtual_sol_reserves as u128 + curve_in;
    let tokens = curve_in * curve.virtual_token_reserves as u128 / denominator;
    tokens.min(curve.real_token_reserves as u128).min(u64::MAX as u128) as u64
}

async fn pumpfun_creator_vault(rpc: &RpcClient, creator: &Pubkey, mint: &Pubkey) -> Result<Pubkey> {
    // Fee-sharing coins: the vault is derived from the sharing-config PDA, not
    // the creator (reference: resolve_creator_vault_for_ix_with_fee_sharing).
    let sharing_config = pda(&[b"sharing-config", mint.as_ref()], &FEE_PROGRAM);
    if let Ok(acc) = rpc.get_account(&sharing_config).await {
        let d = acc.data.as_slice();
        let active = acc.owner == FEE_PROGRAM
            && d.len() >= 43
            && d[..8] == SHARING_CONFIG_DISCRIMINATOR
            && d[10] == 1
            && &d[11..43] == mint.as_ref();
        if active {
            return Ok(pda(&[b"creator-vault", sharing_config.as_ref()], &PUMPFUN_PROGRAM));
        }
    }
    if *creator == Pubkey::default() {
        bail!("pumpfun creator unknown, cannot derive creator_vault");
    }
    Ok(pda(&[b"creator-vault", creator.as_ref()], &PUMPFUN_PROGRAM))
}

/// Mint-owner detection per the reference: "pump"-suffixed mints are always
/// Token-2022; otherwise trust the on-chain mint owner, default Token-2022.
async fn pumpfun_token_program(rpc: &RpcClient, mint: &Pubkey) -> Pubkey {
    if mint.to_string().ends_with("pump") {
        return TOKEN_PROGRAM_2022;
    }
    match rpc.get_account(mint).await {
        Ok(acc) if acc.owner == TOKEN_PROGRAM => TOKEN_PROGRAM,
        _ => TOKEN_PROGRAM_2022,
    }
}

async fn pumpfun_buy(
    rpc: &RpcClient,
    payer: Option<&Pubkey>,
    mint: &Pubkey,
    lamports_in: u64,
    slippage_bps: u64,
) -> Result<BuyQuote> {
    if lamports_in == 0 {
        bail!("amount cannot be zero");
    }
    let curve_addr = pda(&[b"bonding-curve", mint.as_ref()], &PUMPFUN_PROGRAM);
    let account = rpc
        .get_account(&curve_addr)
        .await
        .with_context(|| format!("fetch bonding curve {curve_addr}"))?;
    if account.owner != PUMPFUN_PROGRAM {
        bail!("bonding curve {curve_addr} has unexpected owner {}", account.owner);
    }
    let curve = parse_bonding_curve(&account.data)?;
    if curve.complete {
        bail!("bonding curve complete (graduated); buy on pumpswap instead");
    }
    if curve.quote_mint != Pubkey::default() && curve.quote_mint != WSOL_MINT {
        bail!("pumpfun curve is {}-quoted, only SOL quotes supported", curve.quote_mint);
    }

    let expected_tokens = pumpfun_expected_tokens(&curve, lamports_in);
    if expected_tokens == 0 {
        bail!("pumpfun quote is zero for {lamports_in} lamports");
    }
    let min_tokens_out = apply_slippage(expected_tokens, slippage_bps);

    let mut ixs = Vec::new();
    if let Some(payer) = payer {
        let token_program = pumpfun_token_program(rpc, mint).await;
        let creator_vault = pumpfun_creator_vault(rpc, &curve.creator, mint).await?;
        let user_token_account = associated_token_address(payer, mint, &token_program);
        ixs.push(create_ata_idempotent_ix(payer, mint, &token_program));

        let mut data = [0u8; 25];
        data[..8].copy_from_slice(&PUMPFUN_BUY_EXACT_SOL_IN_DISCRIMINATOR);
        data[8..16].copy_from_slice(&lamports_in.to_le_bytes());
        data[16..24].copy_from_slice(&min_tokens_out.to_le_bytes());
        data[24] = u8::from(curve.is_cashback_coin); // track_volume (legacy layout)

        let fee_recipient = if curve.is_mayhem_mode {
            pick(&MAYHEM_FEE_RECIPIENTS)
        } else {
            PUMPFUN_FEE_RECIPIENT
        };
        let metas = vec![
            PUMPFUN_GLOBAL_META,                                   // 0  global
            AccountMeta::new(fee_recipient, false),                // 1  fee_recipient
            AccountMeta::new_readonly(*mint, false),               // 2  mint
            AccountMeta::new(curve_addr, false),                   // 3  bonding_curve
            AccountMeta::new(                                      // 4  associated_bonding_curve
                associated_token_address(&curve_addr, mint, &token_program),
                false,
            ),
            AccountMeta::new(user_token_account, false),           // 5  associated_user
            AccountMeta::new(*payer, true),                        // 6  user (signer)
            SYSTEM_PROGRAM_META,                                   // 7  system_program
            AccountMeta::new_readonly(token_program, false),       // 8  token_program
            AccountMeta::new(creator_vault, false),                // 9  creator_vault
            PUMPFUN_EVENT_AUTHORITY_META,                          // 10 event_authority
            PUMPFUN_PROGRAM_META,                                  // 11 program
            PUMPFUN_GVA_META,                                      // 12 global_volume_accumulator
            AccountMeta::new(                                      // 13 user_volume_accumulator
                pda(&[b"user_volume_accumulator", payer.as_ref()], &PUMPFUN_PROGRAM),
                false,
            ),
            PUMPFUN_FEE_CONFIG_META,                               // 14 fee_config
            FEE_PROGRAM_META,                                      // 15 fee_program
            AccountMeta::new_readonly(                             // 16 bonding_curve_v2
                pda(&[b"bonding-curve-v2", mint.as_ref()], &PUMPFUN_PROGRAM),
                false,
            ),
            AccountMeta::new(pick(&PROTOCOL_EXTRA_FEE_RECIPIENTS), false), // 17 extra fee recipient
        ];
        ixs.push(Instruction::new_with_bytes(PUMPFUN_PROGRAM, &data, metas));
    }

    Ok(BuyQuote { expected_tokens, min_tokens_out, lamports_in, ixs })
}

// ---- PumpSwap --------------------------------------------------------------

#[derive(Debug, Clone)]
struct Pool {
    creator: Pubkey,
    base_mint: Pubkey,
    quote_mint: Pubkey,
    pool_base_token_account: Pubkey,
    pool_quote_token_account: Pubkey,
    lp_supply: u64,
    coin_creator: Pubkey,
    is_mayhem_mode: bool,
    is_cashback_coin: bool,
    virtual_quote_reserves: i128,
}

fn decode_pool(data: &[u8]) -> Result<Pool> {
    if data.get(..8) != Some(&POOL_DISCRIMINATOR[..]) {
        bail!("account discriminator is not a PumpSwap pool");
    }
    let virtual_quote_reserves = match data.get(245..261) {
        Some(bytes) => i128::from_le_bytes(bytes.try_into().unwrap()),
        None => 0, // legacy 244-byte layout predates virtual quote reserves
    };
    Ok(Pool {
        creator: pubkey_at(data, 11)?,
        base_mint: pubkey_at(data, 43)?,
        quote_mint: pubkey_at(data, 75)?,
        pool_base_token_account: pubkey_at(data, 139)?,
        pool_quote_token_account: pubkey_at(data, 171)?,
        lp_supply: u64_at(data, 203)?,
        coin_creator: pubkey_at(data, 211)?,
        is_mayhem_mode: data.get(243).copied().unwrap_or(0) != 0,
        is_cashback_coin: data.get(244).copied().unwrap_or(0) != 0,
        virtual_quote_reserves,
    })
}

async fn fetch_pool(rpc: &RpcClient, address: &Pubkey) -> Result<Pool> {
    let account = rpc.get_account(address).await?;
    if account.owner != AMM_PROGRAM {
        bail!("pool {address} is not owned by the PumpSwap program");
    }
    decode_pool(&account.data)
}

fn pump_pool_authority_pda(mint: &Pubkey) -> Pubkey {
    pda(&[b"pool-authority", mint.as_ref()], &PUMPFUN_PROGRAM)
}

/// Pool discovery per the reference: pool-v2 PDA, canonical migration pool,
/// then a getProgramAccounts scan filtered on base_mint (offset 43).
async fn find_pool(rpc: &RpcClient, mint: &Pubkey) -> Result<(Pubkey, Pool)> {
    let pool_v2 = pda(&[b"pool-v2", mint.as_ref()], &AMM_PROGRAM);
    if let Ok(pool) = fetch_pool(rpc, &pool_v2).await {
        if pool.base_mint == *mint {
            return Ok((pool_v2, pool));
        }
    }
    let canonical = pda(
        &[
            b"pool",
            &0u16.to_le_bytes(),
            pump_pool_authority_pda(mint).as_ref(),
            mint.as_ref(),
            WSOL_MINT.as_ref(),
        ],
        &AMM_PROGRAM,
    );
    if let Ok(pool) = fetch_pool(rpc, &canonical).await {
        if pool.base_mint == *mint {
            return Ok((canonical, pool));
        }
    }

    let scan = async {
        let resp: serde_json::Value = rpc
            .send(
                solana_client::rpc_request::RpcRequest::Custom { method: "getProgramAccounts" },
                serde_json::json!([
                    AMM_PROGRAM.to_string(),
                    {
                        "encoding": "base64",
                        "filters": [
                            { "memcmp": { "offset": 43, "bytes": mint.to_string() } }
                        ]
                    }
                ]),
            )
            .await?;
        let mut best: Option<(Pubkey, Pool)> = None;
        for entry in resp.as_array().into_iter().flatten() {
            let Ok(address) = entry["pubkey"].as_str().unwrap_or_default().parse::<Pubkey>()
            else {
                continue;
            };
            let encoded = entry["account"]["data"][0].as_str().unwrap_or_default();
            let raw = base64::engine::general_purpose::STANDARD.decode(encoded)?;
            if let Ok(pool) = decode_pool(&raw) {
                if pool.base_mint == *mint
                    && best.as_ref().map(|(_, b)| pool.lp_supply > b.lp_supply).unwrap_or(true)
                {
                    best = Some((address, pool));
                }
            }
        }
        best.ok_or_else(|| anyhow!("no PumpSwap pool found for mint {mint}"))
    };
    match tokio::time::timeout(std::time::Duration::from_secs(3), scan).await {
        Ok(result) => result,
        Err(_) => bail!("PumpSwap pool scan timed out for mint {mint}"),
    }
}

fn decode_token_vault(data: &[u8], owner: &Pubkey, expected_mint: &Pubkey) -> Result<u64> {
    if *owner != TOKEN_PROGRAM && *owner != TOKEN_PROGRAM_2022 {
        bail!("pool vault owned by unsupported token program {owner}");
    }
    if data.get(..32) != Some(expected_mint.as_ref()) {
        bail!("pool vault mint mismatch");
    }
    if data.get(108).copied() != Some(1) {
        bail!("pool vault not initialized");
    }
    u64_at(data, 64)
}

#[derive(Clone, Copy)]
struct FeeBasisPoints {
    lp: u64,
    protocol: u64,
    coin_creator: u64,
}

fn decode_fee_config(data: &[u8]) -> Option<(FeeBasisPoints, Vec<(u128, FeeBasisPoints)>)> {
    if data.get(..8) != Some(&PUMPSWAP_FEE_CONFIG_DISCRIMINATOR[..]) {
        return None;
    }
    let fees_at = |o: usize| -> Option<FeeBasisPoints> {
        Some(FeeBasisPoints {
            lp: u64_at(data, o).ok()?,
            protocol: u64_at(data, o + 8).ok()?,
            coin_creator: u64_at(data, o + 16).ok()?,
        })
    };
    let flat = fees_at(41)?; // 8 disc + 1 bump + 32 admin
    let mut offset = 65;
    let tier_count = u32::from_le_bytes(data.get(offset..offset + 4)?.try_into().ok()?) as usize;
    offset += 4;
    let mut tiers = Vec::with_capacity(tier_count);
    for _ in 0..tier_count {
        let threshold = u128::from_le_bytes(data.get(offset..offset + 16)?.try_into().ok()?);
        tiers.push((threshold, fees_at(offset + 16)?));
        offset += 40;
    }
    Some((flat, tiers))
}

/// Fee schedule per the reference: tiered fees from the on-chain fee_config
/// for canonical (pump-created) pools, flat fees otherwise; legacy 25/5/5
/// defaults when the config account cannot be read.
async fn pumpswap_fee_bps(
    rpc: &RpcClient,
    pool: &Pool,
    base_supply: Option<u64>,
    base_reserve: u64,
    quote_reserve: u64,
) -> FeeBasisPoints {
    let legacy = FeeBasisPoints { lp: LP_FEE_BPS, protocol: PROTOCOL_FEE_BPS, coin_creator: COIN_CREATOR_FEE_BPS };
    let Ok(acc) = rpc.get_account(&PUMPSWAP_FEE_CONFIG).await else { return legacy };
    if acc.owner != FEE_PROGRAM {
        return legacy;
    }
    let Some((flat, tiers)) = decode_fee_config(&acc.data) else { return legacy };
    if pool.creator != pump_pool_authority_pda(&pool.base_mint) {
        return flat;
    }
    let (Some(supply), true) = (base_supply, base_reserve != 0) else { return legacy };
    let market_cap = quote_reserve as u128 * supply as u128 / base_reserve as u128;
    tiers
        .iter()
        .rev()
        .find(|(threshold, _)| market_cap >= *threshold)
        .map(|(_, fees)| *fees)
        .unwrap_or(flat)
}

/// Base tokens out for `quote_in` lamports of WSOL (reference:
/// buy_quote_input_internal_with_fees).
fn pumpswap_expected_tokens(
    quote_in: u64,
    base_reserve: u64,
    quote_reserve: u64,
    virtual_quote_reserves: i128,
    fees: &FeeBasisPoints,
) -> Result<u64> {
    if base_reserve == 0 || quote_reserve == 0 {
        bail!("pumpswap pool has zero reserves");
    }
    let effective_reserve = i128::from(quote_reserve)
        .checked_add(virtual_quote_reserves)
        .and_then(|r| u128::try_from(r).ok())
        .filter(|r| *r != 0)
        .ok_or_else(|| anyhow!("invalid effective quote reserves"))?;

    let total_fee_bps = fees.lp + fees.protocol + fees.coin_creator;
    let mut effective_quote = quote_in as u128 * 10_000 / (10_000 + total_fee_bps as u128);
    let total_with_fees = effective_quote
        + compute_fee(effective_quote, fees.lp as u128)
        + compute_fee(effective_quote, fees.protocol as u128)
        + compute_fee(effective_quote, fees.coin_creator as u128);
    if total_with_fees > quote_in as u128 {
        effective_quote = effective_quote
            .checked_sub(total_with_fees - quote_in as u128)
            .ok_or_else(|| anyhow!("quote input too small to cover fees"))?;
    }
    let input = effective_quote
        .checked_sub(1)
        .ok_or_else(|| anyhow!("quote input too small after fees"))?;
    let base_out = base_reserve as u128 * input / (effective_reserve + input);
    u64::try_from(base_out).context("base amount out exceeds u64")
}

fn wrap_wsol_ixs(payer: &Pubkey, lamports: u64) -> [Instruction; 3] {
    let wsol_ata = associated_token_address(payer, &WSOL_MINT, &TOKEN_PROGRAM);
    [
        create_ata_idempotent_ix(payer, &WSOL_MINT, &TOKEN_PROGRAM),
        solami::system_instruction::transfer(payer, &wsol_ata, lamports),
        Instruction::new_with_bytes(
            TOKEN_PROGRAM,
            &[17], // sync_native
            vec![AccountMeta::new(wsol_ata, false)],
        ),
    ]
}

fn close_wsol_ix(payer: &Pubkey) -> Instruction {
    let wsol_ata = associated_token_address(payer, &WSOL_MINT, &TOKEN_PROGRAM);
    Instruction::new_with_bytes(
        TOKEN_PROGRAM,
        &[9], // close_account
        vec![
            AccountMeta::new(wsol_ata, false),
            AccountMeta::new(*payer, false),
            AccountMeta::new(*payer, true),
        ],
    )
}

async fn pumpswap_buy(
    rpc: &RpcClient,
    payer: Option<&Pubkey>,
    mint: &Pubkey,
    lamports_in: u64,
    slippage_bps: u64,
) -> Result<BuyQuote> {
    if lamports_in == 0 {
        bail!("amount cannot be zero");
    }
    let (pool_addr, pool) = find_pool(rpc, mint).await?;
    if pool.quote_mint != WSOL_MINT {
        bail!("pumpswap pool is {}-quoted, only WSOL quotes supported", pool.quote_mint);
    }

    let accounts = rpc
        .get_multiple_accounts(&[
            pool.pool_base_token_account,
            pool.pool_quote_token_account,
            pool.base_mint,
        ])
        .await?;
    let base_vault = accounts.first().and_then(Option::as_ref).context("base vault missing")?;
    let quote_vault = accounts.get(1).and_then(Option::as_ref).context("quote vault missing")?;
    let mint_account = accounts.get(2).and_then(Option::as_ref).context("base mint missing")?;
    let base_reserve = decode_token_vault(&base_vault.data, &base_vault.owner, &pool.base_mint)?;
    let quote_reserve = decode_token_vault(&quote_vault.data, &quote_vault.owner, &pool.quote_mint)?;
    let base_token_program = base_vault.owner;
    let quote_token_program = quote_vault.owner;
    let base_supply = if mint_account.owner == base_token_program {
        u64_at(&mint_account.data, 36).ok()
    } else {
        None
    };

    let fees = pumpswap_fee_bps(rpc, &pool, base_supply, base_reserve, quote_reserve).await;
    let expected_tokens = pumpswap_expected_tokens(
        lamports_in,
        base_reserve,
        quote_reserve,
        pool.virtual_quote_reserves,
        &fees,
    )?;
    let min_tokens_out = apply_slippage(expected_tokens, slippage_bps);

    let mut ixs = Vec::new();
    if let Some(payer) = payer {
        ixs.extend(wrap_wsol_ixs(payer, lamports_in));
        ixs.push(create_ata_idempotent_ix(payer, mint, &base_token_program));

        let user_base_ata = associated_token_address(payer, &pool.base_mint, &base_token_program);
        let user_quote_ata = associated_token_address(payer, &pool.quote_mint, &quote_token_program);
        let fee_recipient = if pool.is_mayhem_mode {
            pick(&MAYHEM_FEE_RECIPIENTS)
        } else {
            PUMPSWAP_PROTOCOL_FEE_RECIPIENT
        };
        let coin_creator_vault_authority = if pool.coin_creator == Pubkey::default() {
            DEFAULT_COIN_CREATOR_VAULT_AUTHORITY
        } else {
            pda(&[b"creator_vault", pool.coin_creator.as_ref()], &AMM_PROGRAM)
        };
        let volume_accumulator =
            pda(&[b"user_volume_accumulator", payer.as_ref()], &AMM_PROGRAM);
        let buyback_recipient = pick(&PROTOCOL_EXTRA_FEE_RECIPIENTS);

        let mut data = [0u8; 25];
        data[..8].copy_from_slice(&PUMPSWAP_BUY_EXACT_QUOTE_IN_DISCRIMINATOR);
        data[8..16].copy_from_slice(&lamports_in.to_le_bytes());
        data[16..24].copy_from_slice(&min_tokens_out.to_le_bytes());
        data[24] = 1; // track_volume

        let mut metas = vec![
            AccountMeta::new(pool_addr, false),                     // 0  pool
            AccountMeta::new(*payer, true),                         // 1  user (signer)
            PUMPSWAP_GLOBAL_META,                                   // 2  global_config
            AccountMeta::new_readonly(pool.base_mint, false),       // 3  base_mint
            AccountMeta::new_readonly(pool.quote_mint, false),      // 4  quote_mint
            AccountMeta::new(user_base_ata, false),                 // 5  user_base_token_account
            AccountMeta::new(user_quote_ata, false),                // 6  user_quote_token_account
            AccountMeta::new(pool.pool_base_token_account, false),  // 7  pool_base_token_account
            AccountMeta::new(pool.pool_quote_token_account, false), // 8  pool_quote_token_account
            AccountMeta::new_readonly(fee_recipient, false),        // 9  protocol_fee_recipient
            AccountMeta::new(                                       // 10 protocol_fee_recipient_ata
                associated_token_address(&fee_recipient, &pool.quote_mint, &quote_token_program),
                false,
            ),
            AccountMeta::new_readonly(base_token_program, false),   // 11 base_token_program
            AccountMeta::new_readonly(quote_token_program, false),  // 12 quote_token_program
            SYSTEM_PROGRAM_META,                                    // 13 system_program
            ATA_PROGRAM_META,                                       // 14 associated_token_program
            PUMPSWAP_EVENT_AUTHORITY_META,                          // 15 event_authority
            AMM_PROGRAM_META,                                       // 16 program
            AccountMeta::new(                                       // 17 coin_creator_vault_ata
                associated_token_address(
                    &coin_creator_vault_authority,
                    &pool.quote_mint,
                    &quote_token_program,
                ),
                false,
            ),
            AccountMeta::new_readonly(coin_creator_vault_authority, false), // 18 coin_creator_vault_authority
            PUMPSWAP_GVA_META,                                      // 19 global_volume_accumulator
            AccountMeta::new(volume_accumulator, false),            // 20 user_volume_accumulator
            PUMPSWAP_FEE_CONFIG_META,                               // 21 fee_config
            FEE_PROGRAM_META,                                       // 22 fee_program
        ];
        if pool.is_cashback_coin {
            metas.push(AccountMeta::new(
                // 23 cashback: quote ATA of the user volume accumulator
                associated_token_address(&volume_accumulator, &pool.quote_mint, &quote_token_program),
                false,
            ));
        }
        if pool.coin_creator != Pubkey::default() {
            // pool-v2 remaining account only when coin_creator is set; a
            // misplaced slot shifts the buyback recipient and errors 6053
            metas.push(AccountMeta::new_readonly(
                pda(&[b"pool-v2", pool.base_mint.as_ref()], &AMM_PROGRAM),
                false,
            ));
        }
        metas.push(AccountMeta::new_readonly(buyback_recipient, false)); // buyback_fee_recipient
        metas.push(AccountMeta::new(
            associated_token_address(&buyback_recipient, &pool.quote_mint, &quote_token_program),
            false,
        )); // buyback_fee_recipient_ata

        ixs.push(Instruction::new_with_bytes(AMM_PROGRAM, &data, metas));
        ixs.push(close_wsol_ix(payer));
    }

    Ok(BuyQuote { expected_tokens, min_tokens_out, lamports_in, ixs })
}
