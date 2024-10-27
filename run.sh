echo "build and test"
cargo build && cargo install --path .
cd test
moleculec --language cellscript --schema-file blockchain.mol > blockchain.cell
git diff blockchain.cell