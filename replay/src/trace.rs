// Copyright 2023, Offchain Labs, Inc.
// For licensing, see https://github.com/OffchainLabs/cargo-stylus/blob/stylus/licenses/COPYRIGHT.md

#![allow(clippy::redundant_closure_call)]

use alloy_primitives::{Address, FixedBytes, TxHash, B256, U256};
use cargo_stylus_util::color::{Color, DebugColor};
use ethers::{
    providers::{JsonRpcClient, Middleware, Provider},
    types::{
        GethDebugTracerType, GethDebugTracingOptions, GethTrace, Transaction, TransactionReceipt,
    },
    utils::__serde_json::Value,
};
use eyre::{bail, Result};
use sneks::SimpleSnakeNames;
use std::{collections::VecDeque, mem};

#[derive(Debug)]
pub struct Trace {
    pub top_frame: TraceFrame,
    pub receipt: TransactionReceipt,
    pub tx: Transaction,
    pub json: Value,
}

impl Trace {
    pub async fn new<T: JsonRpcClient>(provider: Provider<T>, tx: TxHash) -> Result<Self> {
        let hash = tx.0.into();

        let Some(receipt) = provider.get_transaction_receipt(hash).await? else {
            bail!("failed to get receipt for tx: {}", hash)
        };
        let Some(tx) = provider.get_transaction(hash).await? else {
            bail!("failed to get tx data: {}", hash)
        };

        let query = include_str!("query.js");
        let tracer = GethDebugTracingOptions {
            tracer: Some(GethDebugTracerType::JsTracer(query.to_owned())),
            ..GethDebugTracingOptions::default()
        };
        let GethTrace::Unknown(json) = provider.debug_trace_transaction(hash, tracer).await? else {
            bail!("malformed tracing result")
        };

        let to = receipt.to.map(|x| Address::from(x.0));
        let top_frame = TraceFrame::parse_frame(to, json.clone())?;

        Ok(Self {
            top_frame,
            receipt,
            tx,
            json,
        })
    }

    pub fn reader(self) -> FrameReader {
        FrameReader {
            steps: self.top_frame.steps.clone().into(),
            frame: self.top_frame,
        }
    }
}

#[derive(Clone, Debug)]
pub struct TraceFrame {
    steps: Vec<Hostio>,
    address: Option<Address>,
}

impl TraceFrame {
    fn new(address: Option<Address>) -> Self {
        let steps = vec![];
        Self { steps, address }
    }

    pub fn parse_frame(address: Option<Address>, array: Value) -> Result<TraceFrame> {
        let mut frame = TraceFrame::new(address);

        let Value::Array(array) = array else {
            bail!("not an array: {}", array);
        };

        for step in array {
            let Value::Object(mut keys) = step else {
                bail!("not a valid step: {}", step);
            };

            macro_rules! get_typed {
                ($keys:expr, $ty:ident, $name:expr) => {{
                    let value = match $keys.remove($name) {
                        Some(name) => name,
                        None => bail!("object missing {}: {:?}", $name, $keys),
                    };
                    match value {
                        Value::$ty(string) => string,
                        x => bail!("unexpected type for {}: {}", $name, x),
                    }
                }};
            }
            macro_rules! get_int {
                ($name:expr) => {
                    get_typed!(keys, Number, $name).as_u64().unwrap()
                };
            }

            let name = get_typed!(keys, String, "name");
            let args = get_typed!(keys, Array, "args");
            let outs = get_typed!(keys, Array, "outs");

            let mut args = args.as_slice();
            let mut outs = outs.as_slice();

            let start_ink = get_int!("startInk");
            let end_ink = get_int!("endInk");

            fn read_data(values: &[Value]) -> Result<Box<[u8]>> {
                let mut vec = vec![];
                for value in values {
                    let Value::Number(byte) = value else {
                        bail!("expected a byte but found {value}");
                    };
                    let byte = byte.as_u64().unwrap();
                    if byte > 255 {
                        bail!("expected a byte but found {byte}");
                    };
                    vec.push(byte as u8);
                }
                Ok(vec.into_boxed_slice())
            }

            macro_rules! read_data {
                ($src:ident) => {{
                    let data = read_data(&$src)?;
                    $src = &[];
                    data
                }};
            }
            macro_rules! read_ty {
                ($src:ident, $ty:ident, $conv:expr) => {{
                    let size = mem::size_of::<$ty>();
                    let data = read_data(&$src[..size])?;
                    $src = &$src[size..];
                    $conv(&data[..])
                }};
            }
            macro_rules! read_string {
                ($src:ident) => {{
                    let conv = |x: &[_]| String::from_utf8_lossy(&x).to_string();
                    read_ty!($src, String, conv)
                }};
            }
            macro_rules! read_u256 {
                ($src:ident) => {
                    read_ty!($src, U256, |x| B256::from_slice(x).into())
                };
            }
            macro_rules! read_b256 {
                ($src:ident) => {
                    read_ty!($src, B256, B256::from_slice)
                };
            }
            macro_rules! read_address {
                ($src:ident) => {
                    read_ty!($src, Address, Address::from_slice)
                };
            }
            macro_rules! read_num {
                ($src:ident, $ty:ident) => {{
                    let conv = |x: &[_]| $ty::from_be_bytes(x.try_into().unwrap());
                    read_ty!($src, $ty, conv)
                }};
            }
            macro_rules! read_u8 {
                ($src:ident) => {
                    read_num!($src, u8)
                };
            }
            macro_rules! read_u16 {
                ($src:ident) => {
                    read_num!($src, u16)
                };
            }
            macro_rules! read_u32 {
                ($src:ident) => {
                    read_num!($src, u32)
                };
            }
            macro_rules! read_u64 {
                ($src:ident) => {
                    read_num!($src, u64)
                };
            }
            macro_rules! read_usize {
                ($src:ident) => {
                    read_num!($src, usize)
                };
            }

            macro_rules! frame {
                () => {{
                    let mut info = get_typed!(keys, Object, "info");

                    // geth uses the pattern { "0": Number, "1": Number, ... }
                    let address = get_typed!(info, Object, "address");
                    let mut address: Vec<_> = address.into_iter().collect();
                    address.sort_by_key(|x| x.0.parse::<u8>().unwrap());
                    let address: Vec<_> = address.into_iter().map(|x| x.1).collect();
                    let address = Address::from_slice(&*read_data(&address)?);

                    let steps = info.remove("steps").unwrap();
                    TraceFrame::parse_frame(Some(address), steps)?
                }};
            }

            use HostioKind::*;
            let kind = match name.as_str() {
                "user_entrypoint" => UserEntrypoint {
                    args_len: read_u32!(args),
                },
                "user_returned" => UserReturned {
                    status: read_u32!(outs),
                },
                "read_args" => ReadArgs {
                    args: read_data!(outs),
                },
                "write_result" => WriteResult {
                    result: read_data!(args),
                },
                "exit_early" => ExitEarly {
                    status: read_u32!(args),
                },
                "storage_load_bytes32" => StorageLoadBytes32 {
                    key: read_b256!(args),
                    value: read_b256!(outs),
                },
                "storage_store_bytes32" => StorageStoreBytes32 {
                    key: read_b256!(args),
                    value: read_b256!(args),
                },
                "storage_cache_bytes32" => StorageCacheBytes32 {
                    key: read_b256!(args),
                    value: read_b256!(args),
                },
                "storage_flush_cache" => StorageFlushCache {
                    clear: read_u8!(args),
                },
                "account_balance" => AccountBalance {
                    address: read_address!(args),
                    balance: read_u256!(outs),
                },
                "account_codehash" => AccountCodehash {
                    address: read_address!(args),
                    codehash: read_b256!(outs),
                },
                "block_basefee" => BlockBasefee {
                    basefee: read_u256!(outs),
                },
                "block_coinbase" => BlockCoinbase {
                    coinbase: read_address!(outs),
                },
                "block_gas_limit" => BlockGasLimit {
                    limit: read_u64!(outs),
                },
                "block_number" => BlockNumber {
                    number: read_u64!(outs),
                },
                "block_timestamp" => BlockTimestamp {
                    timestamp: read_u64!(outs),
                },
                "chainid" => Chainid {
                    chainid: read_u64!(outs),
                },
                "contract_address" => ContractAddress {
                    address: read_address!(outs),
                },
                "evm_gas_left" => EvmGasLeft {
                    gas_left: read_u64!(outs),
                },
                "evm_ink_left" => EvmInkLeft {
                    ink_left: read_u64!(outs),
                },
                "msg_reentrant" => MsgReentrant {
                    reentrant: read_u32!(outs) != 0,
                },
                "msg_sender" => MsgSender {
                    sender: read_address!(outs),
                },
                "msg_value" => MsgValue {
                    value: read_b256!(outs),
                },
                "native_keccak256" => NativeKeccak256 {
                    preimage: read_data!(args),
                    digest: read_b256!(outs),
                },
                "tx_gas_price" => TxGasPrice {
                    gas_price: read_u256!(outs),
                },
                "tx_ink_price" => TxInkPrice {
                    ink_price: read_u32!(outs),
                },
                "tx_origin" => TxOrigin {
                    origin: read_address!(outs),
                },
                "pay_for_memory_grow" => PayForMemoryGrow {
                    pages: read_u16!(args),
                },
                "call_contract" => CallContract {
                    address: read_address!(args),
                    gas: read_u64!(args),
                    value: read_u256!(args),
                    data: read_data!(args),
                    outs_len: read_u32!(outs),
                    status: read_u8!(outs),
                    frame: frame!(),
                },
                "delegate_call_contract" => DelegateCallContract {
                    address: read_address!(args),
                    gas: read_u64!(args),
                    data: read_data!(args),
                    outs_len: read_u32!(outs),
                    status: read_u8!(outs),
                    frame: frame!(),
                },
                "static_call_contract" => StaticCallContract {
                    address: read_address!(args),
                    gas: read_u64!(args),
                    data: read_data!(args),
                    outs_len: read_u32!(outs),
                    status: read_u8!(outs),
                    frame: frame!(),
                },
                "create1" => Create1 {
                    endowment: read_u256!(args),
                    code: read_data!(args),
                    address: read_address!(outs),
                    revert_data_len: read_usize!(outs),
                },
                "create2" => Create2 {
                    endowment: read_u256!(args),
                    salt: read_b256!(args),
                    code: read_data!(args),
                    address: read_address!(outs),
                    revert_data_len: read_usize!(outs),
                },
                "emit_log" => EmitLog {
                    topics: read_usize!(args),
                    data: read_data!(args),
                },
                "read_return_data" => ReadReturnData {
                    offset: read_u32!(args),
                    size: read_u32!(args),
                    data: read_data!(outs),
                },
                "return_data_size" => ReturnDataSize {
                    size: read_usize!(outs),
                },
                "console_log_text" => ConsoleLogText {
                    text: read_data!(args),
                },
                "console_log" => ConsoleLog {
                    text: read_string!(args),
                },
                x => todo!("Missing hostio details {x}"),
            };

            assert!(args.is_empty(), "{name}");
            assert!(outs.is_empty(), "{name}");

            frame.steps.push(Hostio {
                kind,
                start_ink,
                end_ink,
            });
        }
        Ok(frame)
    }
}

#[derive(Clone, Debug)]
pub struct Hostio {
    pub kind: HostioKind,
    pub start_ink: u64,
    pub end_ink: u64,
}

#[derive(Clone, Debug, SimpleSnakeNames)]
pub enum HostioKind {
    UserEntrypoint {
        args_len: u32,
    },
    UserReturned {
        status: u32,
    },
    ReadArgs {
        args: Box<[u8]>,
    },
    WriteResult {
        result: Box<[u8]>,
    },
    ExitEarly {
        status: u32,
    },
    StorageLoadBytes32 {
        key: B256,
        value: B256,
    },
    StorageStoreBytes32 {
        key: B256,
        value: B256,
    },
    StorageCacheBytes32 {
        key: FixedBytes<32>,
        value: FixedBytes<32>,
    },
    StorageFlushCache {
        clear: u8,
    },
    AccountBalance {
        address: Address,
        balance: U256,
    },
    AccountCodehash {
        address: Address,
        codehash: B256,
    },
    BlockBasefee {
        basefee: U256,
    },
    BlockCoinbase {
        coinbase: Address,
    },
    BlockGasLimit {
        limit: u64,
    },
    BlockNumber {
        number: u64,
    },
    BlockTimestamp {
        timestamp: u64,
    },
    Chainid {
        chainid: u64,
    },
    ContractAddress {
        address: Address,
    },
    EvmGasLeft {
        gas_left: u64,
    },
    EvmInkLeft {
        ink_left: u64,
    },
    PayForMemoryGrow {
        pages: u16,
    },
    MsgReentrant {
        reentrant: bool,
    },
    MsgSender {
        sender: Address,
    },
    MsgValue {
        value: B256,
    },
    NativeKeccak256 {
        preimage: Box<[u8]>,
        digest: B256,
    },
    TxGasPrice {
        gas_price: U256,
    },
    TxInkPrice {
        ink_price: u32,
    },
    TxOrigin {
        origin: Address,
    },
    ConsoleLog {
        text: String,
    },
    ConsoleLogText {
        text: Box<[u8]>,
    },
    CallContract {
        address: Address,
        data: Box<[u8]>,
        gas: u64,
        value: U256,
        outs_len: u32,
        status: u8,
        frame: TraceFrame,
    },
    DelegateCallContract {
        address: Address,
        data: Box<[u8]>,
        gas: u64,
        outs_len: u32,
        status: u8,
        frame: TraceFrame,
    },
    StaticCallContract {
        address: Address,
        data: Box<[u8]>,
        gas: u64,
        outs_len: u32,
        status: u8,
        frame: TraceFrame,
    },
    Create1 {
        code: Box<[u8]>,
        endowment: U256,
        address: Address,
        revert_data_len: usize,
    },
    Create2 {
        code: Box<[u8]>,
        endowment: U256,
        salt: B256,
        address: Address,
        revert_data_len: usize,
    },
    EmitLog {
        data: Box<[u8]>,
        topics: usize,
    },
    ReadReturnData {
        offset: u32,
        size: u32,
        data: Box<[u8]>,
    },
    ReturnDataSize {
        size: usize,
    },
}

#[derive(Debug)]
pub struct FrameReader {
    frame: TraceFrame,
    steps: VecDeque<Hostio>,
}

impl FrameReader {
    fn next(&mut self) -> Result<Hostio> {
        match self.steps.pop_front() {
            Some(item) => Ok(item),
            None => bail!("No next hostio"),
        }
    }

    pub fn next_hostio(&mut self, expected: &'static str) -> Hostio {
        fn detected(reader: &FrameReader, expected: &'static str) {
            let expected = expected.red();
            let which = match reader.frame.address {
                Some(call) => format!("call to {}", call.red()),
                None => "contract deployment".to_string(),
            };
            println!("{}", "\n════════ Divergence ════════".red());
            println!("Divegence detected while simulating a {which} via local assembly.");
            println!("The simulated environment expected a call to the {expected} Host I/O.",);
        }

        loop {
            let Ok(hostio) = self.next() else {
                detected(self, expected);
                println!("However, no such call is made onchain. Are you sure this the right contract?\n");
                panic!();
            };

            if hostio.kind.name() == expected {
                return hostio;
            }

            let kind = hostio.kind;
            let name = kind.name();
            match name {
                "memory_grow" | "user_entrypoint" | "user_returned" => continue,
                _ => {
                    detected(self, expected);
                    println!("However, onchain there's a call to {name}. Are you sure this the right contract?\n");
                    println!("expected: {}", expected.red());
                    println!("but have: {}\n", kind.debug_red());
                    panic!();
                }
            }
        }
    }
}
