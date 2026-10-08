-- Script semantics for outputs.
--
-- script_type:          p2pkh, p2sh, p2wpkh, p2wsh, p2tr, op_return or
--                       unknown ("not a recognised template"; says nothing
--                       about spendability).
-- provably_unspendable: Bitcoin Core's CScript::IsUnspendable. The script
--                       starts with OP_RETURN or is longer than 10,000 bytes.
--                       Core never adds these outputs to its UTXO set.
--
-- New rows are classified by the indexer from the script bytes
-- (src/db/outputs.rs). Rows indexed before this migration only have the
-- script's text form, so they are backfilled with the equivalent text
-- classifier below. It cannot see the >10,000-byte rule; re-index if such
-- outputs matter.

CREATE OR REPLACE FUNCTION classify_script_pubkey(asm TEXT)
RETURNS TEXT
LANGUAGE SQL
IMMUTABLE
AS $$
    SELECT CASE
        -- Exactly opcode 0x6a. rust-bitcoin prints undefined opcodes
        -- 0xbb-0xfe as OP_RETURN_187 ... OP_RETURN_254, so a plain
        -- LIKE 'OP_RETURN%' would wrongly match them.
        WHEN asm = 'OP_RETURN' OR asm LIKE 'OP_RETURN %' THEN 'op_return'
        WHEN asm ~ '^OP_DUP OP_HASH160 OP_PUSHBYTES_20 [0-9a-f]{40} OP_EQUALVERIFY OP_CHECKSIG$' THEN 'p2pkh'
        WHEN asm ~ '^OP_HASH160 OP_PUSHBYTES_20 [0-9a-f]{40} OP_EQUAL$' THEN 'p2sh'
        WHEN asm ~ '^OP_0 OP_PUSHBYTES_20 [0-9a-f]{40}$' THEN 'p2wpkh'
        WHEN asm ~ '^OP_0 OP_PUSHBYTES_32 [0-9a-f]{64}$' THEN 'p2wsh'
        WHEN asm ~ '^OP_PUSHNUM_1 OP_PUSHBYTES_32 [0-9a-f]{64}$' THEN 'p2tr'
        ELSE 'unknown'
    END
$$;

ALTER TABLE outputs ADD COLUMN IF NOT EXISTS script_type TEXT;
ALTER TABLE outputs ADD COLUMN IF NOT EXISTS provably_unspendable BOOLEAN;

UPDATE outputs
SET
    script_type = classify_script_pubkey(script_pubkey),
    provably_unspendable = (classify_script_pubkey(script_pubkey) = 'op_return')
WHERE script_type IS NULL
   OR provably_unspendable IS NULL;

ALTER TABLE outputs ALTER COLUMN script_type SET NOT NULL;
ALTER TABLE outputs ALTER COLUMN provably_unspendable SET NOT NULL;

-- The UTXO set: unspent outputs that can ever be spent.
CREATE INDEX IF NOT EXISTS idx_outputs_utxo
ON outputs(id)
WHERE spent = FALSE AND provably_unspendable = FALSE;
