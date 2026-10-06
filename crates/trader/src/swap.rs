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
const PUMPFUN_SELL_DISCRIMINATOR: [u8; 8] = [51, 230, 133, 164, 1, 127, 131, 173];
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
const PUMPSWAP_SELL_DISCRIMINATOR: [u8; 8] = [51, 230, 133, 164, 1, 127, 131, 173];
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

/// Map a venue name to an execution plan. Venue strings come from two
/// sources with different spellings (Blur stream: "pump.fun"; gRPC tracker:
/// "pumpswap"), so normalize case and strip separators before matching. An
/// empty dex with a pump.fun-style mint (suffix "pump") is treated as a
/// bonding curve — that suffix is only assigned to pump.fun mints.
pub fn plan_for_dex(dex: &str, mint: &str) -> SwapPlan {
    let norm: String = dex
        .chars()
        .filter(|c| !matches!(c, '.' | '_' | '-'))
        .flat_map(char::to_lowercase)
        .collect();
    match norm.as_str() {
        "pumpfun" => SwapPlan::Pumpfun,
        "pumpswap" => SwapPlan::Pumpswap,
        "" if mint.ends_with("pump") => SwapPlan::Pumpfun,
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
    match plan_for_dex(dex, &mint.to_string()) {
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
    let quote = match plan_for_dex(dex, &mint.to_string()) {
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
    whole + rem / 10_000 + u128::from(!rem.is_multiple_of(10_000))
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
async fn mint_token_program(rpc: &RpcClient, mint: &Pubkey) -> Pubkey {
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
        let token_program = mint_token_program(rpc, mint).await;
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
    let snap = load_pool_snapshot(rpc, mint).await?;
    let pool_addr = snap.address;
    let pool = &snap.pool;
    let base_token_program = snap.base_token_program;
    let quote_token_program = snap.quote_token_program;

    let expected_tokens = pumpswap_expected_tokens(
        lamports_in,
        snap.base_reserve,
        snap.quote_reserve,
        pool.virtual_quote_reserves,
        &snap.fees,
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

// ---- shared pool snapshot -------------------------------------------------

struct PoolSnapshot {
    address: Pubkey,
    pool: Pool,
    base_reserve: u64,
    quote_reserve: u64,
    base_token_program: Pubkey,
    quote_token_program: Pubkey,
    fees: FeeBasisPoints,
}

async fn load_pool_snapshot(rpc: &RpcClient, mint: &Pubkey) -> Result<PoolSnapshot> {
    let (address, pool) = find_pool(rpc, mint).await?;
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
    let base_supply = if mint_account.owner == base_vault.owner {
        u64_at(&mint_account.data, 36).ok()
    } else {
        None
    };
    let fees = pumpswap_fee_bps(rpc, &pool, base_supply, base_reserve, quote_reserve).await;
    Ok(PoolSnapshot {
        address,
        pool,
        base_reserve,
        quote_reserve,
        base_token_program: base_vault.owner,
        quote_token_program: quote_vault.owner,
        fees,
    })
}

// ---- sells -----------------------------------------------------------------

#[derive(Debug)]
pub struct SellQuote {
    pub tokens_in: u64,
    pub expected_lamports_out: u64,
    pub min_lamports_out: u64,
    pub ixs: Vec<Instruction>,
}

/// Full sell construction for live mode. The token amount is the payer's
/// current ATA balance. A pump.fun position whose curve has completed is
/// re-routed to PumpSwap automatically (the pool exists post-graduation).
pub async fn build_sell_ixs(
    rpc: &RpcClient,
    payer: &Pubkey,
    mint: &Pubkey,
    dex: &str,
    slippage_bps: u64,
) -> Result<SellQuote> {
    match plan_for_dex(dex, &mint.to_string()) {
        SwapPlan::Pumpfun => match pumpfun_sell(rpc, payer, mint, slippage_bps).await {
            Ok(q) => Ok(q),
            Err(e) if is_graduated(&e) => pumpswap_sell(rpc, payer, mint, slippage_bps).await,
            Err(e) => Err(e),
        },
        SwapPlan::Pumpswap => pumpswap_sell(rpc, payer, mint, slippage_bps).await,
        SwapPlan::Unsupported => Err(unsupported(dex)),
    }
}

fn is_graduated(e: &anyhow::Error) -> bool {
    e.to_string().contains("bonding curve complete")
}

/// Current ATA balance of `payer` for `mint` (0 when the account is gone).
pub async fn ata_balance(rpc: &RpcClient, payer: &Pubkey, mint: &Pubkey) -> Option<u64> {
    let token_program = mint_token_program(rpc, mint).await;
    let ata = associated_token_address(payer, mint, &token_program);
    let balance = rpc.get_token_account_balance(&ata).await.ok()?;
    balance.amount.parse::<u64>().ok()
}

/// SOL out for selling `tokens_in` on the bonding curve (reference:
/// get_sell_sol_amount_from_token_amount).
fn pumpfun_expected_sol_out(curve: &BondingCurve, tokens_in: u64) -> u64 {
    if tokens_in == 0 || curve.virtual_token_reserves == 0 {
        return 0;
    }
    let numerator = tokens_in as u128 * curve.virtual_sol_reserves as u128;
    let sol_cost = numerator / (curve.virtual_token_reserves as u128 + tokens_in as u128);
    let fee_bps = PUMPFUN_FEE_BASIS_POINTS
        + if curve.creator != Pubkey::default() { PUMPFUN_CREATOR_FEE_BASIS_POINTS } else { 0 };
    sol_cost.saturating_sub(compute_fee(sol_cost, fee_bps)).min(u64::MAX as u128) as u64
}

async fn pumpfun_sell(
    rpc: &RpcClient,
    payer: &Pubkey,
    mint: &Pubkey,
    slippage_bps: u64,
) -> Result<SellQuote> {
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
        bail!("bonding curve complete (graduated); sell on pumpswap instead");
    }
    if curve.quote_mint != Pubkey::default() && curve.quote_mint != WSOL_MINT {
        bail!("pumpfun curve is {}-quoted, only SOL quotes supported", curve.quote_mint);
    }

    let token_program = mint_token_program(rpc, mint).await;
    let user_token_account = associated_token_address(payer, mint, &token_program);
    let tokens_in = ata_balance(rpc, payer, mint)
        .await
        .ok_or_else(|| anyhow!("no token account for {mint}"))?;
    if tokens_in == 0 {
        bail!("token balance is zero, nothing to sell");
    }

    let expected_lamports_out = pumpfun_expected_sol_out(&curve, tokens_in);
    if expected_lamports_out == 0 {
        bail!("pumpfun sell quote is zero for {tokens_in} tokens");
    }
    let min_lamports_out = apply_slippage(expected_lamports_out, slippage_bps);
    let creator_vault = pumpfun_creator_vault(rpc, &curve.creator, mint).await?;

    let mut data = [0u8; 24];
    data[..8].copy_from_slice(&PUMPFUN_SELL_DISCRIMINATOR);
    data[8..16].copy_from_slice(&tokens_in.to_le_bytes());
    data[16..24].copy_from_slice(&min_lamports_out.to_le_bytes());

    let fee_recipient = if curve.is_mayhem_mode {
        pick(&MAYHEM_FEE_RECIPIENTS)
    } else {
        PUMPFUN_FEE_RECIPIENT
    };
    // Sell account order differs from buy: creator_vault and token_program
    // swap slots (8/9) and there is no global volume accumulator.
    let mut metas = vec![
        PUMPFUN_GLOBAL_META,                             // 0  global
        AccountMeta::new(fee_recipient, false),          // 1  fee_recipient
        AccountMeta::new_readonly(*mint, false),         // 2  mint
        AccountMeta::new(curve_addr, false),             // 3  bonding_curve
        AccountMeta::new(                                // 4  associated_bonding_curve
            associated_token_address(&curve_addr, mint, &token_program),
            false,
        ),
        AccountMeta::new(user_token_account, false),     // 5  associated_user
        AccountMeta::new(*payer, true),                  // 6  user (signer)
        SYSTEM_PROGRAM_META,                             // 7  system_program
        AccountMeta::new(creator_vault, false),          // 8  creator_vault
        AccountMeta::new_readonly(token_program, false), // 9  token_program
        PUMPFUN_EVENT_AUTHORITY_META,                    // 10 event_authority
        PUMPFUN_PROGRAM_META,                            // 11 program
        PUMPFUN_FEE_CONFIG_META,                         // 12 fee_config
        FEE_PROGRAM_META,                                // 13 fee_program
    ];
    if curve.is_cashback_coin {
        metas.push(AccountMeta::new(
            // 14 cashback coins carry the user volume accumulator
            pda(&[b"user_volume_accumulator", payer.as_ref()], &PUMPFUN_PROGRAM),
            false,
        ));
    }
    metas.push(AccountMeta::new_readonly(
        // bonding_curve_v2
        pda(&[b"bonding-curve-v2", mint.as_ref()], &PUMPFUN_PROGRAM),
        false,
    ));
    metas.push(AccountMeta::new(pick(&PROTOCOL_EXTRA_FEE_RECIPIENTS), false)); // extra fee recipient

    let ixs = vec![Instruction::new_with_bytes(PUMPFUN_PROGRAM, &data, metas)];
    Ok(SellQuote { tokens_in, expected_lamports_out, min_lamports_out, ixs })
}

/// Lamports out for selling `tokens_in` base on PumpSwap (reference:
/// sell_base_input_internal_with_fees).
fn pumpswap_expected_sol_out(
    tokens_in: u64,
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

    let quote_out = effective_reserve * tokens_in as u128 / (base_reserve as u128 + tokens_in as u128);
    let lp_fee = compute_fee(quote_out, fees.lp as u128);
    let protocol_fee = compute_fee(quote_out, fees.protocol as u128);
    let creator_fee = compute_fee(quote_out, fees.coin_creator as u128);
    let total_fees = lp_fee + protocol_fee + creator_fee;
    if total_fees > quote_out {
        bail!("fees exceed sell output");
    }
    if quote_out - lp_fee > quote_reserve as u128 {
        bail!("insufficient real quote reserves for the sell output");
    }
    u64::try_from(quote_out - total_fees).context("sell output exceeds u64")
}

async fn pumpswap_sell(
    rpc: &RpcClient,
    payer: &Pubkey,
    mint: &Pubkey,
    slippage_bps: u64,
) -> Result<SellQuote> {
    let snap = load_pool_snapshot(rpc, mint).await?;
    let pool = &snap.pool;

    let user_base_ata = associated_token_address(payer, &pool.base_mint, &snap.base_token_program);
    let balance = rpc.get_token_account_balance(&user_base_ata).await.ok();
    let tokens_in = balance
        .and_then(|b| b.amount.parse::<u64>().ok())
        .ok_or_else(|| anyhow!("no token account for {mint}"))?;
    if tokens_in == 0 {
        bail!("token balance is zero, nothing to sell");
    }

    let expected_lamports_out = pumpswap_expected_sol_out(
        tokens_in,
        snap.base_reserve,
        snap.quote_reserve,
        pool.virtual_quote_reserves,
        &snap.fees,
    )?;
    let min_lamports_out = apply_slippage(expected_lamports_out, slippage_bps);

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
    let volume_accumulator = pda(&[b"user_volume_accumulator", payer.as_ref()], &AMM_PROGRAM);
    let buyback_recipient = pick(&PROTOCOL_EXTRA_FEE_RECIPIENTS);
    let user_quote_ata = associated_token_address(payer, &pool.quote_mint, &snap.quote_token_program);

    let mut data = [0u8; 24];
    data[..8].copy_from_slice(&PUMPSWAP_SELL_DISCRIMINATOR);
    data[8..16].copy_from_slice(&tokens_in.to_le_bytes());
    data[16..24].copy_from_slice(&min_lamports_out.to_le_bytes());

    let mut metas = vec![
        AccountMeta::new(snap.address, false),                      // 0  pool
        AccountMeta::new(*payer, true),                             // 1  user (signer)
        PUMPSWAP_GLOBAL_META,                                       // 2  global_config
        AccountMeta::new_readonly(pool.base_mint, false),           // 3  base_mint
        AccountMeta::new_readonly(pool.quote_mint, false),          // 4  quote_mint
        AccountMeta::new(user_base_ata, false),                     // 5  user_base_token_account
        AccountMeta::new(user_quote_ata, false),                    // 6  user_quote_token_account
        AccountMeta::new(pool.pool_base_token_account, false),      // 7  pool_base_token_account
        AccountMeta::new(pool.pool_quote_token_account, false),     // 8  pool_quote_token_account
        AccountMeta::new_readonly(fee_recipient, false),            // 9  protocol_fee_recipient
        AccountMeta::new(                                           // 10 protocol_fee_recipient_ata
            associated_token_address(&fee_recipient, &pool.quote_mint, &snap.quote_token_program),
            false,
        ),
        AccountMeta::new_readonly(snap.base_token_program, false),  // 11 base_token_program
        AccountMeta::new_readonly(snap.quote_token_program, false), // 12 quote_token_program
        SYSTEM_PROGRAM_META,                                        // 13 system_program
        ATA_PROGRAM_META,                                           // 14 associated_token_program
        PUMPSWAP_EVENT_AUTHORITY_META,                              // 15 event_authority
        AMM_PROGRAM_META,                                           // 16 program
        AccountMeta::new(                                           // 17 coin_creator_vault_ata
            associated_token_address(
                &coin_creator_vault_authority,
                &pool.quote_mint,
                &snap.quote_token_program,
            ),
            false,
        ),
        AccountMeta::new_readonly(coin_creator_vault_authority, false), // 18 coin_creator_vault_authority
        // WSOL-quote sell: no global/user volume accumulator (those sit on buys)
        PUMPSWAP_FEE_CONFIG_META,                                   // 19 fee_config
        FEE_PROGRAM_META,                                           // 20 fee_program
    ];
    if pool.is_cashback_coin {
        metas.push(AccountMeta::new(
            // 21 cashback: quote ATA of the user volume accumulator
            associated_token_address(&volume_accumulator, &pool.quote_mint, &snap.quote_token_program),
            false,
        ));
        metas.push(AccountMeta::new(volume_accumulator, false)); // 22 user_volume_accumulator
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
        associated_token_address(&buyback_recipient, &pool.quote_mint, &snap.quote_token_program),
        false,
    )); // buyback_fee_recipient_ata

    let ixs = vec![
        // the WSOL output account must exist before the sell
        create_ata_idempotent_ix(payer, &pool.quote_mint, &snap.quote_token_program),
        Instruction::new_with_bytes(AMM_PROGRAM, &data, metas),
        close_wsol_ix(payer),
    ];
    Ok(SellQuote { tokens_in, expected_lamports_out, min_lamports_out, ixs })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fresh pump.fun curve shape: 30 SOL virtual, ~1.073e15 virtual tokens,
    /// ~7.931e14 real tokens.
    fn fresh_curve(creator: Pubkey) -> BondingCurve {
        BondingCurve {
            virtual_token_reserves: 1_073_000_000_000_000,
            virtual_sol_reserves: 30_000_000_000,
            real_token_reserves: 793_100_000_000_000,
            complete: false,
            creator,
            is_mayhem_mode: false,
            is_cashback_coin: false,
            quote_mint: Pubkey::default(),
        }
    }

    const LEGACY_FEES: FeeBasisPoints = FeeBasisPoints { lp: 25, protocol: 5, coin_creator: 5 };

    // ---- plan_for_dex --------------------------------------------------------

    #[test]
    fn plan_for_dex_accepts_observed_venue_spellings() {
        // Values observed on the live Blur stream and the gRPC wallet tracker.
        assert!(matches!(plan_for_dex("pumpfun", ""), SwapPlan::Pumpfun));
        assert!(matches!(plan_for_dex("pump.fun", ""), SwapPlan::Pumpfun));
        assert!(matches!(plan_for_dex("PUMP.FUN", ""), SwapPlan::Pumpfun));
        assert!(matches!(plan_for_dex("pump_fun", ""), SwapPlan::Pumpfun));
        assert!(matches!(plan_for_dex("pump-fun", ""), SwapPlan::Pumpfun));
        assert!(matches!(plan_for_dex("pumpswap", ""), SwapPlan::Pumpswap));
        assert!(matches!(plan_for_dex("pump.swap", ""), SwapPlan::Pumpswap));
        assert!(matches!(plan_for_dex("PumpSwap", ""), SwapPlan::Pumpswap));
    }

    #[test]
    fn plan_for_dex_rejects_venues_we_cannot_route() {
        assert!(matches!(plan_for_dex("raydium", ""), SwapPlan::Unsupported));
        assert!(matches!(plan_for_dex("raydium_clmm", ""), SwapPlan::Unsupported));
        assert!(matches!(plan_for_dex("orca_whirlpool", ""), SwapPlan::Unsupported));
        assert!(matches!(plan_for_dex("meteora_dlmm", ""), SwapPlan::Unsupported));
        assert!(matches!(plan_for_dex("meteora", ""), SwapPlan::Unsupported));
        assert!(matches!(plan_for_dex("manifest", ""), SwapPlan::Unsupported));
        assert!(matches!(plan_for_dex("alphaq", ""), SwapPlan::Unsupported));
    }

    #[test]
    fn plan_for_dex_empty_venue_falls_back_to_mint_suffix() {
        // pump.fun mints end with the literal "pump" suffix.
        assert!(matches!(
            plan_for_dex("", "9Qmz39S4LvrtsBg9MuUPgdZKfXoYYcggZKjAsFQ7pump"),
            SwapPlan::Pumpfun
        ));
        assert!(matches!(
            plan_for_dex("", "So11111111111111111111111111111111111111112"),
            SwapPlan::Unsupported
        ));
        assert!(matches!(plan_for_dex("", ""), SwapPlan::Unsupported));
    }

    #[test]
    fn plan_for_dex_known_venue_name_beats_mint_suffix() {
        // A pump-suffix mint on an unsupported venue stays unsupported: the
        // explicit venue name is authoritative, the suffix is only a fallback.
        assert!(matches!(
            plan_for_dex("raydium", "9Qmz39S4LvrtsBg9MuUPgdZKfXoYYcggZKjAsFQ7pump"),
            SwapPlan::Unsupported
        ));
    }

    // ---- compute_fee -------------------------------------------------------

    #[test]
    fn compute_fee_exact_divisions() {
        assert_eq!(compute_fee(0, 95), 0);
        assert_eq!(compute_fee(10_000, 25), 25);
        assert_eq!(compute_fee(1_000_000, 95), 9_500);
        assert_eq!(compute_fee(123, 0), 0); // zero fee never rounds up
    }

    #[test]
    fn compute_fee_rounds_up_on_remainder() {
        assert_eq!(compute_fee(1, 1), 1); // 0.0001 -> ceil 1
        assert_eq!(compute_fee(10_001, 25), 26); // 25.0025 -> 26
        assert_eq!(compute_fee(1, 10_000), 1); // 100% of 1
    }

    #[test]
    fn compute_fee_extremes_do_not_overflow() {
        // Full fee on u64::MAX is exact, no ceil artifact.
        assert_eq!(compute_fee(u64::MAX as u128, 10_000), u64::MAX as u128);
        assert_eq!(compute_fee(u128::MAX, 10_000), u128::MAX);
        let _ = compute_fee(u128::MAX, 9_999); // must not panic
    }

    // ---- apply_slippage ----------------------------------------------------

    #[test]
    fn apply_slippage_scales_down() {
        assert_eq!(apply_slippage(1_000, 1_500), 850);
        assert_eq!(apply_slippage(1_000, 0), 1_000);
        assert_eq!(apply_slippage(0, 5_000), 0);
    }

    #[test]
    fn apply_slippage_caps_at_9999_bps() {
        assert_eq!(apply_slippage(10_000, 9_999), 1);
        assert_eq!(apply_slippage(10_000, 20_000), 1); // clamped, never negative
    }

    // ---- pump.fun quotes -----------------------------------------------------

    #[test]
    fn pumpfun_buy_quote_on_fresh_curve() {
        // Non-default creator -> 95 + 30 = 125 bps.
        let curve = fresh_curve(Pubkey::new_unique());
        let tokens = pumpfun_expected_tokens(&curve, 20_000_000); // 0.02 SOL
        assert!(
            (650_000_000_000..=750_000_000_000).contains(&tokens),
            "0.02 SOL on a fresh curve should buy ~7.06e11 tokens, got {tokens}"
        );
        assert!(tokens < curve.real_token_reserves);
    }

    #[test]
    fn pumpfun_buy_quote_creator_fee_tiers() {
        // Default creator pays only 95 bps -> more tokens than the 125 bps path.
        let no_creator_fee = pumpfun_expected_tokens(&fresh_curve(Pubkey::default()), 20_000_000);
        let with_creator_fee = pumpfun_expected_tokens(&fresh_curve(Pubkey::new_unique()), 20_000_000);
        assert!(no_creator_fee > with_creator_fee);
    }

    #[test]
    fn pumpfun_buy_quote_capped_by_real_reserves() {
        let mut curve = fresh_curve(Pubkey::default());
        curve.real_token_reserves = 1;
        assert_eq!(pumpfun_expected_tokens(&curve, 20_000_000), 1);
    }

    #[test]
    fn pumpfun_buy_quote_zero_edges() {
        assert_eq!(pumpfun_expected_tokens(&fresh_curve(Pubkey::default()), 0), 0);
        let mut empty = fresh_curve(Pubkey::default());
        empty.virtual_token_reserves = 0;
        assert_eq!(pumpfun_expected_tokens(&empty, 20_000_000), 0);
    }

    #[test]
    fn pumpfun_sell_quote_round_trips_fresh_curve() {
        // Selling back the ~7e11 tokens a 0.02 SOL buy gets returns ~0.0194 SOL
        // (gross ~0.0196 minus 95 bps).
        let curve = fresh_curve(Pubkey::default());
        let tokens = pumpfun_expected_tokens(&curve, 20_000_000);
        let sol = pumpfun_expected_sol_out(&curve, tokens);
        assert!(
            (18_000_000..=20_000_000).contains(&sol),
            "round-trip should return ~0.019 SOL, got {sol}"
        );
        assert!(sol < 20_000_000, "fees and price impact make the round-trip lossy");
    }

    #[test]
    fn pumpfun_sell_quote_zero_edges() {
        assert_eq!(pumpfun_expected_sol_out(&fresh_curve(Pubkey::default()), 0), 0);
        let mut empty = fresh_curve(Pubkey::default());
        empty.virtual_token_reserves = 0;
        assert_eq!(pumpfun_expected_sol_out(&empty, 100), 0);
    }

    // ---- PumpSwap quotes -----------------------------------------------------

    #[test]
    fn pumpswap_buy_quote_constant_product() {
        // 2:1 pool, no virtual offset: eff = 100000*10000/10035 ~= 99651,
        // input = eff - 1, base_out = 1e9 * 99650 / (2e9 + 99650) ~= 49_802.
        let base_out =
            pumpswap_expected_tokens(100_000, 1_000_000_000, 2_000_000_000, 0, &LEGACY_FEES)
                .unwrap();
        assert!((49_000..=51_000).contains(&base_out), "got {base_out}");
    }

    #[test]
    fn pumpswap_buy_quote_fee_tiers() {
        let legacy =
            pumpswap_expected_tokens(100_000, 1_000_000_000, 2_000_000_000, 0, &LEGACY_FEES)
                .unwrap();
        let high = FeeBasisPoints { lp: 100, protocol: 50, coin_creator: 50 };
        let hi = pumpswap_expected_tokens(100_000, 1_000_000_000, 2_000_000_000, 0, &high).unwrap();
        assert!(hi < legacy, "higher fees must yield fewer tokens");

        let free = FeeBasisPoints { lp: 0, protocol: 0, coin_creator: 0 };
        let out = pumpswap_expected_tokens(100_000, 1_000_000_000, 2_000_000_000, 0, &free).unwrap();
        assert!((49_900..=50_100).contains(&out), "zero-fee pool ~= 49_997, got {out}");
    }

    #[test]
    fn pumpswap_buy_quote_virtual_quote_offset() {
        // A negative virtual offset shrinks the effective quote reserve, so the
        // same quote input moves the constant-product curve further: more base out.
        let plain =
            pumpswap_expected_tokens(100_000, 1_000_000_000, 2_000_000_000, 0, &LEGACY_FEES)
                .unwrap();
        let offset = pumpswap_expected_tokens(
            100_000,
            1_000_000_000,
            2_000_000_000,
            -500_000_000,
            &LEGACY_FEES,
        )
        .unwrap();
        assert!(offset > plain);
    }

    #[test]
    fn pumpswap_quotes_reject_zero_or_invalid_reserves() {
        assert!(pumpswap_expected_tokens(100_000, 0, 2_000_000_000, 0, &LEGACY_FEES).is_err());
        assert!(pumpswap_expected_tokens(100_000, 1_000_000_000, 0, 0, &LEGACY_FEES).is_err());
        // Virtual offset that zeroes the effective quote reserve.
        assert!(pumpswap_expected_tokens(100_000, 1_000_000_000, 1_000, -1_000, &LEGACY_FEES).is_err());
        // And one that pushes it negative.
        assert!(pumpswap_expected_tokens(100_000, 1_000_000_000, 1_000, -2_000, &LEGACY_FEES).is_err());
        assert!(pumpswap_expected_sol_out(100_000, 0, 2_000_000_000, 0, &LEGACY_FEES).is_err());
        assert!(pumpswap_expected_sol_out(100_000, 1_000_000_000, 0, 0, &LEGACY_FEES).is_err());
    }

    #[test]
    fn pumpswap_sell_quote_constant_product() {
        // quote_out = 2e9 * 50_000 / (1e9 + 50_000) = 99_995, minus 35 bps of
        // fees -> 99_645.
        let sol =
            pumpswap_expected_sol_out(50_000, 1_000_000_000, 2_000_000_000, 0, &LEGACY_FEES)
                .unwrap();
        assert!((99_000..=100_000).contains(&sol), "got {sol}");
    }

    #[test]
    fn pumpswap_sell_quote_rejects_reserve_draining_output() {
        // Selling the entire base reserve would pay out more quote than the
        // pool really holds (virtual part is not withdrawable).
        assert!(
            pumpswap_expected_sol_out(1_000_000_000, 1_000_000_000, 1_000, 999_999_000, &LEGACY_FEES)
                .is_err()
        );
    }

    // ---- PDA derivation -----------------------------------------------------

    #[test]
    fn pumpfun_bonding_curve_pda_matches_mainnet() {
        // Verified against mainnet: curve account owner = pump.fun program.
        let mint = Pubkey::from_str_const("E3JvmGcGFDzhu2Cnxyeq5BRvN7HH9JZUsfAUh2v8pump");
        let curve = pda(&[b"bonding-curve", mint.as_ref()], &PUMPFUN_PROGRAM);
        assert_eq!(curve.to_string(), "64N8p9crJJiQpayP8hUGbRL9dqf3DikT5ccXUNNjfTx1");
    }

    #[test]
    fn pumpswap_pool_v2_pda_is_deterministic_and_distinct() {
        let mint = Pubkey::from_str_const("E3JvmGcGFDzhu2Cnxyeq5BRvN7HH9JZUsfAUh2v8pump");
        let pool_v2 = pda(&[b"pool-v2", mint.as_ref()], &AMM_PROGRAM);
        assert_eq!(pool_v2, pda(&[b"pool-v2", mint.as_ref()], &AMM_PROGRAM));
        assert_ne!(pool_v2, Pubkey::default());
        assert_ne!(pool_v2, pda(&[b"bonding-curve", mint.as_ref()], &PUMPFUN_PROGRAM));
        // Same seed under the wrong program must not collide.
        assert_ne!(pool_v2, pda(&[b"pool-v2", mint.as_ref()], &PUMPFUN_PROGRAM));
    }

    #[test]
    fn creator_vault_pda_depends_on_creator_and_seed_spelling() {
        let c1 = Pubkey::new_unique();
        let c2 = Pubkey::new_unique();
        let v1 = pda(&[b"creator-vault", c1.as_ref()], &PUMPFUN_PROGRAM);
        assert_eq!(v1, pda(&[b"creator-vault", c1.as_ref()], &PUMPFUN_PROGRAM));
        assert_ne!(v1, pda(&[b"creator-vault", c2.as_ref()], &PUMPFUN_PROGRAM));
        // pump.fun uses "creator-vault", PumpSwap uses "creator_vault" — a
        // one-character seed difference must yield different addresses.
        assert_ne!(v1, pda(&[b"creator_vault", c1.as_ref()], &AMM_PROGRAM));
    }

    // ---- discriminators / instruction data layout ---------------------------

    #[test]
    fn discriminator_constants_match_reference_sdk() {
        // Pinned against 0xfnzero/sol-trade-sdk @ main (anchor sighashes).
        assert_eq!(PUMPFUN_BUY_EXACT_SOL_IN_DISCRIMINATOR, [56, 252, 116, 8, 158, 223, 205, 95]);
        assert_eq!(PUMPFUN_SELL_DISCRIMINATOR, [51, 230, 133, 164, 1, 127, 131, 173]);
        assert_eq!(PUMPSWAP_BUY_EXACT_QUOTE_IN_DISCRIMINATOR, [198, 46, 21, 82, 180, 217, 232, 112]);
        assert_eq!(POOL_DISCRIMINATOR, [241, 154, 109, 4, 17, 177, 109, 188]);
        assert_eq!(PUMPSWAP_FEE_CONFIG_DISCRIMINATOR, [143, 52, 146, 187, 219, 123, 76, 155]);
        // Both programs anchor their sell as `global:sell` -> identical sighash.
        assert_eq!(PUMPSWAP_SELL_DISCRIMINATOR, PUMPFUN_SELL_DISCRIMINATOR);
    }

    // ---- account decoding (u64 LE fixtures mirror the ix data encoding) -----

    #[test]
    fn parse_bonding_curve_reads_reference_layout() {
        let mut data = vec![0u8; 115];
        data[..8].copy_from_slice(&[1u8; 8]); // discriminator (not checked)
        data[8..16].copy_from_slice(&1_073_000_000_000_000u64.to_le_bytes());
        data[16..24].copy_from_slice(&30_000_000_000u64.to_le_bytes());
        data[24..32].copy_from_slice(&793_100_000_000_000u64.to_le_bytes());
        data[48] = 1; // complete
        let creator = Pubkey::new_unique();
        data[49..81].copy_from_slice(creator.as_ref());
        data[81] = 1; // is_mayhem_mode
        let quote_mint = Pubkey::new_unique();
        data[83..115].copy_from_slice(quote_mint.as_ref());

        let c = parse_bonding_curve(&data).unwrap();
        assert_eq!(c.virtual_token_reserves, 1_073_000_000_000_000);
        assert_eq!(c.virtual_sol_reserves, 30_000_000_000);
        assert_eq!(c.real_token_reserves, 793_100_000_000_000);
        assert!(c.complete);
        assert_eq!(c.creator, creator);
        assert!(c.is_mayhem_mode);
        assert!(!c.is_cashback_coin);
        assert_eq!(c.quote_mint, quote_mint);
    }

    #[test]
    fn parse_bonding_curve_legacy_short_layout_defaults_quote_mint() {
        let data = vec![0u8; 83]; // predates the quote_mint field
        let c = parse_bonding_curve(&data).unwrap();
        assert_eq!(c.quote_mint, Pubkey::default());
        assert!(!c.is_mayhem_mode);
        assert!(!c.is_cashback_coin);
    }

    #[test]
    fn decode_pool_reads_reference_layout() {
        let mut data = vec![0u8; 261];
        data[..8].copy_from_slice(&POOL_DISCRIMINATOR);
        let creator = Pubkey::new_unique();
        let base = Pubkey::new_unique();
        let pool_base = Pubkey::new_unique();
        let pool_quote = Pubkey::new_unique();
        let coin_creator = Pubkey::new_unique();
        data[11..43].copy_from_slice(creator.as_ref());
        data[43..75].copy_from_slice(base.as_ref());
        data[75..107].copy_from_slice(WSOL_MINT.as_ref());
        data[139..171].copy_from_slice(pool_base.as_ref());
        data[171..203].copy_from_slice(pool_quote.as_ref());
        data[203..211].copy_from_slice(&123_456u64.to_le_bytes());
        data[211..243].copy_from_slice(coin_creator.as_ref());
        data[243] = 1; // is_mayhem_mode
        data[245..261].copy_from_slice(&(-42i128).to_le_bytes());

        let p = decode_pool(&data).unwrap();
        assert_eq!(p.creator, creator);
        assert_eq!(p.base_mint, base);
        assert_eq!(p.quote_mint, WSOL_MINT);
        assert_eq!(p.pool_base_token_account, pool_base);
        assert_eq!(p.pool_quote_token_account, pool_quote);
        assert_eq!(p.lp_supply, 123_456);
        assert_eq!(p.coin_creator, coin_creator);
        assert!(p.is_mayhem_mode);
        assert!(!p.is_cashback_coin);
        assert_eq!(p.virtual_quote_reserves, -42);
    }

    #[test]
    fn decode_pool_rejects_wrong_discriminator_and_supports_legacy_len() {
        assert!(decode_pool(&[0u8; 261]).is_err());
        let mut legacy = vec![0u8; 244]; // predates virtual_quote_reserves
        legacy[..8].copy_from_slice(&POOL_DISCRIMINATOR);
        let p = decode_pool(&legacy).unwrap();
        assert_eq!(p.virtual_quote_reserves, 0);
    }

    #[test]
    fn decode_fee_config_reads_flat_fees_and_tiers() {
        let mut data = vec![0u8; 8 + 1 + 32 + 24 + 4 + 40 * 2];
        data[..8].copy_from_slice(&PUMPSWAP_FEE_CONFIG_DISCRIMINATOR);
        // flat fees at offset 41 (8 disc + 1 bump + 32 admin)
        data[41..49].copy_from_slice(&30u64.to_le_bytes());
        data[49..57].copy_from_slice(&6u64.to_le_bytes());
        data[57..65].copy_from_slice(&6u64.to_le_bytes());
        data[65..69].copy_from_slice(&2u32.to_le_bytes()); // tier count
        // tier 0 at 69: threshold 1_000_000, fees 25/5/5
        data[69..85].copy_from_slice(&1_000_000u128.to_le_bytes());
        data[85..93].copy_from_slice(&25u64.to_le_bytes());
        data[93..101].copy_from_slice(&5u64.to_le_bytes());
        data[101..109].copy_from_slice(&5u64.to_le_bytes());
        // tier 1 at 109: threshold 10_000_000, fees 10/2/2
        data[109..125].copy_from_slice(&10_000_000u128.to_le_bytes());
        data[125..133].copy_from_slice(&10u64.to_le_bytes());
        data[133..141].copy_from_slice(&2u64.to_le_bytes());
        data[141..149].copy_from_slice(&2u64.to_le_bytes());

        let (flat, tiers) = decode_fee_config(&data).unwrap();
        assert_eq!((flat.lp, flat.protocol, flat.coin_creator), (30, 6, 6));
        assert_eq!(tiers.len(), 2);
        assert_eq!(tiers[0].0, 1_000_000);
        assert_eq!((tiers[0].1.lp, tiers[0].1.protocol, tiers[0].1.coin_creator), (25, 5, 5));
        assert_eq!(tiers[1].0, 10_000_000);
        assert_eq!((tiers[1].1.lp, tiers[1].1.protocol, tiers[1].1.coin_creator), (10, 2, 2));
    }

    #[test]
    fn decode_fee_config_rejects_wrong_discriminator() {
        assert!(decode_fee_config(&[0u8; 149]).is_none());
    }
}
