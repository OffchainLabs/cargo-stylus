use alloy_json_abi::{Function, JsonAbi, StateMutability};
use eyre::bail;
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::io::BufReader;
use tiny_keccak::{Hasher, Keccak};

fn c_bytearray_initializer(val: &[u8]) -> String {
    let slot_strings: Vec<String> = val
        .iter()
        .map(|input| -> String { format!("0x{:02x}", input) })
        .collect();
    format!("{{{}}}", slot_strings.join(", "))
}

pub fn c_gen(in_path: String, out_path: String) -> eyre::Result<()> {
    let f = fs::File::open(&in_path)?;

    let input: Value = serde_json::from_reader(BufReader::new(f))?;

    let Some(input_contracts) = input["contracts"].as_object() else {
        bail!("did not find top-level contracts object in {}", in_path)
    };

    let mut pathbuf = std::path::PathBuf::new();
    pathbuf.push(out_path);
    for (solidity_file_name, solidity_file_out) in input_contracts.iter() {
        let debug_path = vec![solidity_file_name.as_str()];
        let Some(contracts) = solidity_file_out.as_object() else {
            println!("skipping output for {:?} not an object..", &debug_path);
            continue;
        };
        pathbuf.push(solidity_file_name);
        fs::create_dir_all(&pathbuf)?;

        for (contract_name, contract_val) in contracts.iter() {
            let mut debug_path = debug_path.clone();
            debug_path.push(contract_name);

            let Some(properties) = contract_val.as_object() else {
                println!("skipping output for {:?} not an object..", &debug_path);
                continue;
            };

            let mut methods: HashMap<String, Vec<Function>> = HashMap::default();

            if let Some(raw) = properties.get("abi") {
                // Sadly, JsonAbi = serde_json::from_value is not supported.
                // Tonight, we hack!
                let abi_json = serde_json::to_string(raw)?;
                let abi: JsonAbi = serde_json::from_str(&abi_json)?;
                for function in abi.functions() {
                    let name = function.name.clone();
                    methods
                        .entry(name)
                        .or_insert(Vec::default())
                        .push(function.clone());
                }
            } else {
                println!("skipping abi for {:?}: not found", &debug_path);
            }

            let mut header_body = String::default();
            let mut router_body = String::default();

            for (simple_name, mut overloads) in methods {
                overloads.sort_by_key(|a| a.signature());

                for (index, overload) in overloads.iter().enumerate() {
                    let c_name = match index {
                        0 => simple_name.clone(),
                        x => format!("{}_{}", simple_name, x),
                    };
                    let selector = u32::from_be_bytes(overload.selector());
                    let (hdr_params, call_params, payable) = match overload.state_mutability {
                        StateMutability::Pure => {
                            ("(uint8_t *input, size_t len)", "(input, len)", false)
                        }
                        StateMutability::View => (
                            "(const void *storage, uint8_t *input, size_t len)",
                            "(NULL, input, len)",
                            false,
                        ),
                        StateMutability::NonPayable => (
                            "(void *storage, uint8_t *input, size_t len)",
                            "(NULL, input, len)",
                            false,
                        ),
                        StateMutability::Payable => (
                            "(void *storage, uint8_t *input, size_t len, bebi32 value)",
                            "(NULL, input, len, value)",
                            true,
                        ),
                    };

                    let sig = &overload.signature();

                    header_body +=
                        &format!("#define SELECTOR_{c_name} 0x{selector:08x} // {sig}\n");
                    header_body += &format!("ArbResult {c_name}{hdr_params}; // {sig}\n");
                    router_body += &format!("    if (selector==SELECTOR_{c_name}) {{\n");
                    if !payable {
                        router_body += "        if (!bebi32_is_0(value)) revert();\n";
                    }
                    router_body += &format!("        return {c_name}{call_params};\n    }}\n");
                }
            }

            if !header_body.is_empty() {
                header_body.push('\n');
            }
            debug_path.push("storageLayout");
            if let Some(Value::Object(layout_vals)) = properties.get("storageLayout") {
                debug_path.push("storage");
                if let Some(Value::Array(storage_arr)) = layout_vals.get("storage") {
                    for storage_val in storage_arr.iter() {
                        let Some(storage_obj) = storage_val.as_object() else {
                            println!("skipping output inside {:?}: not an object..", &debug_path);
                            continue;
                        };
                        let Some(Value::String(label)) = storage_obj.get("label") else {
                            println!("skipping output inside {:?}: no label..", &debug_path);
                            continue;
                        };
                        let Some(Value::String(slot)) = storage_obj.get("slot") else {
                            println!("skipping output inside {:?}: no slot..", &debug_path);
                            continue;
                        };
                        let Ok(slot) = slot.parse::<u64>() else {
                            println!("skipping output inside {:?}: slot not u64 ..", &debug_path);
                            continue;
                        };
                        let Some(Value::String(val_type)) = storage_obj.get("type") else {
                            println!("skipping output inside {:?}: no type..", &debug_path);
                            continue;
                        };
                        let Some(Value::Number(read_offset)) = storage_obj.get("offset") else {
                            println!("skipping output inside {:?}: no offset..", &debug_path);
                            continue;
                        };
                        let offset = match read_offset.as_i64() {
                            None => {
                                println!(
                                    "skipping output inside {debug_path:?}: unexpected offset..",
                                );
                                continue;
                            }
                            Some(num) => {
                                if !(0..=32).contains(&num) {
                                    println!(
                                        "skipping output inside {debug_path:?}: unexpected offset..",
                                    );
                                    continue;
                                };
                                32 - num
                            }
                        };
                        let mut slot_buf = vec![0u8; 32 - 8];
                        slot_buf.extend(slot.to_be_bytes());

                        header_body += &format!(
                            "#define STORAGE_SLOT_{label} {} // {val_type}\n",
                            c_bytearray_initializer(&slot_buf),
                        );
                        if val_type.starts_with("t_array(") {
                            if val_type.ends_with(")dyn_storage") {
                                let mut keccak = Keccak::v256();
                                keccak.update(&slot_buf);
                                keccak.finalize(&mut slot_buf);
                                header_body += &format!(
                                    "#define STORAGE_BASE_{label} {} // {val_type}\n",
                                    c_bytearray_initializer(&slot_buf),
                                );
                            }
                        } else if !val_type.starts_with("t_mapping") {
                            header_body += &format!(
                                "#define STORAGE_END_OFFSET_{label} {offset} // {val_type}\n",
                            );
                        }
                    }
                } else {
                    println!("skipping output for {debug_path:?}: not an array..");
                }
                debug_path.pop();
            } else {
                println!("skipping output for {:?}: not an object..", &debug_path);
            }
            debug_path.pop();
            if !header_body.is_empty() {
                let mut unique_identifier = String::from("__");
                unique_identifier += &solidity_file_name.to_uppercase();
                unique_identifier += "_";
                unique_identifier += &contract_name.to_uppercase();
                unique_identifier += "_";

                let contents = format!(
                    r#" // autogenerated by cargo-stylus
#ifndef {uniq}
#define {uniq}

#include <stylus_types.h>
#include <bebi.h>

#ifdef __cplusplus
extern "C" {{
#endif

ArbResult default_func(void *storage, uint8_t *input, size_t len, bebi32 value);

{body}

#ifdef __cplusplus
}}
#endif

#endif // {uniq}
"#,
                    uniq = unique_identifier,
                    body = header_body
                );

                let filename: String = contract_name.into();
                pathbuf.push(filename + ".h");
                fs::write(&pathbuf, &contents)?;
                pathbuf.pop();
            }
            if !router_body.is_empty() {
                let contents = format!(
                    r#" // autogenerated by cargo-stylus

#include "{contract}.h"
#include <stylus_types.h>
#include <stylus_entry.h>
#include <bebi.h>


ArbResult {contract}_entry(uint8_t *input, size_t len) {{
    bebi32 value;
    msg_value(value);
    if (len < 4) {{
        return default_func(NULL, input, len, value);
    }}
    uint32_t selector = bebi_get_u32(input, 0);
    input +=4;
    len -=4;
{body}
    input -=4;
    len +=4;
    return default_func(NULL, input, len, value);
}}

ENTRYPOINT({contract}_entry)
"#,
                    contract = contract_name,
                    body = router_body
                );

                let filename: String = contract_name.into();
                pathbuf.push(filename + "_main.c");
                fs::write(&pathbuf, &contents)?;
                pathbuf.pop();
            }
        }
        pathbuf.pop();
    }
    Ok(())
}
