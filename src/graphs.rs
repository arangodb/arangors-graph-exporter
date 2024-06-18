use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use anyhow::{anyhow, Result};

#[derive(Eq, Hash, PartialEq, Clone, Copy, Ord, PartialOrd, Debug)]
pub struct VertexHash(u64);

#[derive(Eq, PartialEq, Clone, Copy, Ord, PartialOrd, Debug)]
pub struct VertexIndex(u64);

#[derive(Debug)]
pub struct Edge {
    pub from: VertexIndex, // index of vertex
    pub to: VertexIndex,   // index of vertex
}

#[derive(Debug)]
pub struct Graph {
    // Index in list of graphs:
    pub graph_id: u64,

    // key is the hash of the vertex, value is the index, high bit
    // indicates a collision
    pub hash_to_index: HashMap<VertexHash, VertexIndex>,

    // key is the key of the vertex, value is the exceptional hash
    pub exceptions: HashMap<Vec<u8>, VertexHash>,

    // Maps indices of vertices to their names, not necessarily used:
    pub index_to_key: Vec<Vec<u8>>,

    // JSON data for vertices. If all data was empty, it is allowed that
    // the following vector is empty:
    pub vertex_json: Vec<Value>,

    // Additional data for edges. If all data was empty, it is allowed that
    // both of these are empty! After sealing, the offsets get one more
    // entry to mark the end of the last one:
    pub edge_data: Vec<u8>,
    pub edge_data_offsets: Vec<u64>,

    // Maps indices of vertices to offsets in edges by from:
    pub edge_index_by_from: Vec<u64>,

    // Edge index by from:
    pub edges_by_from: Vec<VertexIndex>,

    // Maps indices of vertices to offsets in edge index by to:
    pub edge_index_by_to: Vec<u64>,

    // Edge index by to:
    pub edges_by_to: Vec<VertexIndex>,

    // store keys?
    pub store_keys: bool,

    // sealed?
    pub vertices_sealed: bool,
    pub edges_sealed: bool,

    // Flag, if edges are already indexed:
    pub edges_indexed_from: bool,
    pub edges_indexed_to: bool,

    pub cols_to_keys_to_inds: HashMap<String, HashMap<String, usize>>,
    pub cols_to_inds_to_keys: HashMap<String, HashMap<usize, String>>,
    pub coo_by_from_edge_to: HashMap<(String, String, String), Vec<Vec<usize>>>,
    pub cols_to_features: HashMap<String, HashMap<String, Vec<Vec<f64>>>>,
}

impl Graph {
    pub fn new(store_keys: bool, _bits_for_hash: u8, id: u64) -> Arc<RwLock<Graph>> {
        Arc::new(RwLock::new(Graph {
            graph_id: id,
            hash_to_index: HashMap::new(),
            exceptions: HashMap::new(),
            index_to_key: vec![],
            vertex_json: vec![],
            edge_data: vec![],
            edge_data_offsets: vec![],
            edges_by_from: vec![],
            edge_index_by_from: vec![],
            edges_by_to: vec![],
            edge_index_by_to: vec![],
            store_keys,
            vertices_sealed: false,
            edges_sealed: false,
            edges_indexed_from: false,
            edges_indexed_to: false,
            cols_to_features: HashMap::new(),
            cols_to_keys_to_inds: HashMap::new(),
            cols_to_inds_to_keys: HashMap::new(),
            coo_by_from_edge_to: HashMap::new(),
        }))
    }

    pub fn insert_vertex(
        &mut self,
        key: Vec<u8>, // cannot be empty
        json: Option<Value>,
        collection_name: Vec<u8>,
        field_names: &[String],
    ) {
        let col_name = String::from_utf8(collection_name).unwrap();

        let feature_res = match json {
            None => {
                // We only add things here lazily as soon as some non-empty
                // data has been detected to save memory:
                if field_names.is_empty() {
                    Ok(HashMap::new())
                } else {
                    Err(())
                }
            }
            Some(j) => {
                // now parse the data
                let data = j.as_object();
                match data {
                    Some(data) => {
                        let features_in_json = field_names
                            .iter()
                            .filter(|name| data.contains_key(*name))
                            .collect::<Vec<&String>>();
                        if features_in_json.len() != field_names.len() {
                            Err(())
                        } else {
                            let data_map = data
                                .iter()
                                .filter_map(|(feature_name, val)|
                                    // now try to parse the value
                                    parse_value_to_vec(val).map(|v| (feature_name.clone(), v)))
                                .collect();
                            Ok(data_map)
                        }
                    }
                    None => {
                        if field_names.len() == 1 {
                            let feature_name = &field_names[0];
                            match parse_value_to_vec(&j) {
                                Some(value_vec) => {
                                    Ok(HashMap::from([(feature_name.clone(), value_vec)]))
                                }
                                None => Err(()),
                            }
                        } else {
                            Err(())
                        }
                    }
                }
            }
        };

        if let Ok(feature_map) = feature_res {
            // insert the vertex
            if !self.cols_to_keys_to_inds.contains_key(&col_name) {
                self.cols_to_keys_to_inds
                    .insert(col_name.clone(), HashMap::new());
            }

            if !self.cols_to_inds_to_keys.contains_key(&col_name) {
                self.cols_to_inds_to_keys
                    .insert(col_name.clone(), HashMap::new());
            }

            let keys_to_inds: &mut HashMap<String, usize> =
                self.cols_to_keys_to_inds.get_mut(&col_name).unwrap();
            let inds_to_keys: &mut HashMap<usize, String> =
                self.cols_to_inds_to_keys.get_mut(&col_name).unwrap();

            let cur_ind = keys_to_inds.len();
            let cur_key_str = String::from_utf8(key.clone()).unwrap();

            keys_to_inds.insert(cur_key_str.clone(), cur_ind);
            inds_to_keys.insert(cur_ind, cur_key_str);

            if !self.cols_to_features.contains_key(&col_name) {
                self.cols_to_features
                    .insert(col_name.clone(), HashMap::new());
            }
            let current_col_to_feats = self
                .cols_to_features
                .get_mut(&col_name)
                .expect("Unable to get col");

            for (feature_name, feature_vec) in feature_map {
                if !current_col_to_feats.contains_key(&feature_name) {
                    current_col_to_feats.insert(feature_name.clone(), vec![]);
                }
                current_col_to_feats
                    .get_mut(&feature_name)
                    .unwrap()
                    .append(&mut vec![feature_vec]);
            }
        }
    }

    pub fn insert_edge(
        &mut self,
        col_name: Vec<u8>,
        from_id: Vec<u8>,
        to_id: Vec<u8>,
        _data: Vec<u8>,
    ) -> Result<()> {
        let (from_col, from_key) = {
            let s = String::from_utf8(from_id.clone()).expect("_from to be a string");
            let id_split = s.find('/').expect("Invalid format for _from");
            let (col, key) = s.split_at(id_split);
            (col.to_string(), key[1..].to_string())
        };

        let (to_col, to_key) = {
            let s = String::from_utf8(to_id.clone()).expect("_to to be a string");
            let id_split = s.find('/').expect("Invalid format for _to");
            let (col, key) = s.split_at(id_split);
            (col.to_string(), key[1..].to_string())
        };

        let key_tup = (
            String::from_utf8(col_name).unwrap(),
            from_col.clone(),
            to_col.clone(),
        );
        if !self.coo_by_from_edge_to.contains_key(&key_tup) {
            self.coo_by_from_edge_to
                .insert(key_tup.clone(), vec![vec![], vec![]]);
        }
        let from_col_keys = self
            .cols_to_keys_to_inds
            .get(&from_col)
            .ok_or_else(|| anyhow!("Unable to get keys from for {:?}", &from_col))?;
        let to_col_keys = self
            .cols_to_keys_to_inds
            .get(&to_col)
            .ok_or_else(|| anyhow!("Unable to get keys to for {:?}", &to_col))?;
        let cur_coo = self
            .coo_by_from_edge_to
            .get_mut(&key_tup)
            .ok_or_else(|| anyhow!("Unable to get COO from to for {:?}", &key_tup))?;
        let from_col_id = from_col_keys.get(&from_key);
        let to_col_id = to_col_keys.get(&to_key);
        if let (Some(from_id), Some(to_id)) = (from_col_id, to_col_id) {
            cur_coo[0].push(*from_id);
            cur_coo[1].push(*to_id);
        };
        Ok(())
    }
}

fn parse_value_to_vec(val: &Value) -> Option<Vec<f64>> {
    // first try array
    match val.as_array() {
        Some(v) => {
            let float_casted: Vec<f64> = v.iter().filter_map(|v| v.as_f64()).collect();
            if float_casted.len() != v.len() {
                None
            } else {
                Some(float_casted)
            }
        }
        None => val.as_f64().map(|only_val| vec![only_val]),
    }
}
