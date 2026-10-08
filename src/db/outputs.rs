use anyhow::Result;
use bitcoin::{Address, Network, Script, Transaction as BitcoinTransaction};
use sqlx::{Postgres, Transaction};

/// Scripts longer than this can never be executed (`MAX_SCRIPT_SIZE`).
const MAX_SCRIPT_SIZE: usize = 10_000;

/// The standard output template a scriptPubKey matches.
///
/// "unknown" covers everything else (P2PK, bare multisig, anchors, future
/// witness versions, nonstandard scripts). It says nothing about whether
/// the output can be spent. `classify_script_pubkey` in migration 007
/// mirrors this for rows indexed before the column existed.
pub fn script_type(script_pubkey: &Script) -> &'static str {
    if script_pubkey.is_op_return() {
        "op_return"
    } else if script_pubkey.is_p2pkh() {
        "p2pkh"
    } else if script_pubkey.is_p2sh() {
        "p2sh"
    } else if script_pubkey.is_p2wpkh() {
        "p2wpkh"
    } else if script_pubkey.is_p2wsh() {
        "p2wsh"
    } else if script_pubkey.is_p2tr() {
        "p2tr"
    } else {
        "unknown"
    }
}

/// Bitcoin Core's `CScript::IsUnspendable`: the output can never be spent,
/// so Core never adds it to its UTXO set.
///
/// Deliberately narrower than rust-bitcoin's `is_provably_unspendable`,
/// which also flags scripts starting with disabled or invalid opcodes;
/// Core keeps those in its UTXO set, and Core is the source of truth.
pub fn is_provably_unspendable(script_pubkey: &Script) -> bool {
    script_pubkey.is_op_return() || script_pubkey.len() > MAX_SCRIPT_SIZE
}

/// Converts a scriptPubKey into a standard Bitcoin regtest address.
///
/// Some outputs, such as OP_RETURN outputs, do not correspond to an
/// address. In that case we store NULL.
pub fn output_address(script_pubkey: &Script) -> Option<String> {
    Address::from_script(script_pubkey, Network::Regtest)
        .ok()
        .map(|address| address.to_string())
}

pub async fn save_outputs(
    db: &mut Transaction<'_, Postgres>,
    transaction_id: i64,
    tx: &BitcoinTransaction,
) -> Result<()> {
    for (vout, output) in tx.output.iter().enumerate() {
        let address = output_address(&output.script_pubkey);

        sqlx::query(
            r#"
            INSERT INTO outputs (
                transaction_id,
                vout,
                value,
                script_pubkey,
                address,
                script_type,
                provably_unspendable
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7)

            ON CONFLICT (transaction_id, vout)
            DO UPDATE SET
                value = EXCLUDED.value,
                script_pubkey = EXCLUDED.script_pubkey,
                address = EXCLUDED.address,
                script_type = EXCLUDED.script_type,
                provably_unspendable = EXCLUDED.provably_unspendable
            "#,
        )
        .bind(transaction_id)
        .bind(vout as i32)
        .bind(output.value.to_sat() as i64)
        .bind(output.script_pubkey.to_string())
        .bind(address)
        .bind(script_type(&output.script_pubkey))
        .bind(is_provably_unspendable(&output.script_pubkey))
        .execute(&mut **db)
        .await?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use bitcoin::ScriptBuf;
    use bitcoin::hashes::Hash;

    use super::*;

    fn script(bytes: Vec<u8>) -> ScriptBuf {
        ScriptBuf::from_bytes(bytes)
    }

    #[test]
    fn standard_templates_are_recognised() {
        let cases = [
            (
                ScriptBuf::new_p2pkh(&bitcoin::PubkeyHash::from_byte_array([1; 20])),
                "p2pkh",
            ),
            (
                ScriptBuf::new_p2sh(&bitcoin::ScriptHash::from_byte_array([2; 20])),
                "p2sh",
            ),
            (
                ScriptBuf::new_p2wpkh(&bitcoin::WPubkeyHash::from_byte_array([3; 20])),
                "p2wpkh",
            ),
            (
                ScriptBuf::new_p2wsh(&bitcoin::WScriptHash::from_byte_array([4; 32])),
                "p2wsh",
            ),
            (script([vec![0x51, 0x20], vec![5; 32]].concat()), "p2tr"),
            (script(vec![0x6a, 0x02, 0xbe, 0xef]), "op_return"),
            (script(vec![0x6a]), "op_return"),
        ];

        for (script, expected) in cases {
            assert_eq!(script_type(&script), expected, "{script}");
        }
    }

    #[test]
    fn everything_else_is_unknown_not_unspendable() {
        let unknown = [
            // P2PK, pay-to-anchor, a future witness version, a v0 program of
            // a non-standard length, OP_TRUE, and the empty script.
            script([vec![0x21], vec![2; 33], vec![0xac]].concat()),
            script(vec![0x51, 0x02, 0x4e, 0x73]),
            script([vec![0x52, 0x20], vec![6; 32]].concat()),
            script([vec![0x00, 0x15], vec![7; 21]].concat()),
            script(vec![0x51]),
            script(vec![]),
        ];

        for script in unknown {
            assert_eq!(script_type(&script), "unknown", "{script}");
            assert!(!is_provably_unspendable(&script), "{script}");
        }
    }

    #[test]
    fn provably_unspendable_follows_bitcoin_core() {
        assert!(is_provably_unspendable(&script(vec![0x6a, 0x01, 0x00])));
        assert!(is_provably_unspendable(&script(vec![0x51; 10_001])));
        assert!(!is_provably_unspendable(&script(vec![0x51; 10_000])));

        // Core keeps these in its UTXO set although they can never be spent;
        // rust-bitcoin's broader (and deprecated) helper would drop them.
        for bytes in [vec![0x7e], vec![0xbb, 0x51]] {
            let s = script(bytes);
            #[allow(deprecated)]
            let rust_bitcoin_says = s.is_provably_unspendable();
            assert!(rust_bitcoin_says, "rust-bitcoin: {s}");
            assert!(!is_provably_unspendable(&s), "Core: {s}");
        }
    }

    #[test]
    fn undefined_opcodes_are_not_op_return_despite_their_names() {
        // rust-bitcoin prints 0xbb as OP_RETURN_187; it is not OP_RETURN.
        let s = script(vec![0xbb, 0x51]);
        assert!(s.to_string().starts_with("OP_RETURN_187"));
        assert_eq!(script_type(&s), "unknown");
    }
}
