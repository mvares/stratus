#!/bin/bash
compile() {
    dir=$1
    file=$2

    # compile
    solc --base-path $dir/contracts --include-path $dir/node_modules --bin --optimize -o target --overwrite $dir/contracts/$file
    solc --base-path $dir/contracts --include-path $dir/node_modules --abi --optimize -o target --overwrite $dir/contracts/$file
    solc --base-path $dir/contracts --include-path $dir/node_modules --hashes --optimize -o target --overwrite $dir/contracts/$file

    # copy from target folder to tests
    file_basename=$(basename $file)
    file_basename="${file_basename%.*}"
    cp target/$file_basename.bin ../tests/contracts/
    cp target/$file_basename.abi ../tests/contracts/
    cp target/$file_basename.signatures ../tests/contracts/
}

rm ../tests/contracts/*
compile ../../brlc-token BRLCToken.sol
compile ../../brlc-periphery CardPaymentProcessor.sol
compile ../../brlc-periphery PixCashier.sol
rm -rf target