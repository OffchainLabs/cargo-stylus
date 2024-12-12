// Copyright 2022-2024, Offchain Labs, Inc.
// For licensing, see https://github.com/OffchainLabs/stylus-sdk-rs/blob/stylus/licenses/COPYRIGHT.md

#![allow(unused)]

use super::trace::{FrameReader, HostioKind::*};
use function_name::named;
use lazy_static::lazy_static;
use parking_lot::Mutex;
use std::{
    mem::{self, MaybeUninit},
    ptr::copy_nonoverlapping as memcpy,
};

lazy_static! {
    pub static ref FRAME: Mutex<Option<FrameReader>> = Mutex::new(None);
    pub static ref START_INK: Mutex<u64> = Mutex::new(0);
    pub static ref END_INK: Mutex<u64> = Mutex::new(0);
}

macro_rules! frame {
    ($dec:pat) => {
        let hostio = FRAME.lock().as_mut().unwrap().next_hostio(function_name!());
        *START_INK.lock() = hostio.start_ink;
        *END_INK.lock() = hostio.end_ink;

        let $dec = hostio.kind else { unreachable!() };
    };
}

macro_rules! copy {
    ($src:expr, $dest:expr) => {
        memcpy($src.as_ptr(), $dest, mem::size_of_val(&$src))
    };
    ($src:expr, $dest:expr, $len:expr) => {
        memcpy($src.as_ptr(), $dest, $len)
    };
}

/// Reads the contract calldata. The semantics are equivalent to that of the EVM's
/// [`CALLDATA_COPY`] opcode when requesting the entirety of the current call's calldata.
///
/// [`CALLDATA_COPY`]: https://www.evm.codes/#37
#[named]
#[no_mangle]
pub unsafe extern "C" fn read_args(dest: *mut u8) {
    frame!(ReadArgs { args });
    copy!(args, dest, args.len());
}

/// Writes the final return data. If not called before the contract exists, the return data will
/// be 0 bytes long. Note that this hostio does not cause the contract to exit, which happens
/// naturally when `user_entrypoint` returns.
#[named]
#[no_mangle]
pub unsafe extern "C" fn write_result(data: *const u8, len: u32) {
    frame!(WriteResult { result });
    assert_eq!(read_bytes(data, len), &*result);
}

/// Exits program execution early with the given status code.
/// If `0`, the program returns successfully with any data supplied by `write_result`.
/// Otherwise, the program reverts and treats any `write_result` data as revert data.
///
/// The semantics are equivalent to that of the EVM's [`Return`] and [`Revert`] opcodes.
/// Note: this function just traces, it's up to the caller to actually perform the exit.
///
/// [`Return`]: https://www.evm.codes/#f3
/// [`Revert`]: https://www.evm.codes/#fd
#[named]
#[no_mangle]
pub unsafe extern "C" fn exit_early(status: u32) {
    frame!(ExitEarly { status });
}

/// Reads a 32-byte value from permanent storage. Stylus's storage format is identical to
/// that of the EVM. This means that, under the hood, this hostio is accessing the 32-byte
/// value stored in the EVM state trie at offset `key`, which will be `0` when not previously
/// set. The semantics, then, are equivalent to that of the EVM's [`SLOAD`] opcode.
///
/// [`SLOAD`]: https://www.evm.codes/#54
#[named]
#[no_mangle]
pub unsafe extern "C" fn storage_load_bytes32(key_ptr: *const u8, dest: *mut u8) {
    frame!(StorageLoadBytes32 { key, value });
    assert_eq!(read_fixed(key_ptr), key);
    copy!(value, dest);
}

/// Writes a 32-byte value to the permanent storage cache. Stylus's storage format is identical to that
/// of the EVM. This means that, under the hood, this hostio represents storing a 32-byte value into
/// the EVM state trie at offset `key`. Refunds are tabulated exactly as in the EVM. The semantics, then,
/// are equivalent to that of the EVM's [`SSTORE`] opcode.
///
/// Note: because this value is cached, one must call `storage_flush_cache` to persist the value.
///
/// Auditor's note: we require the [`SSTORE`] sentry per EVM rules. The `gas_cost` returned by the EVM API
/// may exceed this amount, but that's ok because the predominant cost is due to state bloat concerns.
///
/// [`SSTORE`]: https://www.evm.codes/#55
#[named]
#[no_mangle]
pub unsafe extern "C" fn storage_cache_bytes32(key_ptr: *const u8, value_ptr: *const u8) {
    frame!(StorageCacheBytes32 { key, value });
    assert_eq!(read_fixed(key_ptr), key);
    assert_eq!(read_fixed(value_ptr), value);
}

/// Persists any dirty values in the storage cache to the EVM state trie, dropping the cache entirely if requested.
/// Analogous to repeated invocations of [`SSTORE`].
///
/// [`SSTORE`]: https://www.evm.codes/#55
#[named]
#[no_mangle]
pub unsafe extern "C" fn storage_flush_cache(clear: u32) {
    frame!(StorageFlushCache { clear });
}

/// Reads a 32-byte value from transient storage. Stylus's storage format is identical to
/// that of the EVM. This means that, under the hood, this hostio is accessing the 32-byte
/// value stored in the EVM's transient state trie at offset `key`, which will be `0` when not previously
/// set. The semantics, then, are equivalent to that of the EVM's [`TLOAD`] opcode.
///
/// [`TLOAD`]: https://www.evm.codes/#5c
#[named]
#[no_mangle]
pub unsafe extern "C" fn transient_load_bytes32(key_ptr: *const u8, dest: *mut u8) {
    frame!(TransientLoadBytes32 { key, value });
    assert_eq!(read_fixed(key_ptr), key);
    copy!(value, dest);
}

/// Writes a 32-byte value to transient storage. Stylus's storage format is identical to that
/// of the EVM. This means that, under the hood, this hostio represents storing a 32-byte value into
/// the EVM's transient state trie at offset `key`. The semantics, then, are equivalent to that of the
/// EVM's [`TSTORE`] opcode.
///
/// [`TSTORE`]: https://www.evm.codes/#5d
#[named]
#[no_mangle]
pub unsafe extern "C" fn transient_store_bytes32(key_ptr: *const u8, value_ptr: *const u8) {
    frame!(TransientStoreBytes32 { key, value });
    assert_eq!(read_fixed(key_ptr), key);
    assert_eq!(read_fixed(value_ptr), value);
}

/// Gets the ETH balance in wei of the account at the given address.
/// The semantics are equivalent to that of the EVM's [`BALANCE`] opcode.
///
/// [`BALANCE`]: https://www.evm.codes/#31
#[named]
#[no_mangle]
pub unsafe extern "C" fn account_balance(address_ptr: *const u8, dest: *mut u8) {
    frame!(AccountBalance { address, balance });
    assert_eq!(read_fixed(address_ptr), address);
    copy!(balance.to_be_bytes::<32>(), dest);
}

/// Gets a subset of the code from the account at the given address. The semantics are identical to that
/// of the EVM's [`EXT_CODE_COPY`] opcode, aside from one small detail: the write to the buffer `dest` will
/// stop after the last byte is written. This is unlike the EVM, which right pads with zeros in this scenario.
/// The return value is the number of bytes written, which allows the caller to detect if this has occured.
///
/// [`EXT_CODE_COPY`]: https://www.evm.codes/#3C
#[named]
#[no_mangle]
pub unsafe extern "C" fn account_code(
    address_ptr: *const u8,
    offset_recv: u32,
    size_recv: u32,
    dest: *mut u8,
) -> u32 {
    frame!(AccountCode {
        address,
        offset,
        size,
        code
    });
    assert_eq!(offset_recv, offset);
    assert_eq!(size_recv, size);
    assert_eq!(read_fixed(address_ptr), address);
    copy!(code, dest);
    code.len() as u32
}

/// Gets the size of the code in bytes at the given address. The semantics are equivalent
/// to that of the EVM's [`EXT_CODESIZE`].
///
/// [`EXT_CODESIZE`]: https://www.evm.codes/#3B
#[named]
#[no_mangle]
pub unsafe extern "C" fn account_code_size(address_ptr: *const u8) -> u32 {
    frame!(AccountCodeSize { address, size });
    assert_eq!(read_fixed(address_ptr), address);
    size
}

/// Gets the code hash of the account at the given address. The semantics are equivalent
/// to that of the EVM's [`EXT_CODEHASH`] opcode. Note that the code hash of an account without
/// code will be the empty hash
/// `keccak("") = c5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470`.
///
/// [`EXT_CODEHASH`]: https://www.evm.codes/#3F
#[named]
#[no_mangle]
pub unsafe extern "C" fn account_codehash(address_ptr: *const u8, dest: *mut u8) {
    frame!(AccountCodehash { address, codehash });
    assert_eq!(read_fixed(address_ptr), address);
    copy!(codehash, dest);
}

/// Gets the basefee of the current block. The semantics are equivalent to that of the EVM's
/// [`BASEFEE`] opcode.
///
/// [`BASEFEE`]: https://www.evm.codes/#48
#[named]
#[no_mangle]
pub unsafe extern "C" fn block_basefee(dest: *mut u8) {
    frame!(BlockBasefee { basefee });
    copy!(basefee.to_be_bytes::<32>(), dest);
}

/// Gets the coinbase of the current block, which on Arbitrum chains is the L1 batch poster's
/// address. This differs from Ethereum where the validator including the transaction
/// determines the coinbase.
#[named]
#[no_mangle]
pub unsafe extern "C" fn block_coinbase(dest: *mut u8) {
    frame!(BlockCoinbase { coinbase });
    copy!(coinbase, dest);
}

/// Gets the gas limit of the current block. The semantics are equivalent to that of the EVM's
/// [`GAS_LIMIT`] opcode. Note that as of the time of this writing, `evm.codes` incorrectly
/// implies that the opcode returns the gas limit of the current transaction.  When in doubt,
/// consult [`The Ethereum Yellow Paper`].
///
/// [`GAS_LIMIT`]: https://www.evm.codes/#45
/// [`The Ethereum Yellow Paper`]: https://ethereum.github.io/yellowpaper/paper.pdf
#[named]
#[no_mangle]
pub unsafe extern "C" fn block_gas_limit() -> u64 {
    frame!(BlockGasLimit { limit });
    limit
}

/// Gets a bounded estimate of the L1 block number at which the Sequencer sequenced the
/// transaction. See [`Block Numbers and Time`] for more information on how this value is
/// determined.
///
/// [`Block Numbers and Time`]: https://developer.arbitrum.io/time
#[named]
#[no_mangle]
pub unsafe extern "C" fn block_number() -> u64 {
    frame!(BlockNumber { number });
    number
}

/// Gets a bounded estimate of the Unix timestamp at which the Sequencer sequenced the
/// transaction. See [`Block Numbers and Time`] for more information on how this value is
/// determined.
///
/// [`Block Numbers and Time`]: https://developer.arbitrum.io/time
#[named]
#[no_mangle]
pub unsafe extern "C" fn block_timestamp() -> u64 {
    frame!(BlockTimestamp { timestamp });
    timestamp
}

/// Gets the unique chain identifier of the Arbitrum chain. The semantics are equivalent to
/// that of the EVM's [`CHAIN_ID`] opcode.
///
/// [`CHAIN_ID`]: https://www.evm.codes/#46
#[named]
#[no_mangle]
pub unsafe extern "C" fn chainid() -> u64 {
    frame!(Chainid { chainid });
    chainid
}

/// Calls the contract at the given address with options for passing value and to limit the
/// amount of gas supplied. The return status indicates whether the call succeeded, and is
/// nonzero on failure.
///
/// In both cases `return_data_len` will store the length of the result, the bytes of which can
/// be read via the `read_return_data` hostio. The bytes are not returned directly so that the
/// programmer can potentially save gas by choosing which subset of the return result they'd
/// like to copy.
///
/// The semantics are equivalent to that of the EVM's [`CALL`] opcode, including callvalue
/// stipends and the 63/64 gas rule. This means that supplying the `u64::MAX` gas can be used
/// to send as much as possible.
///
/// [`CALL`]: https://www.evm.codes/#f1
#[named]
#[no_mangle]
pub unsafe extern "C" fn call_contract(
    address_ptr: *const u8,
    calldata: *const u8,
    calldata_len: u32,
    value_ptr: *const u8,
    gas_supplied: u64,
    return_data_len: *mut u32,
) -> u8 {
    frame!(CallContract {
        address,
        data,
        gas,
        value,
        outs_len,
        status,
        frame,
    });
    assert_eq!(read_fixed(address_ptr), address);
    assert_eq!(read_bytes(calldata, calldata_len), &*data);
    assert_eq!(read_fixed(value_ptr), value.to_be_bytes::<32>());
    assert_eq!(gas_supplied, gas);
    *return_data_len = outs_len;
    status
}

/// Delegate calls the contract at the given address, with the option to limit the amount of
/// gas supplied. The return status indicates whether the call succeeded, and is nonzero on
/// failure.
///
/// In both cases `return_data_len` will store the length of the result, the bytes of which
/// can be read via the `read_return_data` hostio. The bytes are not returned directly so that
/// the programmer can potentially save gas by choosing which subset of the return result
/// they'd like to copy.
///
/// The semantics are equivalent to that of the EVM's [`DELEGATE_CALL`] opcode, including the
/// 63/64 gas rule. This means that supplying `u64::MAX` gas can be used to send as much as
/// possible.
///
/// [`DELEGATE_CALL`]: https://www.evm.codes/#F4
#[named]
#[no_mangle]
pub unsafe extern "C" fn delegate_call_contract(
    address_ptr: *const u8,
    calldata: *const u8,
    calldata_len: u32,
    gas_supplied: u64,
    return_data_len: *mut u32,
) -> u8 {
    frame!(DelegateCallContract {
        address,
        data,
        gas,
        outs_len,
        status,
        frame,
    });
    assert_eq!(read_fixed(address_ptr), address);
    assert_eq!(read_bytes(calldata, calldata_len), &*data);
    assert_eq!(gas_supplied, gas);
    *return_data_len = outs_len;
    status
}

/// Static calls the contract at the given address, with the option to limit the amount of gas
/// supplied. The return status indicates whether the call succeeded, and is nonzero on
/// failure.
///
/// In both cases `return_data_len` will store the length of the result, the bytes of which can
/// be read via the `read_return_data` hostio. The bytes are not returned directly so that the
/// programmer can potentially save gas by choosing which subset of the return result they'd
/// like to copy.
///
/// The semantics are equivalent to that of the EVM's [`STATIC_CALL`] opcode, including the
/// 63/64 gas rule. This means that supplying `u64::MAX` gas can be used to send as much as
/// possible.
///
/// [`STATIC_CALL`]: https://www.evm.codes/#FA
#[named]
#[no_mangle]
pub unsafe extern "C" fn static_call_contract(
    address_ptr: *const u8,
    calldata: *const u8,
    calldata_len: u32,
    gas_supplied: u64,
    return_data_len: *mut u32,
) -> u8 {
    frame!(StaticCallContract {
        address,
        data,
        gas,
        outs_len,
        status,
        frame,
    });
    assert_eq!(read_fixed(address_ptr), address);
    assert_eq!(read_bytes(calldata, calldata_len), &*data);
    assert_eq!(gas_supplied, gas);
    *return_data_len = outs_len;
    status
}

/// Gets the address of the current contract. The semantics are equivalent to that of the EVM's
/// [`ADDRESS`] opcode.
///
/// [`ADDRESS`]: https://www.evm.codes/#30
#[named]
#[no_mangle]
pub unsafe extern "C" fn contract_address(dest: *mut u8) {
    frame!(ContractAddress { address });
    copy!(address, dest);
}

/// Deploys a new contract using the init code provided, which the EVM executes to construct
/// the code of the newly deployed contract. The init code must be written in EVM bytecode, but
/// the code it deploys can be that of a Stylus contract. The code returned will be treated as
/// WASM if it begins with the EOF-inspired header `0xEFF000`. Otherwise the code will be
/// interpreted as that of a traditional EVM-style contract. See [`Deploying Stylus Contracts`]
/// for more information on writing init code.
///
/// On success, this hostio returns the address of the newly created account whose address is
/// a function of the sender and nonce. On failure the address will be `0`, `return_data_len`
/// will store the length of the revert data, the bytes of which can be read via the
/// `read_return_data` hostio. The semantics are equivalent to that of the EVM's [`CREATE`]
/// opcode, which notably includes the exact address returned.
///
/// [`Deploying Stylus Contracts`]: https://developer.arbitrum.io/TODO
/// [`CREATE`]: https://www.evm.codes/#f0
#[named]
#[no_mangle]
pub unsafe extern "C" fn create1(
    code_ptr: *const u8,
    code_len: u32,
    value: *const u8,
    contract: *mut u8,
    revert_data_len_ptr: *mut u32,
) {
    frame!(Create1 {
        code,
        endowment,
        address,
        revert_data_len
    });
    assert_eq!(read_bytes(code_ptr, code_len), &*code);
    assert_eq!(read_fixed(value), endowment.to_be_bytes::<32>());
    copy!(address, contract);
    *revert_data_len_ptr = revert_data_len;
}

/// Deploys a new contract using the init code provided, which the EVM executes to construct
/// the code of the newly deployed contract. The init code must be written in EVM bytecode, but
/// the code it deploys can be that of a Stylus contract. The code returned will be treated as
/// WASM if it begins with the EOF-inspired header `0xEFF000`. Otherwise the code will be
/// interpreted as that of a traditional EVM-style contract. See [`Deploying Stylus Contracts`]
/// for more information on writing init code.
///
/// On success, this hostio returns the address of the newly created account whose address is a
/// function of the sender, salt, and init code. On failure the address will be `0`,
/// `return_data_len` will store the length of the revert data, the bytes of which can be read
/// via the `read_return_data` hostio. The semantics are equivalent to that of the EVM's
/// `[CREATE2`] opcode, which notably includes the exact address returned.
///
/// [`Deploying Stylus Contracts`]: https://developer.arbitrum.io/TODO
/// [`CREATE2`]: https://www.evm.codes/#f5
#[named]
#[no_mangle]
pub unsafe extern "C" fn create2(
    code_ptr: *const u8,
    code_len: u32,
    value_ptr: *const u8,
    salt_ptr: *const u8,
    contract: *mut u8,
    revert_data_len_ptr: *mut u32,
) {
    frame!(Create2 {
        code,
        endowment,
        salt,
        address,
        revert_data_len
    });
    assert_eq!(read_bytes(code_ptr, code_len), &*code);
    assert_eq!(read_fixed(value_ptr), endowment.to_be_bytes::<32>());
    assert_eq!(read_fixed(salt_ptr), salt);
    copy!(address, contract);
    *revert_data_len_ptr = revert_data_len;
}

/// Emits an EVM log with the given number of topics and data, the first bytes of which should
/// be the 32-byte-aligned topic data. The semantics are equivalent to that of the EVM's
/// [`LOG0`], [`LOG1`], [`LOG2`], [`LOG3`], and [`LOG4`] opcodes based on the number of topics
/// specified. Requesting more than `4` topics will induce a revert.
///
/// [`LOG0`]: https://www.evm.codes/#a0
/// [`LOG1`]: https://www.evm.codes/#a1
/// [`LOG2`]: https://www.evm.codes/#a2
/// [`LOG3`]: https://www.evm.codes/#a3
/// [`LOG4`]: https://www.evm.codes/#a4
#[named]
#[no_mangle]
pub unsafe extern "C" fn emit_log(data_ptr: *const u8, len: u32, topic_count: u32) {
    frame!(EmitLog { data, topics });
    assert_eq!(read_bytes(data_ptr, len), &*data);
    assert_eq!(topics, topic_count);
}

/// Gets the amount of gas left after paying for the cost of this hostio. The semantics are
/// equivalent to that of the EVM's [`GAS`] opcode.
///
/// [`GAS`]: https://www.evm.codes/#5a
#[named]
#[no_mangle]
pub unsafe extern "C" fn evm_gas_left() -> u64 {
    frame!(EvmGasLeft { gas_left });
    gas_left
}

/// Gets the amount of ink remaining after paying for the cost of this hostio. The semantics
/// are equivalent to that of the EVM's [`GAS`] opcode, except the units are in ink. See
/// [`Ink and Gas`] for more information on Stylus's compute pricing.
///
/// [`GAS`]: https://www.evm.codes/#5a
/// [`Ink and Gas`]: https://developer.arbitrum.io/TODO
#[named]
#[no_mangle]
pub unsafe extern "C" fn evm_ink_left() -> u64 {
    frame!(EvmInkLeft { ink_left });
    ink_left
}

/// The `entrypoint!` macro handles importing this hostio, which is required if the
/// contract's memory grows. Otherwise compilation through the `ArbWasm` precompile will revert.
/// Internally the Stylus VM forces calls to this hostio whenever new WASM pages are allocated.
/// Calls made voluntarily will unproductively consume gas.
#[named]
#[no_mangle]
pub unsafe extern "C" fn pay_for_memory_grow(new_pages: u16) {
    frame!(PayForMemoryGrow { pages });
    assert_eq!(new_pages, pages);
}

/// Computes `value ÷ exponent` using 256-bit math, writing the result to the first.
/// The semantics are equivalent to that of the EVM's [`DIV`] opcode, which means that a `divisor` of `0`
/// writes `0` to `value`.
///
/// [`DIV`]: https://www.evm.codes/#04
#[named]
#[no_mangle]
pub unsafe fn math_div(value: *mut u8, divisor: *const u8) {
    frame!(MathDiv { a, b, result });
    assert_eq!(read_fixed(value), a.to_be_bytes::<32>());
    assert_eq!(read_fixed(divisor), b.to_be_bytes::<32>());
    copy!(result.to_be_bytes::<32>(), value);
}

/// Computes `value % exponent` using 256-bit math, writing the result to the first.
/// The semantics are equivalent to that of the EVM's [`MOD`] opcode, which means that a `modulus` of `0`
/// writes `0` to `value`.
///
/// [`MOD`]: https://www.evm.codes/#06
#[named]
#[no_mangle]
pub unsafe fn math_mod(value: *mut u8, modulus: *const u8) {
    frame!(MathMod { a, b, result });
    assert_eq!(read_fixed(value), a.to_be_bytes::<32>());
    assert_eq!(read_fixed(modulus), b.to_be_bytes::<32>());
    copy!(result.to_be_bytes::<32>(), value);
}

/// Computes `value ^ exponent` using 256-bit math, writing the result to the first.
/// The semantics are equivalent to that of the EVM's [`EXP`] opcode.
///
/// [`EXP`]: https://www.evm.codes/#0A
#[named]
#[no_mangle]
pub unsafe fn math_pow(value: *mut u8, exponent: *const u8) {
    frame!(MathPow { a, b, result });
    assert_eq!(read_fixed(value), a.to_be_bytes::<32>());
    assert_eq!(read_fixed(exponent), b.to_be_bytes::<32>());
    copy!(result.to_be_bytes::<32>(), value);
}

/// Computes `(value + addend) % modulus` using 256-bit math, writing the result to the first.
/// The semantics are equivalent to that of the EVM's [`ADDMOD`] opcode, which means that a `modulus` of `0`
/// writes `0` to `value`.
///
/// [`ADDMOD`]: https://www.evm.codes/#08
#[named]
#[no_mangle]
pub unsafe fn math_add_mod(value: *mut u8, addend: *const u8, modulus: *const u8) {
    frame!(MathAddMod { a, b, c, result });
    assert_eq!(read_fixed(value), a.to_be_bytes::<32>());
    assert_eq!(read_fixed(addend), b.to_be_bytes::<32>());
    assert_eq!(read_fixed(modulus), c.to_be_bytes::<32>());
    copy!(result.to_be_bytes::<32>(), value);
}

/// Computes `(value * multiplier) % modulus` using 256-bit math, writing the result to the first.
/// The semantics are equivalent to that of the EVM's [`MULMOD`] opcode, which means that a `modulus` of `0`
/// writes `0` to `value`.
///
/// [`MULMOD`]: https://www.evm.codes/#09
#[named]
#[no_mangle]
pub unsafe fn math_mul_mod(value: *mut u8, multiplier: *const u8, modulus: *const u8) {
    frame!(MathAddMod { a, b, c, result });
    assert_eq!(read_fixed(value), a.to_be_bytes::<32>());
    assert_eq!(read_fixed(multiplier), b.to_be_bytes::<32>());
    assert_eq!(read_fixed(modulus), c.to_be_bytes::<32>());
    copy!(result.to_be_bytes::<32>(), value);
}

/// Whether the current call is reentrant.
#[named]
#[no_mangle]
pub unsafe extern "C" fn msg_reentrant() -> bool {
    frame!(MsgReentrant { reentrant });
    reentrant
}

/// Gets the address of the account that called the contract. For normal L2-to-L2 transactions
/// the semantics are equivalent to that of the EVM's [`CALLER`] opcode, including in cases
/// arising from [`DELEGATE_CALL`].
///
/// For L1-to-L2 retryable ticket transactions, the top-level sender's address will be aliased.
/// See [`Retryable Ticket Address Aliasing`] for more information on how this works.
///
/// [`CALLER`]: https://www.evm.codes/#33
/// [`DELEGATE_CALL`]: https://www.evm.codes/#f4
/// [`Retryable Ticket Address Aliasing`]: https://developer.arbitrum.io/arbos/l1-to-l2-messaging#address-aliasing
#[named]
#[no_mangle]
pub unsafe extern "C" fn msg_sender(dest: *mut u8) {
    frame!(MsgSender { sender });
    copy!(sender, dest);
}

/// Get the ETH value in wei sent to the contract. The semantics are equivalent to that of the
/// EVM's [`CALLVALUE`] opcode.
///
/// [`CALLVALUE`]: https://www.evm.codes/#34
#[named]
#[no_mangle]
pub unsafe extern "C" fn msg_value(dest: *mut u8) {
    frame!(MsgValue { value });
    copy!(value, dest);
}

/// Efficiently computes the [`keccak256`] hash of the given preimage.
/// The semantics are equivalent to that of the EVM's [`SHA3`] opcode.
///
/// [`keccak256`]: https://en.wikipedia.org/wiki/SHA-3
/// [`SHA3`]: https://www.evm.codes/#20
#[named]
#[no_mangle]
pub unsafe extern "C" fn native_keccak256(bytes: *const u8, len: u32, output: *mut u8) {
    frame!(NativeKeccak256 { preimage, digest });
    assert_eq!(read_bytes(bytes, len), &*preimage);
    copy!(digest, output);
}

/// Copies the bytes of the last EVM call or deployment return result. Does not revert if out of
/// bounds, but rather copies the overlapping portion. The semantics are otherwise equivalent
/// to that of the EVM's [`RETURN_DATA_COPY`] opcode.
///
/// [`RETURN_DATA_COPY`]: https://www.evm.codes/#3e
#[named]
#[no_mangle]
pub unsafe extern "C" fn read_return_data(
    dest: *mut u8,
    offset_value: u32,
    size_value: u32,
) -> u32 {
    frame!(ReadReturnData { offset, size, data });
    assert_eq!(offset_value, offset);
    assert_eq!(size_value, size);
    copy!(data, dest, data.len());
    data.len() as u32
}

/// Returns the length of the last EVM call or deployment return result, or `0` if neither have
/// happened during the contract's execution. The semantics are equivalent to that of the EVM's
/// [`RETURN_DATA_SIZE`] opcode.
///
/// [`RETURN_DATA_SIZE`]: https://www.evm.codes/#3d
#[named]
#[no_mangle]
pub unsafe extern "C" fn return_data_size() -> u32 {
    frame!(ReturnDataSize { size });
    size
}

/// Gets the gas price in wei per gas, which on Arbitrum chains equals the basefee. The
/// semantics are equivalent to that of the EVM's [`GAS_PRICE`] opcode.
///
/// [`GAS_PRICE`]: https://www.evm.codes/#3A
#[named]
#[no_mangle]
pub unsafe extern "C" fn tx_gas_price(dest: *mut u8) {
    frame!(TxGasPrice { gas_price });
    copy!(gas_price.to_be_bytes::<32>(), dest);
}

/// Gets the price of ink in evm gas basis points. See [`Ink and Gas`] for more information on
/// Stylus's compute-pricing model.
///
/// [`Ink and Gas`]: https://developer.arbitrum.io/TODO
#[named]
#[no_mangle]
pub unsafe extern "C" fn tx_ink_price() -> u32 {
    frame!(TxInkPrice { ink_price });
    ink_price
}

/// Gets the top-level sender of the transaction. The semantics are equivalent to that of the
/// EVM's [`ORIGIN`] opcode.
///
/// [`ORIGIN`]: https://www.evm.codes/#32
#[named]
#[no_mangle]
pub unsafe extern "C" fn tx_origin(dest: *mut u8) {
    frame!(TxOrigin { origin });
    copy!(origin, dest);
}

/// Prints a 32-bit floating point number to the console. Only available in debug mode with
/// floating point enabled.
#[named]
#[no_mangle]
pub unsafe extern "C" fn log_f32(value: f32) {
    frame!(ConsoleLog { text });
    println!("{text}");
}

/// Prints a 64-bit floating point number to the console. Only available in debug mode with
/// floating point enabled.
#[named]
#[no_mangle]
pub unsafe extern "C" fn log_f64(value: f64) {
    frame!(ConsoleLog { text });
    println!("{text}");
}

/// Prints a 32-bit integer to the console, which can be either signed or unsigned.
/// Only available in debug mode.
#[named]
#[no_mangle]
pub unsafe extern "C" fn log_i32(value: i32) {
    frame!(ConsoleLog { text });
    println!("{text}");
}

/// Prints a 64-bit integer to the console, which can be either signed or unsigned.
/// Only available in debug mode.
#[named]
#[no_mangle]
pub unsafe extern "C" fn log_i64(value: i64) {
    frame!(ConsoleLog { text });
    println!("{text}");
}

/// Prints a UTF-8 encoded string to the console. Only available in debug mode.
#[named]
#[no_mangle]
pub unsafe extern "C" fn log_txt(text_ptr: *const u8, len: u32) {
    frame!(ConsoleLogText { text });
    assert_eq!(read_bytes(text_ptr, len), &*text);
}

unsafe fn read_fixed<const N: usize>(ptr: *const u8) -> [u8; N] {
    let mut value = MaybeUninit::<[u8; N]>::uninit();
    memcpy(ptr, value.as_mut_ptr() as *mut _, N);
    value.assume_init()
}

unsafe fn read_bytes(ptr: *const u8, len: u32) -> Vec<u8> {
    let len = len as usize;
    let mut data = Vec::with_capacity(len);
    memcpy(ptr, data.as_mut_ptr(), len);
    data.set_len(len);
    data
}
