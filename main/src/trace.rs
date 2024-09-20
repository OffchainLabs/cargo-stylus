// Copyright 2023, Offchain Labs, Inc.
// For licensing, see https://github.com/OffchainLabs/cargo-stylus/blob/stylus/licenses/COPYRIGHT.md

#![allow(clippy::redundant_closure_call)]

use crate::util::color::{Color, DebugColor};
use crate::SimulateArgs;
use alloy_primitives::{Address, TxHash, B256, U256};
use ethers::{
    providers::{JsonRpcClient, Middleware, Provider},
    types::{
        BlockId, GethDebugTracerType, GethDebugTracingCallOptions, GethDebugTracingOptions,
        GethTrace, Transaction, TransactionRequest,
    },
    utils::__serde_json::{from_value, Value},
};
use eyre::{bail, OptionExt, Result, WrapErr};
use serde::{Deserialize, Serialize};
use sneks::SimpleSnakeNames;
use std::{collections::VecDeque, mem};

#[derive(Debug)]
pub struct Trace {
    pub top_frame: TraceFrame,
    pub tx: Transaction,
    pub json: Value,
}

impl Trace {
    pub async fn new<T: JsonRpcClient>(
        provider: Provider<T>,
        tx: TxHash,
        use_native_tracer: bool,
    ) -> Result<Self> {
        let hash = tx.0.into();

        let Some(receipt) = provider.get_transaction_receipt(hash).await? else {
            bail!("failed to get receipt for tx: {}", hash)
        };
        let Some(tx) = provider.get_transaction(hash).await? else {
            bail!("failed to get tx data: {}", hash)
        };

        let query = if use_native_tracer {
            "stylusTracer"
        } else {
            include_str!("query.js")
        };
        let tracer = GethDebugTracingOptions {
            tracer: Some(GethDebugTracerType::JsTracer(query.to_owned())),
            ..GethDebugTracingOptions::default()
        };
        let GethTrace::Unknown(json) = provider.debug_trace_transaction(hash, tracer).await? else {
            bail!("malformed tracing result")
        };

        if let Value::Array(arr) = json.clone() {
            if arr.is_empty() {
                bail!("No trace frames found, perhaps you are attempting to trace the contract deployment transaction");
            }
        }

        let maybe_activation_trace: Result<Vec<ActivationTraceFrame>, _> = from_value(json.clone());
        if maybe_activation_trace.is_ok() {
            bail!("Your tx was a contract activation transaction. It has no trace frames");
        }

        let to = receipt.to.map(|x| Address::from(x.0));
        let top_frame = TraceFrame::parse_frame(to, json.clone())?;

        Ok(Self {
            top_frame,
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
    pub async fn simulate<T: JsonRpcClient>(
        provider: Provider<T>,
        args: &SimulateArgs,
    ) -> Result<Self> {
        // Build the transaction request
        let mut tx_request = TransactionRequest::new();

        if let Some(from) = args.from {
            tx_request = tx_request.from(from);
        }
        if let Some(to) = args.to {
            tx_request = tx_request.to(to);
        }
        if let Some(gas) = args.gas {
            tx_request = tx_request.gas(gas);
        }
        if let Some(gas_price) = args.gas_price {
            tx_request = tx_request.gas_price(gas_price);
        }
        if let Some(value) = args.value {
            tx_request = tx_request.value(value);
        }
        if let Some(data) = &args.data {
            tx_request = tx_request.data(data.clone());
        }

        // Use the same tracer as in Trace::new
        let query = if args.use_native_tracer {
            "stylusTracer"
        } else {
            include_str!("query.js")
        };

        // Corrected construction of tracer_options
        let tracer_options = GethDebugTracingCallOptions {
            tracing_options: GethDebugTracingOptions {
                tracer: Some(GethDebugTracerType::JsTracer(query.to_owned())),
                ..Default::default()
            },
            ..Default::default()
        };

        // Use the latest block; alternatively, this can be made configurable
        let block_id = None::<BlockId>;

        let GethTrace::Unknown(json) = provider
            .debug_trace_call(tx_request, block_id, tracer_options)
            .await?
        else {
            bail!("Malformed tracing result");
        };

        if let Value::Array(arr) = json.clone() {
            if arr.is_empty() {
                bail!("No trace frames found.");
            }
        }
        // Since this is a simulated transaction, we create a dummy Transaction object
        let tx = Transaction {
            from: args.from.unwrap_or_default(),
            to: args.to,
            gas: args
                .gas
                .map(|gas| {
                    let bytes = [0u8; 32]; // U256 in both libraries is 32 bytes
                    gas.to_be_bytes().copy_from_slice(&bytes[..8]); // Convert alloy_primitives::U256 to bytes
                    ethers::types::U256::from_big_endian(&bytes) // Convert bytes to ethers::types::U256
                })
                .unwrap_or_else(|| ethers::types::U256::zero()), // Default to 0 if no gas is provided
            gas_price: args.gas_price,
            value: args.value.unwrap_or_else(|| ethers::types::U256::zero()),
            input: args.data.clone().unwrap_or_default().into(),
            // Default values for other fields
            ..Default::default()
        };

        // Parse the trace frames
        let top_frame = TraceFrame::parse_frame(None, json.clone())?;

        Ok(Self {
            top_frame,
            tx,
            json,
        })
    }
}

#[derive(Serialize, Deserialize)]
pub struct ActivationTraceFrame {
    address: Value,
}

#[derive(Clone, Debug, PartialEq, Eq)]
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
            macro_rules! get_hex {
                ($name:expr) => {{
                    let data = get_typed!(keys, String, $name);
                    let data = data
                        .strip_prefix("0x")
                        .ok_or_eyre(concat!($name, " does not contain 0x prefix"))?;
                    hex::decode(data)
                        .wrap_err(concat!("failed to parse ", $name))?
                        .into_boxed_slice()
                }};
            }

            let name = get_typed!(keys, String, "name");
            let mut args = get_hex!("args");
            let mut outs = get_hex!("outs");

            let start_ink = get_int!("startInk");
            let end_ink = get_int!("endInk");

            macro_rules! read_data {
                ($src:ident) => {{
                    let data = $src;
                    $src = Box::new([]);
                    data
                }};
            }
            macro_rules! read_ty {
                ($src:ident, $ty:ident, $conv:expr) => {{
                    let size = mem::size_of::<$ty>();
                    let len = $src.len();
                    if size > len {
                        bail!(
                            "parse {}: want {} bytes; got {}",
                            stringify!($src),
                            size,
                            len
                        );
                    }
                    let (left, right) = $src.split_at(size);
                    let result = $conv(left);
                    $src = right.to_vec().into_boxed_slice();
                    result
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

            macro_rules! frame {
                () => {{
                    let address = get_hex!("address");
                    let address = Address::from_slice(&address);
                    let steps = keys.remove("steps").unwrap();
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
                "storage_cache_bytes32" => StorageCacheBytes32 {
                    key: read_b256!(args),
                    value: read_b256!(args),
                },
                "storage_flush_cache" => StorageFlushCache {
                    clear: read_u8!(args),
                },
                "transient_load_bytes32" => TransientLoadBytes32 {
                    key: read_b256!(args),
                    value: read_b256!(outs),
                },
                "transient_store_bytes32" => TransientStoreBytes32 {
                    key: read_b256!(args),
                    value: read_b256!(args),
                },
                "account_balance" => AccountBalance {
                    address: read_address!(args),
                    balance: read_u256!(outs),
                },
                "account_code" => AccountCode {
                    address: read_address!(args),
                    offset: read_u32!(args),
                    size: read_u32!(args),
                    code: read_data!(outs),
                },
                "account_code_size" => AccountCodeSize {
                    address: read_address!(args),
                    size: read_u32!(outs),
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
                "math_div" => MathDiv {
                    a: read_u256!(args),
                    b: read_u256!(args),
                    result: read_u256!(outs),
                },
                "math_mod" => MathMod {
                    a: read_u256!(args),
                    b: read_u256!(args),
                    result: read_u256!(outs),
                },
                "math_pow" => MathPow {
                    a: read_u256!(args),
                    b: read_u256!(args),
                    result: read_u256!(outs),
                },
                "math_add_mod" => MathAddMod {
                    a: read_u256!(args),
                    b: read_u256!(args),
                    c: read_u256!(args),
                    result: read_u256!(outs),
                },
                "math_mul_mod" => MathMulMod {
                    a: read_u256!(args),
                    b: read_u256!(args),
                    c: read_u256!(args),
                    result: read_u256!(outs),
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
                    revert_data_len: read_u32!(outs),
                },
                "create2" => Create2 {
                    endowment: read_u256!(args),
                    salt: read_b256!(args),
                    code: read_data!(args),
                    address: read_address!(outs),
                    revert_data_len: read_u32!(outs),
                },
                "emit_log" => EmitLog {
                    topics: read_u32!(args),
                    data: read_data!(args),
                },
                "read_return_data" => ReadReturnData {
                    offset: read_u32!(args),
                    size: read_u32!(args),
                    data: read_data!(outs),
                },
                "return_data_size" => ReturnDataSize {
                    size: read_u32!(outs),
                },
                "console_log_text" => ConsoleLogText {
                    text: read_data!(args),
                },
                "console_log" => ConsoleLog {
                    text: read_string!(args),
                },
                x => {
                    if x.starts_with("evm_") {
                        EVMCall {
                            name: x.to_owned(),
                            frame: frame!(),
                        }
                    } else {
                        todo!("Missing hostio details {x}")
                    }
                }
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Hostio {
    pub kind: HostioKind,
    pub start_ink: u64,
    pub end_ink: u64,
}

#[allow(dead_code)]
#[derive(Clone, Debug, Eq, PartialEq, SimpleSnakeNames)]
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
    StorageCacheBytes32 {
        key: B256,
        value: B256,
    },
    StorageFlushCache {
        clear: u8,
    },
    TransientLoadBytes32 {
        key: B256,
        value: B256,
    },
    TransientStoreBytes32 {
        key: B256,
        value: B256,
    },
    AccountBalance {
        address: Address,
        balance: U256,
    },
    AccountCode {
        address: Address,
        offset: u32,
        size: u32,
        code: Box<[u8]>,
    },
    AccountCodeSize {
        address: Address,
        size: u32,
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
    MathDiv {
        a: U256,
        b: U256,
        result: U256,
    },
    MathMod {
        a: U256,
        b: U256,
        result: U256,
    },
    MathPow {
        a: U256,
        b: U256,
        result: U256,
    },
    MathAddMod {
        a: U256,
        b: U256,
        c: U256,
        result: U256,
    },
    MathMulMod {
        a: U256,
        b: U256,
        c: U256,
        result: U256,
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
        revert_data_len: u32,
    },
    Create2 {
        code: Box<[u8]>,
        endowment: U256,
        salt: B256,
        address: Address,
        revert_data_len: u32,
    },
    EmitLog {
        data: Box<[u8]>,
        topics: u32,
    },
    ReadReturnData {
        offset: u32,
        size: u32,
        data: Box<[u8]>,
    },
    ReturnDataSize {
        size: u32,
    },
    EVMCall {
        name: String,
        frame: TraceFrame,
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
                "pay_for_memory_grow" | "user_entrypoint" | "user_returned" => continue,
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

#[cfg(test)]
mod tests {
    use super::*;
    use alloy_primitives::{address, fixed_bytes};

    #[test]
    fn parse_simple() {
        let trace = r#"
        [
          {
            "name": "storage_load_bytes32",
            "args": "0xfafafafafafafafafafafafafafafafafafafafafafafafafafafafafafafafa",
            "outs": "0xebebebebebebebebebebebebebebebebebebebebebebebebebebebebebebebeb",
            "startInk": 1000,
            "endInk": 900 
          }
        ]"#;
        let json = serde_json::from_str(trace).expect("failed to parse json");
        let top_frame = TraceFrame::parse_frame(None, json).expect("failed to parse frame");
        assert_eq!(
            top_frame.steps,
            vec![Hostio {
                kind: HostioKind::StorageLoadBytes32 {
                    key: fixed_bytes!(
                        "fafafafafafafafafafafafafafafafafafafafafafafafafafafafafafafafa"
                    ),
                    value: fixed_bytes!(
                        "ebebebebebebebebebebebebebebebebebebebebebebebebebebebebebebebeb"
                    ),
                },
                start_ink: 1000,
                end_ink: 900,
            },]
        );
    }

    #[test]
    fn parse_call() {
        let trace = r#"
        [
          {
            "name": "call_contract",
            "args": "0xdeaddeaddeaddeaddeaddeaddeaddeaddeaddead00000000000000ff000000000000000000000000000000000000000000000000000000000000ffffbeef",
            "outs": "0x0000000f00",
            "startInk": 1000,
            "endInk": 500,
            "address": "0xdeaddeaddeaddeaddeaddeaddeaddeaddeaddead",
            "steps": [
              {
                "name": "user_entrypoint",
                "args": "0x00000001",
                "outs": "0x",
                "startInk": 900,
                "endInk": 600
              }
            ]
          }
        ]"#;
        let json = serde_json::from_str(trace).expect("failed to parse json");
        let top_frame = TraceFrame::parse_frame(None, json).expect("failed to parse frame");
        assert_eq!(
            top_frame.steps,
            vec![Hostio {
                kind: HostioKind::CallContract {
                    address: address!("deaddeaddeaddeaddeaddeaddeaddeaddeaddead"),
                    data: Box::new([0xbe, 0xef]),
                    gas: 255,
                    value: U256::from(65535),
                    outs_len: 15,
                    status: 0,
                    frame: TraceFrame {
                        steps: vec![Hostio {
                            kind: HostioKind::UserEntrypoint { args_len: 1 },
                            start_ink: 900,
                            end_ink: 600,
                        },],
                        address: Some(address!("deaddeaddeaddeaddeaddeaddeaddeaddeaddead"),),
                    },
                },
                start_ink: 1000,
                end_ink: 500,
            },],
        );
    }

    #[test]
    fn parse_evm_call() {
        let trace = r#"
        [
          {
            "name": "evm_call_contract",
            "args": "0x",
            "outs": "0x",
            "startInk": 0,
            "endInk": 0,
            "address": "0x457b1ba688e9854bdbed2f473f7510c476a3da09",
            "steps": [
              {
                "name": "user_entrypoint",
                "args": "0x00000001",
                "outs": "0x",
                "startInk": 0,
                "endInk": 0
              }
            ]
          }
        ]"#;
        let json = serde_json::from_str(trace).expect("failed to parse json");
        let top_frame = TraceFrame::parse_frame(None, json).expect("failed to parse frame");
        assert_eq!(
            top_frame.steps,
            vec![Hostio {
                kind: HostioKind::EVMCall {
                    name: String::from("evm_call_contract"),
                    frame: TraceFrame {
                        steps: vec![Hostio {
                            kind: HostioKind::UserEntrypoint { args_len: 1 },
                            start_ink: 0,
                            end_ink: 0,
                        },],
                        address: Some(address!("457b1ba688e9854bdbed2f473f7510c476a3da09")),
                    },
                },
                start_ink: 0,
                end_ink: 0,
            },],
        );
    }
}
