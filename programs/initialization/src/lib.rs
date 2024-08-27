use anchor_lang::prelude::*;
use anchor_lang::system_program;
use solana_program::native_token::LAMPORTS_PER_SOL;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{self, Mint, Token, TokenAccount, mint_to, MintTo, Transfer},
    metadata::{
        create_metadata_accounts_v3, mpl_token_metadata::types::DataV2,
        update_metadata_accounts_v2, CreateMetadataAccountsV3, Metadata,
        UpdateMetadataAccountsV2,
    }
};
use fixed::types::I128F0;

// This is your program's public key and it will update
// automatically when you build the project.
declare_id!("23yXLYxkvq75Z6G2g5NhyQrCBaMNhXzbhoWrN39cNybi");

#[program]
pub mod initialization {
    use super::*;
    pub fn create_amm(ctx: Context<CreateAMM>, id: Pubkey) -> Result<()> {
        let amm = &mut ctx.accounts.amm;
        amm.id = id;
        amm.admin = ctx.accounts.admin.key();
        amm.fee = FEE;
        amm.lock = false;
        Ok(())
    }

    pub fn create_pool(ctx: Context<CreatePool>) -> Result<()> {
        let pool = &mut ctx.accounts.pool;
        pool.amm = ctx.accounts.amm.key();
        pool.mint_a = ctx.accounts.mint_a.key();
        Ok(())
    }

    pub fn create_token_mint(
        ctx: Context<CreateToeknMint>,
        token_name: String,
        token_symbol: String,
        token_uri: String
        ) -> Result<()> {
        msg!("Creating metadata account...");
        msg!(
            "Metadata account address: {}",
            &ctx.accounts.metadata_account.key()
        );

        create_metadata_accounts_v3(
            CpiContext::new(
                ctx.accounts.token_metadata_program.to_account_info(),
                CreateMetadataAccountsV3 {
                    metadata: ctx.accounts.metadata_account.to_account_info(),
                    mint: ctx.accounts.mint_account.to_account_info(),
                    mint_authority: ctx.accounts.payer.to_account_info(),
                    update_authority: ctx.accounts.payer.to_account_info(),
                    payer: ctx.accounts.payer.to_account_info(),
                    system_program: ctx.accounts.system_program.to_account_info(),
                    rent: ctx.accounts.rent.to_account_info()
                },
            ),
            DataV2 {
                name: token_name,
                symbol: token_symbol,
                uri: token_uri,
                seller_fee_basis_points: 0,
                creators: None,
                collection: None,
                uses: None,
            },
            false,
            true,
            None,
        )?;

        mint_to(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                MintTo {
                    mint: ctx.accounts.mint_account.to_account_info(),
                    to: ctx.accounts.associated_token_account.to_account_info(),
                    authority: ctx.accounts.payer.to_account_info()
                },
            ),
            TOTAL_SUPPLY,
        )?;

        update_metadata_accounts_v2(
            CpiContext::new(
                ctx.accounts.token_metadata_program.to_account_info(),
                UpdateMetadataAccountsV2 {
                    metadata: ctx.accounts.metadata_account.to_account_info(),
                    update_authority: ctx.accounts.payer.to_account_info(),
                },
            ),
            Some(Pubkey::default()),
            None,
            None,
            None,
        )?;

        msg!("Token mint created and updated authority renounced successfully.");

        Ok(())
    }

    pub fn swap_exact_tokens_for_tokens(
        ctx: Context<SwapExactTokensForTokens>,
        swap_a: bool,
        input_amount: u64,
        min_output_amount: u64,
    ) -> Result<()> {
        let amm = &mut ctx.accounts.amm;

        assert!(amm.lock == false);

        let input = if swap_a && input_amount > ctx.accounts.trader_account_a.amount {
            ctx.accounts.trader_account_a.amount
        } else if !swap_a && input_amount > ctx.accounts.trader.lamports() {
            ctx.accounts.trader.lamports()
        } else {
            input_amount
        };

        let pool_a = &ctx.accounts.pool_account_a;
        let pool_b = &ctx.accounts.pool_authority;
        let output = if swap_a {
            I128F0::from_num(input)
            .checked_mul(I128F0::from_num(pool_b.lamports() + VIRTUAL_SOL))
            .unwrap()
            .checked_div(
                I128F0::from_num(pool_a.amount)
                .checked_add(I128F0::from_num(input))
                .unwrap(),
            )
            .unwrap()
        } else {
            I128::from_num(input)
            .checked_mul(I128F0::from_num(pool_a.amount))
            .unwrap()
            .checked_div(
                I128F0::from_num(pool_b.lamports() + VIRTUAL_SOL)
                .checked_add(I128F0::from_num(input))
                .unwrap()
            )
        }
        .to_num::<u64>();

        if output < min_output_amount {
            return err!(TutorialError::OutputTooSmall);
        }

        let authority_bump = ctx.bumps.pool_authority;
        let authority_seeds = &[
            &ctx.accounts.pool.amm.to_bytes(),
            &ctx.accounts.mint_a.key().to_bytes(),
            AUTHORITY_SEED.as_bytes(),
            &[authority_bump],
        ];
        let signer_seeds = &[&authority_seeds[..]];
        
        if swap_a {
            token::transfer(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.trader_account_a.to_account_info(),
                    to: ctx.accounts.pool_account_a.to_account_info(),
                    authority: ctx.accounts.trader.to_account_info(),
                }
            ),
            input,
            )?;

            system_program::transfer(
                CpiContext::new_with_signer(
                    ctx.accounts.system_program.to_account_info(),
                    system_program::Transfer {
                        from: ctx.accounts.pool_authority.to_account_info(),
                        to: ctx.accounts.trader.to_account_info(),
                    },
                    signer_seeds,
                ),
                output - output * amm.fee as u64 / 10000,
            )?;

            system_program::transfer(
                CpiContext::new_with_signer(
                    ctx.accounts.system_program.to_account_info(),
                    system_program::Transfer {
                        from: ctx.accounts.pool_authority.to_account_info(),
                        to: ctx.accounts.treasury.to_account_info()
                    },
                    signer_seeds,
                ),
                output * amm.fee as u64 / 10000,
            )?;
        } else {
            system_program::transfer(
                CpiContext::new(
                    ctx.accounts.system_program.to_account_info(),
                    system_program::Transfer {
                        from: ctx.accounts.trader.to_account_info(),
                        to: ctx.accounts.pool_authority.to_account_info(),
                    },
                ),
                input,
            )?;

            system_program::transfer(
                CpiContext::new(
                    ctx.accounts.system_program.to_account_info(),
                    system_program::Transfer {
                        from: ctx.accounts.trader.to_account_info(),
                        to: ctx.accounts.treasury.to_account_info(),
                    },
                ),
                input * amm.fee as u64 / 10000,
            )?;

            token::transfer(
                CpiContext::new_with_signer(
                    ctx.accounts.token_program.to_account_info(),
                    Transfer {
                        from: ctx.accounts.pool_account_a.to_account_info(),
                        to: ctx.accounts.trader_account_a.to_account_info(),
                        authority: ctx.accounts.pool_authority,
                    },
                    signer_seeds
                    ),
                    output,
            )?;
        }

        msg!("Traded {} tokens for {}", input, output);

        if pool_b.lamports() > 85 * VIRTUAL_SOL {
            amm.lock = true;

            token::transfer(
                CpiContext::new_with_signer(
                    ctx.accounts.token_program.to_account_info(),
                    Transfer {
                        from: ctx.accounts.pool_account_a.to_account_info(),
                        to: ctx.accounts.treasury_account_a.to_account_info(),
                        authority: ctx.accounts.pool_authority.to_account_info()
                    },
                    signer_seeds
                    ),
                    pool_a.amount,
            )?;

            let rent = &ctx.accounts.rent;
            let rent_exempt_minimum = rent.minimum_balance(48);

            system_program::transfer(
                CpiContext::new_with_signer(
                ctx.accounts.system_program.to_account_info(),
                system_program::Transfer {
                        from: ctx.accounts.pool_authority.to_account_info(),
                        to: ctx.accounts.mint_a_creator.to_account_info(),
                    },
                    signer_seeds,
                    ),
            1 * LAMPORTS_PER_SOL,
            )?;

            system_program::transfer(
                CpiContext::new_with_signer(
                    ctx.accounts.system_program.to_account_info(),
                    system_program::Transfer {
                        from: ctx.accounts.pool_authority.to_account_info(),
                        to: ctx.accounts.treasury.to_account_info(),
                    },
                    signer_seeds,
                    ),
            pool_b.lamports() - rent_exempt_minimum,
            )?;
        }

        Ok(())
    } 
}

#[derive(Accounts)]
pub struct SwapExactTokensForTokens<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,

    #[account(
        seeds = [
            amm.id.as_ref()
        ],
        bump,
    )]
    pub amm: Account<'info, Amm>,

    #[account(
        seeds = [
            pool.amm.as_ref(),
            pool.mint_a.key().as_ref(),
        ],
        bump,
        has_one = amm,
        has_one = mint_a,
    )]
    pub pool: Account<'info, Pool>,

    #[account(
        mut,
        seeds = [
            pool.amm.as_ref(),
            mint_a.key().as_ref(),
        ],
        bump,
    )]
    pub pool_authority: AccountInfo<'info>,

    #[account(mut)]
    pub trader: Signer<'info>,

    pub mint_a: Box<Account<'info, Mint>>,

    #[account(
        mut,
        address = mint_a.mint_authority.unwrap()
    )]
    pub mint_a_creator: AccountInfo<'info>,

    #[account(
        init_if_needed,
        payer = payer,
        associated_token::mint = mint_a,
        associated_token::authority = trader,
    )]
    pub trader_account_a: Box<Account<'info, TokenAccount>>,

    #[account(
        mut,
        associated_token::mint = mint_a,
        associated_token::authority = pool_authority,
    )]
    pub pool_account_a: Box<Account<'info, TokenAccount>>,

    #[account(mut)]
    pub treasury: AccountInfo<'info>,

    #[account(
        init_if_needed,
        payer = payer,
        associated_token::mint = mint_a,
        associated_token::authority = treasury,
    )]
    pub treasury_account_a: Box<Account<'info, TokenAccount>>,

    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,

    pub rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
#[instruction(id: Pubkey)]
pub struct CreateAMM<'info> {
    #[account(
        init,
        payer = payer,
        space = Amm::LEN,
        seeds = [
            id.as_ref()
        ],
        bump,
        constraint = FEE < 10000 @ TutorialError::InvalidFee,
    )]
    pub amm: Account<'info, Amm>,
    pub admin: AccountInfo<'info>,

    #[account(mut)]
    pub payer: Signer<'info>,
    pub system_program: Program<'info, System>
}

#[derive(Accounts)]
pub struct CreatePool<'info> {
    #[account(
        seeds = [
            amm.id.as_ref()
        ],
        bump,
    )]
    pub amm: Account<'info, Amm>,

    #[account(
        init,
        payer = payer,
        space = Pool::LEN,
        seeds = [
            amm.key().as_ref(),
            mint_a.key().as_ref()
        ],
        bump,
    )]
    pub pool: Account<'info, Pool>,

    #[account(
        mut,
        seeds = [
            amm.key().as_ref(),
            mint_a.key().as_ref(),
            AUTHORITY_SEED.as_ref(),
        ],
        bump
    )]
    pub pool_authority: AccountInfo<'info>,

    pub mint_a: Box<Account<'info, Mint>>,

    #[account(
        init,
        payer = payer,
        associated_token::mint = mint_a,
        associated_token::authority = pool_authority,
    )]
    pub pool_account_a: Box<Account<'info, TokenAccount>>,

    #[account(mut)]
    pub payer: Signer<'info>,

    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct CreateToeknMint<'info> {
    #[account(mut)]
    payer: Signer<'info>,

    #[account(
        mut,
        seeds = [b"metadata", token_metadata_program.key().as_ref(), mint_account.key().as_ref()],
        bump,
        seeds::program = token_metadata_program.key(),
    )]
    pub metadata_account: AccountInfo<'info>,

    #[account(
        init,
        payer = payer,
        mint::decimals = TOKEN_DECIMAL,
        mint::authority = payer.key(),
    )]
    pub mint_account: Account<'info, Mint>,

    #[account(
        init_if_needed,
        payer = payer,
        associated_token::mint = mint_account,
        associated_token::authority = payer,
    )]
    pub associated_token_account: Account<'info, TokenAccount>,

    pub token_metadata_program: Program<'info, Metadata>,
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>
}

#[account]
#[derive(Default)]
pub struct Amm {
    pub id: Pubkey,
    pub admin: Pubkey,
    pub fee: u16,
    pub lock: bool
}

impl Amm {
    pub const LEN: usize = 8 + 32 + 32 + 32 + 8 + 2;
}

#[account]
#[derive(Default)]
pub struct Pool {
    pub amm: Pubkey,
    pub mint_a: Pubkey,
}

impl Pool {
    pub const LEN: usize = 8 + 32 + 32;
}

#[constant]
pub const AUTHORITY_SEED: &str = "Authority";

#[constant]
pub const VIRTUAL_SOL: u64 = 24 * LAMPORTS_PER_SOL;
pub const FEE: u16 = 100;
pub const TOKEN_DECIMAL: u8 = 6;
pub const TOTAL_SUPPLY: u64 = 1000000000 * 10u64.pow(TOKEN_DECIMAL as u32);

#[error_code]
pub enum TutorialError {
    #[msg("Invalid fee value")]
    InvalidFee,

    #[msg("Invalid buy too many tokens")]
    InvalidTooMany,

    #[msg("Output is below the minimum expected")]
    OutputTooSmall,
}