/**
 * Bitcoin Core's "provably unspendable" rule for a scriptPubKey given as the
 * indexer's ASM text: the script starts with OP_RETURN (0x6a). Such outputs are
 * never UTXOs. Matches `classify_script_pubkey` in migration 007, which also
 * avoids undefined opcodes that rust-bitcoin prints as "OP_RETURN_187" etc.
 */
export function isProvablyUnspendable(scriptAsm: string): boolean {
  return scriptAsm === "OP_RETURN" || scriptAsm.startsWith("OP_RETURN ");
}
