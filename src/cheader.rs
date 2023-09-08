use std::collections::HashMap;
use std::io::BufReader;
use std::fs;
use serde_json::Value;
use eyre::bail;

pub fn c_headers(in_path: String, out_path: String) ->eyre::Result<()> {
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
        pathbuf.push(&solidity_file_name);
        fs::create_dir_all(&pathbuf)?;
        let mut header_body = String::default();
        for (contract_name, contract_val) in contracts.iter() {
            let mut debug_path = debug_path.clone();
            debug_path.push(&contract_name);
            let Some(properties) = contract_val.as_object() else {
                println!("skipping output for {:?} not an object..", &debug_path);
                continue;
            };
            
            let mut methods :HashMap<&str, Vec<(&str, &str)>> = HashMap::default();

            debug_path.push("evm");
            if let Some(Value::Object(evm_vals)) = properties.get("evm") {
                debug_path.push("methodIdentifiers");
                if let Some(Value::Object(method_map)) = evm_vals.get("methodIdentifiers") {
                    for (method_name, method_val) in method_map.iter() {
                        let Some(method_val_str) = method_val.as_str() else {
                            println!("skipping output for {:?}/{} not a string..", &debug_path, method_name);
                            continue;
                        };
                        let Some(simple_name_length) = method_name.find('(') else {
                            println!("skipping output for {:?}/{} no \"(\"..", &debug_path, method_name);
                            continue;
                        };
                        let simple_name = &method_name[..simple_name_length];
                        methods.entry(simple_name).or_insert(Vec::default()).push((method_name.as_str(), method_val_str));
                    }
                } else {
                    println!("skipping output for {:?}: not an object..", &debug_path);
                }
                debug_path.pop();
            }
            debug_path.pop();

            for (simple_name, overloads) in methods {
                for (index, (full_name, identifier)) in overloads.iter().enumerate() {
                    let index_suffix = match index {
                        0 => String::default(),
                        x => format!("_{}",x),
                    };
                    header_body.push_str(format!("#define METHOD_{}{} 0x{} // {}\n", simple_name, index_suffix, identifier, full_name).as_str())
                }
            }

            if header_body.len() != 0 {
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
                        let Some(Value::Number(offset)) = storage_obj.get("offset") else {
                            println!("skipping output inside {:?}: no offset..", &debug_path);
                            continue;
                        };
                        header_body.push_str("#define STORAGE_SLOT_");
                        header_body.push_str(&label);
                        header_body.push(' ');
                        header_body.push_str(&slot);
                        header_body.push('\n');
                        header_body.push_str("#define STORAGE_OFFSET_");
                        header_body.push_str(&label);
                        header_body.push(' ');
                        header_body.push_str(offset.to_string().as_str());
                        header_body.push('\n');
                    }
                } else {
                    println!("skipping output for {:?}: not an array..", &debug_path);
                }
                debug_path.pop();
            } else {
                println!("skipping output for {:?}: not an object..", &debug_path);
            }
            debug_path.pop();
            if header_body.len() != 0 {
                let mut unique_identifier = String::from("__");
                unique_identifier.push_str(&solidity_file_name.to_uppercase());
                unique_identifier.push('_');
                unique_identifier.push_str(&contract_name.to_uppercase());
                unique_identifier.push('_');

                let contents = format!(r#" // autogenerated by cargo-stylus
#ifndef {uniq}
#define {uniq}

#ifdef __cplusplus
extern "C" {{
#endif

{body}

#ifdef __cplusplus
}}
#endif

#endif // {uniq}
"#, uniq=unique_identifier, body=header_body);

                let filename :String = contract_name.into();
                pathbuf.push(filename + ".h");
                fs::write(&pathbuf, &contents)?;
                pathbuf.pop();   
            }
        }
        pathbuf.pop();
    }
    Ok(())
}