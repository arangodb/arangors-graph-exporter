use crate::graph_store::graph::{Edge, Graph};
use crate::load::load_strategy::LoadStrategy;
use bytes::Bytes;
use log::{debug, info};
use serde::{Deserialize, Serialize};
use serde_json::{Value};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex, RwLock};
use std::time::SystemTime;
use crate::arangodb::dump::{CollectionsInfo, generate_fields_map};

#[derive(Debug, Serialize, Deserialize)]
struct CursorResult {
    result: Vec<Value>,
}

fn collection_name_from_id(id: &str) -> String {
    let pos = id.find('/');
    match pos {
        None => "".to_string(),
        Some(p) => id[0..p].to_string(),
    }
}

pub fn receive_edges(
    receiver: Receiver<Bytes>,
    graph_clone: Arc<RwLock<Graph>>,
    load_strategy: LoadStrategy,
    prog_reported_clone: Arc<Mutex<u64>>,
    begin: SystemTime,
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
        let mut edges: Vec<Edge> = Vec::with_capacity(froms.len());
        {
            // First translate keys to indexes by reading
            // the graph object:
            let graph = graph_clone.read().unwrap();
            assert!(froms.len() == tos.len());
            for i in 0..froms.len() {
                match graph.get_new_edge_between_vertices(&froms[i], &tos[i]) {
                    Ok(edge) => edges.push(edge),
                    Err(message) => eprintln!("{}", message),
                }
            }
        }
        let nr_edges: u64;
        {
            // Now actually insert edges by writing the graph
            // object:
            let mut graph = graph_clone.write().unwrap();
            edges
                .into_iter()
                .for_each(|e| graph.insert_edge_unchecked(e));
            nr_edges = graph.number_of_edges();
            //for e in edges {
            //    // don't need to worry about this error for now
            //   let _ = graph.insert_edge(e.0, e.1, e.2, vec![]);
            //}
            //nr_edges = graph.number_of_edges();
        }
        let mut prog = prog_reported_clone.lock().unwrap();
        if nr_edges > *prog + 1000000_u64 {
            *prog = nr_edges;
            info!(
                            "{:?} Have imported {} edges.",
                            std::time::SystemTime::now().duration_since(begin).unwrap(),
                            *prog
                        );
        }
    }
    Ok(())
}

pub fn receive_vertices(
    receiver: Receiver<Bytes>,
    graph_clone: Arc<RwLock<Graph>>,
    collections_info: CollectionsInfo,
    load_strategy: LoadStrategy,
    prog_reported_clone: Arc<Mutex<u64>>,
    begin: SystemTime,
) -> Result<(), String> {
    // colection name -> fields
    let vcf_map = generate_fields_map(&collections_info);
    while let Ok(resp) = receiver.recv() {
        let body = std::str::from_utf8(resp.as_ref())
            .map_err(|e| format!("UTF8 error when parsing body: {:?}", e))?;
        debug!(
            "{:?} Received post response, body size: {}",
            std::time::SystemTime::now().duration_since(begin),
            body.len()
        );
        let mut vertex_keys: Vec<Vec<u8>> = Vec::with_capacity(400000);
        let mut current_vertex_col: Option<Vec<u8>> = None;
        let mut vertex_json: Vec<Vec<Value>> = vec![];
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
                let idstr: &String = match id {
                    Value::String(i) => {
                        let mut buf = vec![];
                        buf.extend_from_slice(i[..].as_bytes());
                        vertex_keys.push(buf);
                        i
                    }
                    _ => {
                        return Err(format!(
                            "JSON is no object with a string _id attribute:\n{}",
                            line
                        ));
                    }
                };
                // If we get here, we have to extract the field
                // values in `fields` from the json and store it
                // to vertex_json:
                let get_value = |v: &Value, field: &str| -> Value {
                    if field == "@collection_name" {
                        Value::String(collection_name_from_id(idstr))
                    } else {
                        v[field].clone()
                    }
                };

                let mut cols: Vec<Value> = Vec::with_capacity(fields.len());
                for f in fields.iter() {
                    let j = get_value(&v, f);
                    cols.push(j);
                }
                vertex_json.push(cols);
            }
        } else {
            // load_strategy == LoadStrategy::Aql
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
                let idstr: &String = match id {
                    Value::String(i) => {
                        let mut buf = vec![];
                        buf.extend_from_slice(i[..].as_bytes());
                        vertex_keys.push(buf);
                        i
                    }
                    _ => {
                        return Err(format!(
                            "JSON is no object with a string _id attribute:\n{}",
                            v
                        ));
                    }
                };
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
                let get_value = |v: &Value, field: &str| -> Value {
                    if field == "@collection_name" {
                        Value::String(collection_name_from_id(idstr))
                    } else {
                        v[field].clone()
                    }
                };

                let mut cols: Vec<Value> = Vec::with_capacity(fields.len());
                for f in fields.iter() {
                    let j = get_value(&v, f);
                    cols.push(j);
                }
                vertex_json.push(cols);
            }
        }
        let nr_vertices: u64;
        {
            let mut graph = graph_clone.write().unwrap();
            for i in 0..vertex_keys.len() {
                let k = &vertex_keys[i];
                let mut cols: Vec<Value> = vec![];
                std::mem::swap(&mut cols, &mut vertex_json[i]);
                graph.insert_vertex(k.clone(), cols);
            }
            nr_vertices = graph.number_of_vertices();
        }
        let mut prog = prog_reported_clone.lock().unwrap();
        if nr_vertices > *prog + 1000000_u64 {
            *prog = nr_vertices;
            info!("{:?} Have imported {} vertices.",
                            std::time::SystemTime::now().duration_since(begin).unwrap(),
                            *prog
                        );
        }
    }
    Ok(())
}
