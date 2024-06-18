use crate::graphs::Graph;
use crate::load::load_strategy::LoadStrategy;
use bytes::Bytes;
use log::debug;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::mpsc::Receiver;
use std::sync::{Arc, RwLock};

#[derive(Debug, Serialize, Deserialize)]
struct CursorResult {
    result: Vec<Value>,
}

pub fn receive_edges(
    receiver: Receiver<Bytes>,
    graph_clone: Arc<RwLock<Graph>>,
    load_strategy: LoadStrategy,
) -> Result<(), String> {
    while let Ok(resp) = receiver.recv() {
        let body = std::str::from_utf8(resp.as_ref())
            .map_err(|e| format!("UTF8 error when parsing body: {:?}", e))?;
        let mut froms: Vec<Vec<u8>> = Vec::with_capacity(1000000);
        let mut tos: Vec<Vec<u8>> = Vec::with_capacity(1000000);
        let mut current_col_name: Option<Vec<u8>> = None;
        if load_strategy == LoadStrategy::Dump {
            for line in body.lines() {
                let v: Value = match serde_json::from_str(line) {
                    Err(err) => {
                        return Err(format!(
                            "Error parsing document for line:\n{}\n{:?}",
                            line, err
                        ));
                    }
                    Ok(val) => val,
                };
                let from = &v["_from"];
                match from {
                    Value::String(i) => {
                        let mut buf = vec![];
                        buf.extend_from_slice(i[..].as_bytes());
                        froms.push(buf);
                    }
                    _ => {
                        return Err(format!(
                            "JSON is no object with a string _from attribute:\n{}",
                            line
                        ));
                    }
                }
                let to = &v["_to"];
                match to {
                    Value::String(i) => {
                        let mut buf = vec![];
                        buf.extend_from_slice(i[..].as_bytes());
                        tos.push(buf);
                    }
                    _ => {
                        return Err(format!(
                            "JSON is no object with a string _from attribute:\n{}",
                            line
                        ));
                    }
                }
                if current_col_name.is_none() {
                    let id = &v["_id"];
                    match id {
                        Value::String(i) => {
                            let pos = i.find('/').unwrap();
                            current_col_name = Some((&i[0..pos]).into());
                        }
                        _ => {
                            return Err("JSON _id is not string attribute".to_string());
                        }
                    }
                };
            }
        } else {
            let values = match serde_json::from_str::<CursorResult>(body) {
                Err(err) => {
                    return Err(format!(
                        "Error parsing document for body:\n{}\n{:?}",
                        body, err
                    ));
                }
                Ok(val) => val,
            };
            for v in values.result.into_iter() {
                let from = &v["_from"];
                match from {
                    Value::String(i) => {
                        let mut buf = vec![];
                        buf.extend_from_slice(i[..].as_bytes());
                        froms.push(buf);
                    }
                    _ => {
                        return Err(format!(
                            "JSON is no object with a string _from attribute:\n{}",
                            v
                        ));
                    }
                }
                let to = &v["_to"];
                match to {
                    Value::String(i) => {
                        let mut buf = vec![];
                        buf.extend_from_slice(i[..].as_bytes());
                        tos.push(buf);
                    }
                    _ => {
                        return Err(format!(
                            "JSON is no object with a string _from attribute:\n{}",
                            v
                        ));
                    }
                }
                if current_col_name.is_none() {
                    let id = &v["_id"];
                    match id {
                        Value::String(i) => {
                            let pos = i.find('/').unwrap();
                            current_col_name = Some((&i[0..pos]).into());
                        }
                        _ => {
                            return Err("JSON _id is not string attribute".to_string());
                        }
                    }
                };
            }
        }
        let mut edges: Vec<(Vec<u8>, Vec<u8>, Vec<u8>)> = Vec::with_capacity(froms.len());
        // First translate keys to indexes by reading
        // the graph object:
        assert!(froms.len() == tos.len());
        for i in 0..froms.len() {
            let from_key = &froms[i];
            let to_key = &tos[i];
            edges.push((
                current_col_name.clone().unwrap(),
                from_key.clone(),
                to_key.clone(),
            ));
        }
        {
            // Now actually insert edges by writing the graph
            // object:
            let mut graph = graph_clone.write().unwrap();
            for e in edges {
                // don't need to worry about this error for now
                let _ = graph.insert_edge(e.0, e.1, e.2, vec![]);
            }
        }
    }
    Ok(())
}

pub fn receive_vertices(
    receiver: Receiver<Bytes>,
    graph_clone: Arc<RwLock<Graph>>,
    vertex_coll_field_map_clone: Arc<RwLock<HashMap<String, Vec<String>>>>,
    load_strategy: LoadStrategy,
) -> Result<(), String> {
    let vcf_map = vertex_coll_field_map_clone.read().unwrap();
    let begin = std::time::SystemTime::now();
    while let Ok(resp) = receiver.recv() {
        let body = std::str::from_utf8(resp.as_ref())
            .map_err(|e| format!("UTF8 error when parsing body: {:?}", e))?;
        debug!(
            "{:?} Received post response, body size: {}",
            std::time::SystemTime::now().duration_since(begin),
            body.len()
        );
        let mut vertex_keys: Vec<Vec<u8>> = vec![];
        let mut current_vertex_col: Option<Vec<u8>> = None;
        vertex_keys.reserve(400000);
        let mut vertex_json: Vec<Value> = vec![];
        let mut json_initialized = false;
        let mut fields: Vec<String> = vec![];
        if load_strategy == LoadStrategy::Dump {
            for line in body.lines() {
                let v: Value = match serde_json::from_str(line) {
                    Err(err) => {
                        return Err(format!(
                            "Error parsing document for line:\n{}\n{:?}",
                            line, err
                        ));
                    }
                    Ok(val) => val,
                };
                let id = &v["_id"];
                let key = &v["_key"];
                match id {
                    Value::String(i) => {
                        let mut buf = vec![];
                        buf.extend_from_slice(key.as_str().unwrap().as_bytes());
                        vertex_keys.push(buf);
                        if current_vertex_col.is_none() {
                            let pos = i.find('/').unwrap();
                            current_vertex_col = Some((&i[0..pos]).into());
                        }
                        if !json_initialized {
                            json_initialized = true;
                            let pos = i.find('/');
                            match pos {
                                None => {
                                    fields = vec![];
                                }
                                Some(p) => {
                                    let collname = i[0..p].to_string();
                                    let flds = vcf_map.get(&collname);
                                    match flds {
                                        None => {
                                            fields = vec![];
                                        }
                                        Some(v) => {
                                            fields = v.clone();
                                            vertex_json.reserve(400000);
                                        }
                                    }
                                }
                            }
                        }
                    }
                    _ => {
                        return Err(format!(
                            "JSON is no object with a string _id attribute:\n{}",
                            line
                        ));
                    }
                }
                // If we get here, we have to extract the field
                // values in `fields` from the json and store it
                // to vertex_json:
                if !fields.is_empty() {
                    if fields.len() == 1 {
                        vertex_json.push(v[&fields[0]].clone());
                    } else {
                        let mut j = json!({});
                        for f in fields.iter() {
                            j[&f] = v[&f].clone();
                        }
                        vertex_json.push(j);
                    }
                }
            }
        } else {
            let values = match serde_json::from_str::<CursorResult>(body) {
                Err(err) => {
                    return Err(format!(
                        "Error parsing document for body:\n{}\n{:?}",
                        body, err
                    ));
                }
                Ok(val) => val,
            };
            for v in values.result.into_iter() {
                let id = &v["_id"];
                let key = &v["_key"];
                match id {
                    Value::String(i) => {
                        let mut buf = vec![];
                        buf.extend_from_slice(key.as_str().unwrap().as_bytes());
                        vertex_keys.push(buf);
                        if current_vertex_col.is_none() {
                            let pos = i.find('/').unwrap();
                            current_vertex_col = Some((&i[0..pos]).into());
                        }
                        if !json_initialized {
                            json_initialized = true;
                            let pos = i.find('/');
                            match pos {
                                None => {
                                    fields = vec![];
                                }
                                Some(p) => {
                                    let collname = i[0..p].to_string();
                                    let flds = vcf_map.get(&collname);
                                    match flds {
                                        None => {
                                            fields = vec![];
                                        }
                                        Some(v) => {
                                            fields = v.clone();
                                            vertex_json.reserve(400000);
                                        }
                                    }
                                }
                            }
                        }
                    }
                    _ => {
                        return Err(format!(
                            "JSON is no object with a string _id attribute:\n{}",
                            v
                        ));
                    }
                }
                // If we get here, we have to extract the field
                // values in `fields` from the json and store it
                // to vertex_json:
                if !fields.is_empty() {
                    if fields.len() == 1 {
                        vertex_json.push(v[&fields[0]].clone());
                    } else {
                        let mut j = json!({});
                        for f in fields.iter() {
                            j[&f] = v[&f].clone();
                        }
                        vertex_json.push(j);
                    }
                }
            }
        }
        {
            let mut graph = graph_clone.write().unwrap();
            for i in 0..vertex_keys.len() {
                let k = &vertex_keys[i];
                graph.insert_vertex(
                    k.clone(),
                    if vertex_json.is_empty() {
                        None
                    } else {
                        Some(vertex_json[i].clone())
                    },
                    current_vertex_col.clone().unwrap(),
                    &fields,
                );
            }
        }
    }
    Ok(())
}
