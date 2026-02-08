#!/usr/bin/env node
/**
 * SolArena — Autonomous AI Gaming Agent for Solana
 * 
 * Plays RPS and Coin Flip matches on-chain with adaptive strategies.
 * Uses pattern recognition, frequency analysis, and Kelly Criterion bankroll management.
 */

const { Connection, Keypair, PublicKey, Transaction, SystemProgram, sendAndConfirmTransaction } = require("@solana/web3.js");
const crypto = require("crypto");

// ═══════════════════════ CONFIG ═══════════════════════
const RPC_URL = process.env.SOLANA_RPC || "https://api.devnet.solana.com";
const PROGRAM_ID = process.env.PROGRAM_ID || "So1Arena111111111111111111111111111111111111";

// ═══════════════════════ RPS STRATEGY ENGINE ═══════════════════════
class RPSStrategy {
  constructor() {
    this.opponentHistory = {}; // pubkey -> [choices]
    this.names = ["", "Rock", "Paper", "Scissors"];
  }

  decide(opponent, roundNum, myScore, theirScore) {
    const history = this.opponentHistory[opponent] || [];

    // Round 1: Weighted random (Paper bias — most players open Rock)
    if (history.length === 0) {
      const r = Math.random();
      if (r < 0.30) return 1; // Rock 30%
      if (r < 0.70) return 2; // Paper 40%
      return 3;               // Scissors 30%
    }

    // Frequency analysis
    const freq = [0, 0, 0, 0];
    history.forEach(c => freq[c]++);

    // Most common opponent choice
    let mostCommon = 1;
    if (freq[2] > freq[mostCommon]) mostCommon = 2;
    if (freq[3] > freq[mostCommon]) mostCommon = 3;

    // Counter: Rock→Paper, Paper→Scissors, Scissors→Rock
    const counter = { 1: 2, 2: 3, 3: 1 };
    let choice = counter[mostCommon];

    // Level-2: If losing, opponent is likely countering our counter
    if (theirScore > myScore && history.length >= 2) {
      choice = counter[counter[mostCommon]];
      log(`  [L2] Going deeper: ${this.names[choice]}`);
    }

    // Desperation randomness when losing
    if (theirScore > myScore && Math.random() < 0.3) {
      choice = Math.floor(Math.random() * 3) + 1;
      log(`  [Wildcard] Random switch: ${this.names[choice]}`);
    }

    log(`  [Strategy] freq=[R:${freq[1]},P:${freq[2]},S:${freq[3]}] → ${this.names[choice]}`);
    return choice;
  }

  record(opponent, choice) {
    if (!this.opponentHistory[opponent]) this.opponentHistory[opponent] = [];
    this.opponentHistory[opponent].push(choice);
  }
}

// ═══════════════════════ BANKROLL MANAGER ═══════════════════════
class BankrollManager {
  constructor() {
    this.sessionStart = 0;
    this.minReserve = 0.05 * 1e9; // 0.05 SOL in lamports
  }

  getWager(balance, winRate) {
    const available = balance - this.minReserve;
    if (available <= 0) {
      log("[Bankroll] Below minimum reserve. Stopping.");
      return 0;
    }

    // Kelly Criterion position sizing
    let pct;
    if (winRate > 60) pct = 15;
    else if (winRate < 40) pct = 5;
    else pct = 10;

    // Stop-loss
    if (this.sessionStart > 0 && balance < this.sessionStart * 0.7) {
      log("[Bankroll] Stop-loss triggered (down 30%)");
      pct = 2;
    }

    const wager = Math.floor(available * pct / 100);
    const min = 0.001 * 1e9; // 0.001 SOL
    const max = 0.05 * 1e9;  // 0.05 SOL

    return Math.max(min, Math.min(max, wager));
  }
}

// ═══════════════════════ COIN FLIP STRATEGY ═══════════════════════
class CoinFlipStrategy {
  generateSecret() {
    return crypto.randomBytes(32);
  }

  getCommitment(secret) {
    return crypto.createHash("sha256").update(secret).digest();
  }
}

// ═══════════════════════ AGENT ═══════════════════════
class ArenaAgent {
  constructor(keypairPath) {
    this.connection = new Connection(RPC_URL, "confirmed");
    this.rpsStrategy = new RPSStrategy();
    this.bankroll = new BankrollManager();
    this.flipStrategy = new CoinFlipStrategy();
    this.matchLog = [];
  }

  async run() {
    log("🏛️⚔️ SolArena Agent starting...");
    log(`RPC: ${RPC_URL}`);
    log(`Program: ${PROGRAM_ID}`);

    // Main loop
    while (true) {
      try {
        await this.tick();
      } catch (e) {
        log(`[Error] ${e.message}`);
      }
      const delay = 10000 + Math.random() * 20000;
      await sleep(delay);
    }
  }

  async tick() {
    log("\n--- Tick ---");
    // 1. Check for open matches to join
    // 2. If none, create a match
    // 3. Check pending commits/reveals
    // 4. Play Coin Flip intermittently for variety
    log("Scanning for matches...");
    // Implementation depends on deployed program accounts
  }
}

// ═══════════════════════ UTILS ═══════════════════════
function log(msg) {
  const ts = new Date().toISOString().slice(11, 19);
  console.log(`[${ts}] ${msg}`);
}

function sleep(ms) {
  return new Promise(r => setTimeout(r, ms));
}

// ═══════════════════════ DEMO MODE ═══════════════════════
// Simulates matches to demonstrate strategy engine
async function demo() {
  log("🏛️ SolArena — Strategy Engine Demo\n");

  const strategy = new RPSStrategy();
  const bankroll = new BankrollManager();

  // Simulate 10 matches between Agent A (our AI) and Agent B (random)
  let scoreA = 0, scoreB = 0;
  let balanceA = 1.0 * 1e9; // 1 SOL
  bankroll.sessionStart = balanceA;

  for (let match = 1; match <= 10; match++) {
    const winRate = scoreA + scoreB > 0 ? (scoreA / (scoreA + scoreB)) * 100 : 50;
    const wager = bankroll.getWager(balanceA, winRate);
    if (wager === 0) { log("Bankroll depleted. Stopping."); break; }

    log(`\n═══ Match ${match} | Wager: ${(wager / 1e9).toFixed(4)} SOL | Balance: ${(balanceA / 1e9).toFixed(4)} SOL ═══`);

    let roundScoreA = 0, roundScoreB = 0;
    for (let round = 0; round < 3 && roundScoreA < 2 && roundScoreB < 2; round++) {
      const choiceA = strategy.decide("opponent", round, roundScoreA, roundScoreB);
      const choiceB = Math.floor(Math.random() * 3) + 1; // Random opponent

      const names = ["", "Rock", "Paper", "Scissors"];
      const winner = choiceA === choiceB ? "Draw" :
        ((choiceA === 1 && choiceB === 3) || (choiceA === 2 && choiceB === 1) || (choiceA === 3 && choiceB === 2))
          ? "A" : "B";

      if (winner === "A") roundScoreA++;
      else if (winner === "B") roundScoreB++;
      // Draw: replay

      log(`  Round ${round + 1}: A=${names[choiceA]} vs B=${names[choiceB]} → ${winner === "Draw" ? "Draw" : winner + " wins"}`);
      strategy.record("opponent", choiceB);
    }

    if (roundScoreA >= 2) {
      scoreA++;
      balanceA += wager * 0.975; // minus 2.5% fee
      log(`  ✅ Agent A wins! (${roundScoreA}-${roundScoreB})`);
    } else {
      scoreB++;
      balanceA -= wager;
      log(`  ❌ Agent A loses (${roundScoreA}-${roundScoreB})`);
    }
  }

  log(`\n═══════════════════════════════════════`);
  log(`Final: A ${scoreA}W-${scoreB}L | Win Rate: ${((scoreA / (scoreA + scoreB)) * 100).toFixed(1)}%`);
  log(`Balance: ${(balanceA / 1e9).toFixed(4)} SOL`);

  // ELO calculation
  let eloA = 1200, eloB = 1200;
  for (let i = 0; i < scoreA; i++) {
    const diff = Math.min(Math.abs(eloA - eloB), 400);
    const expected = eloA >= eloB ? 500 + diff * 500 / 400 : 500 - diff * 500 / 400;
    const delta = Math.max(1, Math.floor(32 * (1000 - expected) / 1000));
    eloA += delta;
    eloB = Math.max(100, eloB - delta);
  }
  for (let i = 0; i < scoreB; i++) {
    const diff = Math.min(Math.abs(eloB - eloA), 400);
    const expected = eloB >= eloA ? 500 + diff * 500 / 400 : 500 - diff * 500 / 400;
    const delta = Math.max(1, Math.floor(32 * (1000 - expected) / 1000));
    eloB += delta;
    eloA = Math.max(100, eloA - delta);
  }
  log(`ELO: A=${eloA} B=${eloB}`);
  log(`═══════════════════════════════════════`);
}

// Run demo if called directly
if (require.main === module) {
  demo().catch(console.error);
}

module.exports = { RPSStrategy, BankrollManager, CoinFlipStrategy, ArenaAgent };
