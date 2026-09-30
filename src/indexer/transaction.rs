use bitcoin::Block;

pub struct TransactionProcessor;

impl TransactionProcessor {
    pub fn print_transactions(block: &Block) {
        println!();
        println!("=== Transactions ===");

        for (tx_index, tx) in block.txdata.iter().enumerate() {
            println!();
            println!("Transaction {}", tx_index);
            println!("TXID: {}", tx.compute_txid());

            println!("Inputs: {}", tx.input.len());

            for (input_index, input) in tx.input.iter().enumerate() {
                println!("  Input {}", input_index);
                println!("  Previous TXID: {}", input.previous_output.txid);
                println!("  Previous VOUT: {}", input.previous_output.vout);
            }

            println!("Outputs: {}", tx.output.len());

            for (output_index, output) in tx.output.iter().enumerate() {
                println!("  Output {}", output_index);
                println!("  Value: {} sats", output.value.to_sat());
                println!("  Script: {}", output.script_pubkey);
            }
        }
    }
}
