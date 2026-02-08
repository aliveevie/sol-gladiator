use anchor_lang::prelude::*;
use anchor_lang::solana_program::keccak;

declare_id!("HXWhnY7mdrZoR4aSmd3nCk6ccp4bQc3XyPZSoBD1pG1Y");

#[program]
pub mod sol_arena {
    use super::*;

    // ═══════════════════════ INITIALIZE ═══════════════════════
    pub fn initialize(ctx: Context<Initialize>) -> Result<()> {
        let arena = &mut ctx.accounts.arena;
        arena.authority = ctx.accounts.authority.key();
        arena.total_matches = 0;
        arena.total_players = 0;
        arena.fee_rate_bps = 250; // 2.5%
        arena.fee_balance = 0;
        Ok(())
    }

    // ═══════════════════════ REGISTER PLAYER ═══════════════════════
    pub fn register_player(ctx: Context<RegisterPlayer>) -> Result<()> {
        let player = &mut ctx.accounts.player_stats;
        player.authority = ctx.accounts.authority.key();
        player.wins = 0;
        player.losses = 0;
        player.draws = 0;
        player.elo = 1200;
        player.total_wagered = 0;
        player.total_won = 0;
        player.matches_played = 0;

        let arena = &mut ctx.accounts.arena;
        arena.total_players += 1;
        Ok(())
    }

    // ═══════════════════════ RPS: CREATE MATCH ═══════════════════════
    pub fn create_rps_match(ctx: Context<CreateRpsMatch>, wager: u64) -> Result<()> {
        require!(wager > 0, ArenaError::ZeroWager);

        let match_account = &mut ctx.accounts.rps_match;
        match_account.player_a = ctx.accounts.player.key();
        match_account.player_b = Pubkey::default();
        match_account.wager = wager;
        match_account.score_a = 0;
        match_account.score_b = 0;
        match_account.current_round = 0;
        match_account.phase = MatchPhase::Open as u8;
        match_account.settled = false;
        match_account.created_at = Clock::get()?.unix_timestamp;

        // Transfer wager to escrow
        anchor_lang::system_program::transfer(
            CpiContext::new(
                ctx.accounts.system_program.to_account_info(),
                anchor_lang::system_program::Transfer {
                    from: ctx.accounts.player.to_account_info(),
                    to: ctx.accounts.escrow.to_account_info(),
                },
            ),
            wager,
        )?;

        Ok(())
    }

    // ═══════════════════════ RPS: JOIN MATCH ═══════════════════════
    pub fn join_rps_match(ctx: Context<JoinRpsMatch>) -> Result<()> {
        let match_account = &mut ctx.accounts.rps_match;
        require!(match_account.phase == MatchPhase::Open as u8, ArenaError::NotOpen);
        require!(ctx.accounts.player.key() != match_account.player_a, ArenaError::CantPlaySelf);

        match_account.player_b = ctx.accounts.player.key();
        match_account.phase = MatchPhase::Committing as u8;

        // Transfer wager to escrow
        anchor_lang::system_program::transfer(
            CpiContext::new(
                ctx.accounts.system_program.to_account_info(),
                anchor_lang::system_program::Transfer {
                    from: ctx.accounts.player.to_account_info(),
                    to: ctx.accounts.escrow.to_account_info(),
                },
            ),
            match_account.wager,
        )?;

        Ok(())
    }

    // ═══════════════════════ RPS: COMMIT CHOICE ═══════════════════════
    pub fn commit_choice(ctx: Context<CommitChoice>, commitment: [u8; 32]) -> Result<()> {
        let match_account = &mut ctx.accounts.rps_match;
        require!(!match_account.settled, ArenaError::AlreadySettled);

        let round = match_account.current_round as usize;
        require!(round < 3, ArenaError::InvalidRound);

        let is_a = ctx.accounts.player.key() == match_account.player_a;
        let is_b = ctx.accounts.player.key() == match_account.player_b;
        require!(is_a || is_b, ArenaError::NotPlayer);

        if is_a {
            require!(match_account.commits_a[round] == [0u8; 32], ArenaError::AlreadyCommitted);
            match_account.commits_a[round] = commitment;
        } else {
            require!(match_account.commits_b[round] == [0u8; 32], ArenaError::AlreadyCommitted);
            match_account.commits_b[round] = commitment;
        }

        Ok(())
    }

    // ═══════════════════════ RPS: REVEAL CHOICE ═══════════════════════
    pub fn reveal_choice(ctx: Context<RevealChoice>, choice: u8, salt: [u8; 32]) -> Result<()> {
        let match_account = &mut ctx.accounts.rps_match;
        require!(!match_account.settled, ArenaError::AlreadySettled);
        require!(choice >= 1 && choice <= 3, ArenaError::InvalidChoice); // 1=Rock, 2=Paper, 3=Scissors

        let round = match_account.current_round as usize;
        require!(round < 3, ArenaError::InvalidRound);

        // Verify commitment: keccak256(choice || salt)
        let mut data = Vec::with_capacity(33);
        data.push(choice);
        data.extend_from_slice(&salt);
        let hash = keccak::hash(&data);

        let is_a = ctx.accounts.player.key() == match_account.player_a;
        let is_b = ctx.accounts.player.key() == match_account.player_b;
        require!(is_a || is_b, ArenaError::NotPlayer);

        if is_a {
            require!(match_account.commits_a[round] == hash.to_bytes(), ArenaError::CommitmentMismatch);
            require!(match_account.choices_a[round] == 0, ArenaError::AlreadyRevealed);
            match_account.choices_a[round] = choice;
        } else {
            require!(match_account.commits_b[round] == hash.to_bytes(), ArenaError::CommitmentMismatch);
            require!(match_account.choices_b[round] == 0, ArenaError::AlreadyRevealed);
            match_account.choices_b[round] = choice;
        }

        // Both revealed? Resolve round
        if match_account.choices_a[round] != 0 && match_account.choices_b[round] != 0 {
            let a = match_account.choices_a[round];
            let b = match_account.choices_b[round];

            if a == b {
                // Draw — don't advance, replay round (reset commits)
                match_account.commits_a[round] = [0u8; 32];
                match_account.commits_b[round] = [0u8; 32];
                match_account.choices_a[round] = 0;
                match_account.choices_b[round] = 0;
            } else if (a == 1 && b == 3) || (a == 2 && b == 1) || (a == 3 && b == 2) {
                match_account.score_a += 1;
                match_account.current_round += 1;
            } else {
                match_account.score_b += 1;
                match_account.current_round += 1;
            }

            // Check best of 3
            if match_account.score_a >= 2 || match_account.score_b >= 2 {
                match_account.phase = MatchPhase::Finished as u8;
                match_account.settled = true;
                // Winner determined — payout handled by settle instruction
            }
        }

        Ok(())
    }

    // ═══════════════════════ RPS: SETTLE ═══════════════════════
    pub fn settle_rps(ctx: Context<SettleRps>) -> Result<()> {
        let match_account = &mut ctx.accounts.rps_match;
        require!(match_account.settled, ArenaError::NotSettled);

        let pot = match_account.wager * 2;
        let arena = &ctx.accounts.arena;
        let fee = pot * arena.fee_rate_bps as u64 / 10000;
        let payout = pot - fee;

        let winner;
        let loser;
        if match_account.score_a >= 2 {
            winner = match_account.player_a;
            loser = match_account.player_b;
        } else {
            winner = match_account.player_b;
            loser = match_account.player_a;
        }

        // Transfer from escrow to winner
        **ctx.accounts.escrow.to_account_info().try_borrow_mut_lamports()? -= payout;
        **ctx.accounts.winner.to_account_info().try_borrow_mut_lamports()? += payout;

        // Fee to arena
        **ctx.accounts.escrow.to_account_info().try_borrow_mut_lamports()? -= fee;
        **ctx.accounts.arena.to_account_info().try_borrow_mut_lamports()? += fee;

        // Update winner stats
        let winner_stats = &mut ctx.accounts.winner_stats;
        winner_stats.wins += 1;
        winner_stats.total_won += payout;
        winner_stats.total_wagered += match_account.wager;
        winner_stats.matches_played += 1;

        // Update loser stats
        let loser_stats = &mut ctx.accounts.loser_stats;
        loser_stats.losses += 1;
        loser_stats.total_wagered += match_account.wager;
        loser_stats.matches_played += 1;

        // ELO update (K=32, linear approx)
        let w_elo = winner_stats.elo as i64;
        let l_elo = loser_stats.elo as i64;
        let diff = (w_elo - l_elo).abs().min(400);
        let expected_w = if w_elo >= l_elo {
            500 + diff * 500 / 400
        } else {
            500 - diff * 500 / 400
        };
        let delta = (32 * (1000 - expected_w) / 1000).max(1);
        winner_stats.elo = (w_elo + delta).max(100) as u16;
        loser_stats.elo = (l_elo - delta).max(100) as u16;

        // Update arena
        let arena_mut = &mut ctx.accounts.arena;
        arena_mut.total_matches += 1;
        arena_mut.fee_balance += fee;

        Ok(())
    }

    // ═══════════════════════ COIN FLIP: CREATE ═══════════════════════
    pub fn create_coin_flip(ctx: Context<CreateCoinFlip>, wager: u64, commitment: [u8; 32]) -> Result<()> {
        require!(wager > 0, ArenaError::ZeroWager);

        let flip = &mut ctx.accounts.coin_flip;
        flip.player_a = ctx.accounts.player.key();
        flip.player_b = Pubkey::default();
        flip.wager = wager;
        flip.commit_a = commitment;
        flip.commit_b = [0u8; 32];
        flip.secret_a = [0u8; 32];
        flip.secret_b = [0u8; 32];
        flip.revealed_a = false;
        flip.revealed_b = false;
        flip.settled = false;
        flip.created_at = Clock::get()?.unix_timestamp;

        // Transfer wager
        anchor_lang::system_program::transfer(
            CpiContext::new(
                ctx.accounts.system_program.to_account_info(),
                anchor_lang::system_program::Transfer {
                    from: ctx.accounts.player.to_account_info(),
                    to: ctx.accounts.escrow.to_account_info(),
                },
            ),
            wager,
        )?;

        Ok(())
    }

    // ═══════════════════════ COIN FLIP: JOIN ═══════════════════════
    pub fn join_coin_flip(ctx: Context<JoinCoinFlip>, commitment: [u8; 32]) -> Result<()> {
        let flip = &mut ctx.accounts.coin_flip;
        require!(flip.player_b == Pubkey::default(), ArenaError::NotOpen);
        require!(ctx.accounts.player.key() != flip.player_a, ArenaError::CantPlaySelf);

        flip.player_b = ctx.accounts.player.key();
        flip.commit_b = commitment;

        anchor_lang::system_program::transfer(
            CpiContext::new(
                ctx.accounts.system_program.to_account_info(),
                anchor_lang::system_program::Transfer {
                    from: ctx.accounts.player.to_account_info(),
                    to: ctx.accounts.escrow.to_account_info(),
                },
            ),
            flip.wager,
        )?;

        Ok(())
    }

    // ═══════════════════════ COIN FLIP: REVEAL ═══════════════════════
    pub fn reveal_flip_secret(ctx: Context<RevealFlipSecret>, secret: [u8; 32]) -> Result<()> {
        let flip = &mut ctx.accounts.coin_flip;
        require!(!flip.settled, ArenaError::AlreadySettled);

        let hash = keccak::hash(&secret);

        let is_a = ctx.accounts.player.key() == flip.player_a;
        if is_a {
            require!(!flip.revealed_a, ArenaError::AlreadyRevealed);
            require!(flip.commit_a == hash.to_bytes(), ArenaError::CommitmentMismatch);
            flip.secret_a = secret;
            flip.revealed_a = true;
        } else {
            require!(!flip.revealed_b, ArenaError::AlreadyRevealed);
            require!(flip.commit_b == hash.to_bytes(), ArenaError::CommitmentMismatch);
            flip.secret_b = secret;
            flip.revealed_b = true;
        }

        // Both revealed → determine winner
        if flip.revealed_a && flip.revealed_b {
            let mut combined = Vec::with_capacity(64);
            combined.extend_from_slice(&flip.secret_a);
            combined.extend_from_slice(&flip.secret_b);
            let result = keccak::hash(&combined);
            // Even = heads (A wins), Odd = tails (B wins)
            flip.result_heads = result.to_bytes()[31] % 2 == 0;
            flip.settled = true;
        }

        Ok(())
    }
}

// ═══════════════════════ ACCOUNTS ═══════════════════════

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(init, payer = authority, space = 8 + Arena::SIZE, seeds = [b"arena"], bump)]
    pub arena: Account<'info, Arena>,
    #[account(mut)]
    pub authority: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct RegisterPlayer<'info> {
    #[account(init, payer = authority, space = 8 + PlayerStats::SIZE, seeds = [b"player", authority.key().as_ref()], bump)]
    pub player_stats: Account<'info, PlayerStats>,
    #[account(mut, seeds = [b"arena"], bump)]
    pub arena: Account<'info, Arena>,
    #[account(mut)]
    pub authority: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct CreateRpsMatch<'info> {
    #[account(init, payer = player, space = 8 + RpsMatch::SIZE)]
    pub rps_match: Account<'info, RpsMatch>,
    /// CHECK: Escrow PDA
    #[account(mut, seeds = [b"escrow", rps_match.key().as_ref()], bump)]
    pub escrow: UncheckedAccount<'info>,
    #[account(mut)]
    pub player: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct JoinRpsMatch<'info> {
    #[account(mut)]
    pub rps_match: Account<'info, RpsMatch>,
    /// CHECK: Escrow PDA
    #[account(mut, seeds = [b"escrow", rps_match.key().as_ref()], bump)]
    pub escrow: UncheckedAccount<'info>,
    #[account(mut)]
    pub player: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct CommitChoice<'info> {
    #[account(mut)]
    pub rps_match: Account<'info, RpsMatch>,
    pub player: Signer<'info>,
}

#[derive(Accounts)]
pub struct RevealChoice<'info> {
    #[account(mut)]
    pub rps_match: Account<'info, RpsMatch>,
    pub player: Signer<'info>,
}

#[derive(Accounts)]
pub struct SettleRps<'info> {
    #[account(mut)]
    pub rps_match: Account<'info, RpsMatch>,
    #[account(mut, seeds = [b"arena"], bump)]
    pub arena: Account<'info, Arena>,
    /// CHECK: Escrow PDA
    #[account(mut, seeds = [b"escrow", rps_match.key().as_ref()], bump)]
    pub escrow: UncheckedAccount<'info>,
    /// CHECK: Winner receives payout
    #[account(mut)]
    pub winner: UncheckedAccount<'info>,
    #[account(mut)]
    pub winner_stats: Account<'info, PlayerStats>,
    #[account(mut)]
    pub loser_stats: Account<'info, PlayerStats>,
}

#[derive(Accounts)]
pub struct CreateCoinFlip<'info> {
    #[account(init, payer = player, space = 8 + CoinFlip::SIZE)]
    pub coin_flip: Account<'info, CoinFlip>,
    /// CHECK: Escrow PDA
    #[account(mut, seeds = [b"escrow", coin_flip.key().as_ref()], bump)]
    pub escrow: UncheckedAccount<'info>,
    #[account(mut)]
    pub player: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct JoinCoinFlip<'info> {
    #[account(mut)]
    pub coin_flip: Account<'info, CoinFlip>,
    /// CHECK: Escrow PDA
    #[account(mut, seeds = [b"escrow", coin_flip.key().as_ref()], bump)]
    pub escrow: UncheckedAccount<'info>,
    #[account(mut)]
    pub player: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct RevealFlipSecret<'info> {
    #[account(mut)]
    pub coin_flip: Account<'info, CoinFlip>,
    pub player: Signer<'info>,
}

// ═══════════════════════ STATE ═══════════════════════

#[account]
pub struct Arena {
    pub authority: Pubkey,
    pub total_matches: u64,
    pub total_players: u64,
    pub fee_rate_bps: u16,
    pub fee_balance: u64,
}

impl Arena {
    pub const SIZE: usize = 32 + 8 + 8 + 2 + 8;
}

#[account]
pub struct PlayerStats {
    pub authority: Pubkey,
    pub wins: u32,
    pub losses: u32,
    pub draws: u32,
    pub elo: u16,
    pub total_wagered: u64,
    pub total_won: u64,
    pub matches_played: u32,
}

impl PlayerStats {
    pub const SIZE: usize = 32 + 4 + 4 + 4 + 2 + 8 + 8 + 4;
}

#[account]
pub struct RpsMatch {
    pub player_a: Pubkey,
    pub player_b: Pubkey,
    pub wager: u64,
    pub phase: u8,
    pub score_a: u8,
    pub score_b: u8,
    pub current_round: u8,
    pub settled: bool,
    pub created_at: i64,
    pub commits_a: [[u8; 32]; 3],
    pub commits_b: [[u8; 32]; 3],
    pub choices_a: [u8; 3],
    pub choices_b: [u8; 3],
}

impl RpsMatch {
    pub const SIZE: usize = 32 + 32 + 8 + 1 + 1 + 1 + 1 + 1 + 8 + (32 * 3) + (32 * 3) + 3 + 3;
}

#[account]
pub struct CoinFlip {
    pub player_a: Pubkey,
    pub player_b: Pubkey,
    pub wager: u64,
    pub commit_a: [u8; 32],
    pub commit_b: [u8; 32],
    pub secret_a: [u8; 32],
    pub secret_b: [u8; 32],
    pub revealed_a: bool,
    pub revealed_b: bool,
    pub result_heads: bool,
    pub settled: bool,
    pub created_at: i64,
}

impl CoinFlip {
    pub const SIZE: usize = 32 + 32 + 8 + 32 + 32 + 32 + 32 + 1 + 1 + 1 + 1 + 8;
}

// ═══════════════════════ ENUMS ═══════════════════════

#[derive(Clone, Copy, PartialEq)]
pub enum MatchPhase {
    Open = 0,
    Committing = 1,
    Revealing = 2,
    Finished = 3,
}

// ═══════════════════════ ERRORS ═══════════════════════

#[error_code]
pub enum ArenaError {
    #[msg("Wager must be greater than zero")]
    ZeroWager,
    #[msg("Match is not open")]
    NotOpen,
    #[msg("Cannot play against yourself")]
    CantPlaySelf,
    #[msg("Not a player in this match")]
    NotPlayer,
    #[msg("Already committed")]
    AlreadyCommitted,
    #[msg("Already revealed")]
    AlreadyRevealed,
    #[msg("Commitment mismatch")]
    CommitmentMismatch,
    #[msg("Invalid choice (must be 1-3)")]
    InvalidChoice,
    #[msg("Invalid round")]
    InvalidRound,
    #[msg("Match already settled")]
    AlreadySettled,
    #[msg("Match not settled yet")]
    NotSettled,
}
