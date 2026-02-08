import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { SolArena } from "../target/types/sol_arena";

describe("sol_arena", () => {
  anchor.setProvider(anchor.AnchorProvider.env());
  const program = anchor.workspace.SolArena as Program<SolArena>;

  it("Initializes the arena", async () => {
    // Test will be added after deployment
    console.log("Program ID:", program.programId.toString());
  });
});
