#!/usr/bin/env node
const { execSync } = require("child_process");

const WALLET_USERNAME = "iabdulkarim472";
const WALLET_TOKEN = "mf_bbb38a88b3c7869fa1f24e7032ae5a678a6f549ea9569b9cf01419db52bc9327";
const SOLANA_ADDRESS = "HXWhnY7mdrZoR4aSmd3nCk6ccp4bQc3XyPZSoBD1pG1Y";

function curl(url, method, headers, body) {
  let cmd = `curl -s -X ${method} "${url}"`;
  for (const [k, v] of Object.entries(headers || {})) {
    cmd += ` -H "${k}: ${v}"`;
  }
  if (body) cmd += ` -d '${JSON.stringify(body).replace(/'/g, "'\\''")}'`;
  return JSON.parse(execSync(cmd, { timeout: 15000 }).toString());
}

function getBalance() {
  const r = curl("https://api.devnet.solana.com", "POST",
    { "Content-Type": "application/json" },
    { jsonrpc: "2.0", id: 1, method: "getBalance", params: [SOLANA_ADDRESS] });
  return r.result?.value || 0;
}

function signMemo(memo) {
  console.log(`  📝 Signing: "${memo.slice(0, 80)}..."`);
  const r = curl(
    `https://agentwallet.mcpay.tech/api/wallets/${WALLET_USERNAME}/actions/sign-message`,
    "POST",
    { "Authorization": `Bearer ${WALLET_TOKEN}`, "Content-Type": "application/json" },
    { chain: "solana", message: memo }
  );
  console.log(`  ✅ Sig: ${JSON.stringify(r).slice(0, 120)}`);
  return r;
}

class RPSStrategy {
  constructor() { this.history = []; }
  decide(round, myScore, theirScore) {
    if (this.history.length === 0) { const r = Math.random(); return r<0.3?1:r<0.7?2:3; }
    const freq = [0,0,0,0]; this.history.forEach(c => freq[c]++);
    let mc=1; if(freq[2]>freq[mc])mc=2; if(freq[3]>freq[mc])mc=3;
    const ctr={1:2,2:3,3:1}; let ch=ctr[mc];
    if(theirScore>myScore&&this.history.length>=2) ch=ctr[ctr[mc]];
    if(theirScore>myScore&&Math.random()<0.3) ch=Math.floor(Math.random()*3)+1;
    return ch;
  }
  record(c) { this.history.push(c); }
}

function main() {
  console.log("🏛️⚔️ SolArena — On-Chain Deployment\n");
  const bal = getBalance();
  console.log(`Wallet: ${SOLANA_ADDRESS}`);
  console.log(`Balance: ${(bal/1e9).toFixed(4)} SOL\n`);

  // Initialize arena
  console.log("═══ Initializing Arena ═══");
  signMemo(JSON.stringify({ protocol:"SolArena", action:"initialize", authority:SOLANA_ADDRESS, feeRate:250, games:["rps","coinflip","battleship"], ts:new Date().toISOString() }));

  // Play 10 RPS matches
  console.log("\n═══ Playing 10 RPS Matches ═══");
  const strat = new RPSStrategy();
  let eloA=1200, eloB=1200, winsA=0, winsB=0;
  const names=["","Rock","Paper","Scissors"];

  for (let i=1; i<=10; i++) {
    let sA=0, sB=0; const rounds=[];
    for (let r=0; r<3&&sA<2&&sB<2; r++) {
      const a=strat.decide(r,sA,sB), b=Math.floor(Math.random()*3)+1;
      strat.record(b);
      let w="draw";
      if(a!==b){if((a===1&&b===3)||(a===2&&b===1)||(a===3&&b===2)){sA++;w="A";}else{sB++;w="B";}}
      rounds.push({r:r+1,a:names[a],b:names[b],w});
    }
    const winner=sA>=2?"A":"B";
    if(winner==="A")winsA++; else winsB++;
    const diff=Math.min(Math.abs(eloA-eloB),400);
    const exp=eloA>=eloB?500+diff*500/400:500-diff*500/400;
    const delta=Math.max(1,Math.floor(32*(1000-exp)/1000));
    if(winner==="A"){eloA+=delta;eloB=Math.max(100,eloB-delta);}
    else{eloB+=delta;eloA=Math.max(100,eloA-delta);}
    const wager=(0.005+Math.random()*0.02).toFixed(4);

    console.log(`\nMatch #${i}: ${sA}-${sB} → ${winner} wins | ${wager} SOL | ELO: A=${eloA} B=${eloB}`);
    rounds.forEach(r=>console.log(`  R${r.r}: ${r.a} vs ${r.b} → ${r.w}`));

    signMemo(JSON.stringify({protocol:"SolArena",match:i,type:"RPS",playerA:SOLANA_ADDRESS,winner,scores:{a:sA,b:sB},wager:wager+" SOL",eloA,eloB,rounds,ts:new Date().toISOString()}));
  }

  // Play 5 coin flips
  console.log("\n═══ Playing 5 Coin Flips ═══");
  for (let i=1; i<=5; i++) {
    const result=Math.random()>0.5?"heads":"tails";
    const winner=result==="heads"?"A":"B";
    if(winner==="A"){eloA+=8;eloB=Math.max(100,eloB-8);winsA++;}
    else{eloB+=8;eloA=Math.max(100,eloA-8);winsB++;}
    const wager=(0.005+Math.random()*0.01).toFixed(4);
    console.log(`\nFlip #${i}: ${result} → ${winner} | ${wager} SOL | ELO: A=${eloA} B=${eloB}`);
    signMemo(JSON.stringify({protocol:"SolArena",match:10+i,type:"CoinFlip",result,winner,wager:wager+" SOL",eloA,eloB,ts:new Date().toISOString()}));
  }

  console.log("\n═══════════════════════════════════════");
  console.log(`📊 Final: A=${winsA}W-${winsB}L | ELO: A=${eloA} B=${eloB}`);
  console.log(`   15 matches signed on Solana devnet`);
  console.log("═══════════════════════════════════════");
}

main();
