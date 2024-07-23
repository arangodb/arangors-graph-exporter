use arangors::graph::{EdgeDefinition, Graph};
use arangors::Connection;
use lightning::{
    CollectionInfo, DataLoadConfiguration, DataLoadConfigurationBuilder, DatabaseConfiguration,
    DatabaseConfigurationBuilder, GraphLoader,
};
use serial_test::serial;

use arangors::connection::Version;
use arangors::document::options::InsertOptions;
use lightning::errors::GraphLoaderError;
use rstest::fixture;
use serde_json::Value;
use std::env;

static GRAPH: &str = "IntegrationTestGraph";
static EDGE_COLLECTION: &str = "IntegrationTestEdge";
static VERTEX_COLLECTION: &str = "IntegrationTestVertex";
static USERNAME: &str = "root";
static PASSWORD: &str = "test";
static DATABASE: &str = "_system";
static DATABASE_URL: &str = "http://localhost:8529";

fn get_db_url() -> String {
    env::var("ARANGODB_DB_URL").unwrap_or_else(|_| DATABASE_URL.to_string())
}

fn build_db_config() -> DatabaseConfiguration {
    let endpoints = vec![get_db_url()];
    let db_config_builder = DatabaseConfigurationBuilder::new()
        .endpoints(endpoints)
        .username(USERNAME.to_string())
        .password(PASSWORD.to_string())
        .database(DATABASE.to_string());
    db_config_builder.build()
}

fn build_load_config() -> DataLoadConfiguration {
    DataLoadConfigurationBuilder::new()
        .parallelism(8)
        .batch_size(100000)
        .build()
}

fn build_load_config_with_v_with_e(
    fetch_all_v_attributes: bool,
    fetch_all_e_attributes: bool,
) -> DataLoadConfiguration {
    DataLoadConfigurationBuilder::new()
        .parallelism(8)
        .batch_size(100000)
        .load_all_vertex_attributes(fetch_all_v_attributes)
        .load_all_edge_attributes(fetch_all_e_attributes)
        .build()
}

async fn is_cluster() -> bool {
    let conn = Connection::establish_basic_auth(DATABASE_URL, USERNAME, PASSWORD)
        .await
        .unwrap();
    let info = conn.into_admin().await;
    let role = info.unwrap().server_role().await.unwrap();
    if role == "COORDINATOR" {
        return true;
    }
    false
}

async fn get_arangodb_version() -> Version {
    let conn = Connection::establish_basic_auth(DATABASE_URL, USERNAME, PASSWORD)
        .await
        .unwrap();
    let db = conn.db(DATABASE).await.unwrap();
    db.arango_version().await.unwrap()
}

fn extract_version_parts(version: &str) -> Result<(u32, u32, u32), &'static str> {
    let version_part = version.split('-').next().ok_or("Invalid version string")?;
    let mut parts = version_part.split('.');

    let major = parts
        .next()
        .ok_or("Missing major version")?
        .parse()
        .map_err(|_| "Invalid major version")?;
    let minor = parts
        .next()
        .ok_or("Missing minor version")?
        .parse()
        .map_err(|_| "Invalid minor version")?;
    let patch = parts
        .next()
        .ok_or("Missing patch version")?
        .parse()
        .map_err(|_| "Invalid patch version")?;

    Ok((major, minor, patch))
}

async fn create_graph(insert_data: bool) {
    let conn = Connection::establish_basic_auth(DATABASE_URL, USERNAME, PASSWORD)
        .await
        .unwrap();

    let edge_definition = EdgeDefinition {
        collection: EDGE_COLLECTION.to_string(),
        from: vec![VERTEX_COLLECTION.to_string()],
        to: vec![VERTEX_COLLECTION.to_string()],
    };
    let graph = Graph::builder()
        .name(GRAPH.to_string())
        .edge_definitions(vec![edge_definition])
        .orphan_collections(vec![])
        .build();

    let db = conn.db(DATABASE).await.unwrap();
    let _ = db.drop_graph(GRAPH, true).await;
    let graph_res = db.create_graph(graph, false).await;
    assert!(graph_res.is_ok());

    if insert_data {
        // creates a "line" graph with 10 vertices and 9 edges
        // x,y,z attributes will be inserted to both vertex and edge collections
        let vertex_collection = db.collection(VERTEX_COLLECTION).await.unwrap();
        let edge_collection = db.collection(EDGE_COLLECTION).await.unwrap();

        let insert_options = InsertOptions::builder().overwrite(true).build();
        for i in 0..10 {
            let key_string = i.to_string();
            vertex_collection
                .create_document(
                    serde_json::json!({"_key": key_string, "x": i + 1, "y": i + 2, "z": i + 3}),
                    insert_options.clone(),
                )
                .await
                .unwrap();
        }

        for i in 0..9 {
            let from_id = VERTEX_COLLECTION.to_string() + "/" + &i.to_string();
            let to_id = VERTEX_COLLECTION.to_string() + "/" + &(i + 1).to_string();
            let key = i.to_string();
            edge_collection
                .create_document(
                    serde_json::json!({"_from": from_id, "_to": to_id, "_key": key, "x": i + 1, "y": i + 2, "z": i + 3}),
                    insert_options.clone(),
                )
                .await
                .unwrap();
        }
        let properties_v = vertex_collection.document_count().await.unwrap();
        assert_eq!(properties_v.info.count, Some(10));
        let properties_e = edge_collection.document_count().await.unwrap();
        assert_eq!(properties_e.info.count, Some(9));
    }
}

async fn drop_graph() {
    let conn = Connection::establish_basic_auth(DATABASE_URL, USERNAME, PASSWORD)
        .await
        .unwrap();

    let db = conn.db(DATABASE).await.unwrap();
    let _ = db.drop_graph(GRAPH, true).await;
}

async fn setup(insert_data: bool) {
    create_graph(insert_data).await;
}

async fn teardown() {
    drop_graph().await;
}

#[tokio::test]
#[serial]
async fn init_named_graph_loader() {
    setup(false).await;
    let db_config = build_db_config();
    let load_config = build_load_config();

    let graph_loader_res =
        GraphLoader::new_named(db_config, load_config, GRAPH.to_string(), None, None).await;
    if let Err(ref e) = graph_loader_res {
        println!("{:?}", e);
    }

    assert!(graph_loader_res.is_ok());
    teardown().await;
}

#[tokio::test]
#[serial]
async fn init_named_graph_loader_with_data() {
    setup(true).await;
    let db_config = build_db_config();
    let load_config = build_load_config();

    let graph_loader_res =
        GraphLoader::new_named(db_config, load_config, GRAPH.to_string(), None, None).await;
    if let Err(ref e) = graph_loader_res {
        println!("{:?}", e);
    }

    assert!(graph_loader_res.is_ok());

    // check that data is loaded.
    // in this case, no global vertex attributes have been requested
    // therefore only the _key attribute is loaded for vertex documents
    let graph_loader = graph_loader_res.unwrap();
    let handle_vertices = move |vertex_ids: &Vec<Vec<u8>>,
                                columns: &mut Vec<Vec<Value>>,
                                vertex_field_names: &Vec<String>| {
        assert_eq!(vertex_ids.len(), 10);

        assert_eq!(columns.len(), 10);
        for (v_index, vertex) in columns.iter().enumerate() {
            assert_eq!(vertex.len(), 1);
            assert_eq!(vertex.len(), vertex_field_names.len());
            let element_0 = &vertex[0];
            let id = element_0.as_str().unwrap();
            let expected_id = format!("{}/{}", VERTEX_COLLECTION, v_index);
            assert_eq!(id.to_string(), expected_id);
        }

        assert_eq!(vertex_field_names.len(), 1);
        assert_eq!(vertex_field_names[0], "_id");
        Ok(())
    };
    let vertices_result = graph_loader.do_vertices(handle_vertices).await;
    assert!(vertices_result.is_ok());

    let handle_edges = move |from_ids: &Vec<Vec<u8>>,
                             to_ids: &Vec<Vec<u8>>,
                             columns: &mut Vec<Vec<Value>>,
                             edge_field_names: &Vec<String>| {
        assert_eq!(from_ids.len(), 9);
        assert_eq!(from_ids.len(), to_ids.len());
        assert_eq!(columns.len(), 9);

        for (v_index, edge) in columns.iter().enumerate() {
            assert_eq!(edge.len(), 2);
            assert_eq!(edge.len(), edge_field_names.len());
            let element_0 = &edge[0];
            let from_id = element_0.as_str().unwrap();
            let to_id = &edge[1].as_str().unwrap();
            let expected_from_id = format!("{}/{}", VERTEX_COLLECTION, v_index);
            let expected_to_id = format!("{}/{}", VERTEX_COLLECTION, v_index + 1);
            assert_eq!(from_id.to_string(), expected_from_id);
            assert_eq!(to_id.to_string(), expected_to_id);
        }

        assert_eq!(edge_field_names.len(), 2);
        assert_eq!(edge_field_names[0], "_from");
        assert_eq!(edge_field_names[1], "_to");
        Ok(())
    };
    let edges_result = graph_loader.do_edges(handle_edges).await;
    assert!(edges_result.is_ok());

    teardown().await;
}

fn get_attribute_position(field_names: &Vec<String>, attribute: &str) -> usize {
    field_names.iter().position(|x| x == attribute).unwrap()
}

#[tokio::test]
#[serial]
async fn init_named_graph_loader_with_data_all_v_and_e_attributes_manually_set() {
    setup(true).await;
    let db_config = build_db_config();
    let load_config = build_load_config();
    let global_fields = vec!["x".to_string(), "y".to_string(), "z".to_string()];
    let graph_loader_res = GraphLoader::new_named(
        db_config,
        load_config,
        GRAPH.to_string(),
        Some(global_fields.clone()),
        Some(global_fields),
    )
    .await;
    if let Err(ref e) = graph_loader_res {
        println!("{:?}", e);
    }

    assert!(graph_loader_res.is_ok());

    // check that data is loaded.
    // in this case, no global vertex attributes have been requested
    // therefore only the _key attribute is loaded for vertex documents
    let graph_loader = graph_loader_res.unwrap();
    let handle_vertices = move |vertex_ids: &Vec<Vec<u8>>,
                                columns: &mut Vec<Vec<Value>>,
                                vertex_field_names: &Vec<String>| {
        assert_eq!(vertex_ids.len(), 10);
        assert_eq!(columns.len(), 10);

        for (v_index, v_id) in vertex_ids.iter().enumerate() {
            let id = v_id.iter().map(|x| *x as char).collect::<String>();
            let expected_id = format!("{}/{}", VERTEX_COLLECTION, v_index);
            assert_eq!(id, expected_id);
        }

        for (v_index, vertex) in columns.iter().enumerate() {
            assert_eq!(vertex.len(), 3);
            assert_eq!(vertex.len(), vertex_field_names.len());

            let x = &vertex[get_attribute_position(vertex_field_names, "x")]
                .as_u64()
                .unwrap();
            let y = &vertex[get_attribute_position(vertex_field_names, "y")]
                .as_u64()
                .unwrap();
            let z = &vertex[get_attribute_position(vertex_field_names, "z")]
                .as_u64()
                .unwrap();
            let expected_x_value = (v_index + 1) as u64;
            let expected_y_value = (v_index + 2) as u64;
            let expected_z_value = (v_index + 3) as u64;
            assert_eq!(x, &expected_x_value);
            assert_eq!(y, &expected_y_value);
            assert_eq!(z, &expected_z_value);
        }

        assert_eq!(vertex_field_names.len(), 3);
        assert!(vertex_field_names.contains(&"x".to_string()));
        assert!(vertex_field_names.contains(&"y".to_string()));
        assert!(vertex_field_names.contains(&"z".to_string()));
        Ok(())
    };
    let vertices_result = graph_loader.do_vertices(handle_vertices).await;
    assert!(vertices_result.is_ok());

    let handle_edges = move |from_ids: &Vec<Vec<u8>>,
                             to_ids: &Vec<Vec<u8>>,
                             columns: &mut Vec<Vec<Value>>,
                             edge_field_names: &Vec<String>| {
        assert_eq!(from_ids.len(), 9);
        assert_eq!(from_ids.len(), to_ids.len());
        assert_eq!(columns.len(), 9);

        for (e_index, from_id) in from_ids.iter().enumerate() {
            let from_id_str = from_id.iter().map(|x| *x as char).collect::<String>();
            let to_id_str = to_ids[e_index]
                .iter()
                .map(|x| *x as char)
                .collect::<String>();
            assert_eq!(from_id_str, format!("{}/{}", VERTEX_COLLECTION, e_index));
            assert_eq!(to_id_str, format!("{}/{}", VERTEX_COLLECTION, e_index + 1));
        }

        for (e_index, to_id) in to_ids.iter().enumerate() {
            let from_id_str = from_ids[e_index]
                .iter()
                .map(|x| *x as char)
                .collect::<String>();
            let to_id_str = to_id.iter().map(|x| *x as char).collect::<String>();
            assert_eq!(from_id_str, format!("{}/{}", VERTEX_COLLECTION, e_index));
            assert_eq!(to_id_str, format!("{}/{}", VERTEX_COLLECTION, e_index + 1));
        }

        for (e_index, edge) in columns.iter().enumerate() {
            assert_eq!(edge.len(), 3);
            assert_eq!(edge.len(), edge_field_names.len());

            let x = &edge[get_attribute_position(edge_field_names, "x")]
                .as_u64()
                .unwrap();
            let y = &edge[get_attribute_position(edge_field_names, "y")]
                .as_u64()
                .unwrap();
            let z = &edge[get_attribute_position(edge_field_names, "z")]
                .as_u64()
                .unwrap();
            let expected_x_value = (e_index + 1) as u64;
            let expected_y_value = (e_index + 2) as u64;
            let expected_z_value = (e_index + 3) as u64;
            assert_eq!(x, &expected_x_value);
            assert_eq!(y, &expected_y_value);
            assert_eq!(z, &expected_z_value);
        }

        assert_eq!(edge_field_names.len(), 3);
        assert!(edge_field_names.contains(&"x".to_string()));
        assert!(edge_field_names.contains(&"y".to_string()));
        assert!(edge_field_names.contains(&"z".to_string()));
        Ok(())
    };
    let edges_result = graph_loader.do_edges(handle_edges).await;
    assert!(edges_result.is_ok());

    teardown().await;
}

#[tokio::test]
#[serial]
async fn init_named_graph_loader_with_data_all_v_and_e_collection_name_attribute() {
    setup(true).await;
    let db_config = build_db_config();
    let load_config = build_load_config();
    let global_fields = vec!["@collection_name".to_string()];
    let graph_loader_res = GraphLoader::new_named(
        db_config,
        load_config,
        GRAPH.to_string(),
        Some(global_fields.clone()),
        Some(global_fields),
    )
    .await;
    if let Err(ref e) = graph_loader_res {
        println!("{:?}", e);
    }

    assert!(graph_loader_res.is_ok());

    // check that data is loaded.
    // in this case, no global vertex attributes have been requested
    // therefore only the _key attribute is loaded for vertex documents
    let graph_loader = graph_loader_res.unwrap();
    let handle_vertices = move |vertex_ids: &Vec<Vec<u8>>,
                                columns: &mut Vec<Vec<Value>>,
                                vertex_field_names: &Vec<String>| {
        assert_eq!(vertex_ids.len(), 10);
        assert_eq!(columns.len(), 10);

        for (v_index, v_id) in vertex_ids.iter().enumerate() {
            let id = v_id.iter().map(|x| *x as char).collect::<String>();
            let expected_id = format!("{}/{}", VERTEX_COLLECTION, v_index);
            assert_eq!(id, expected_id);
        }

        for (_v_index, vertex) in columns.iter().enumerate() {
            assert_eq!(vertex.len(), 1);
            assert_eq!(vertex.len(), vertex_field_names.len());

            let collection_name = &vertex
                [get_attribute_position(vertex_field_names, "@collection_name")]
            .as_str()
            .unwrap();
            let expected_collection_name = VERTEX_COLLECTION;
            assert_eq!(collection_name, &expected_collection_name);
        }

        assert_eq!(vertex_field_names.len(), 1);
        assert!(vertex_field_names.contains(&"@collection_name".to_string()));
        Ok(())
    };
    let vertices_result = graph_loader.do_vertices(handle_vertices).await;
    assert!(vertices_result.is_ok());

    let handle_edges = move |from_ids: &Vec<Vec<u8>>,
                             to_ids: &Vec<Vec<u8>>,
                             columns: &mut Vec<Vec<Value>>,
                             edge_field_names: &Vec<String>| {
        assert_eq!(from_ids.len(), 9);
        assert_eq!(from_ids.len(), to_ids.len());
        assert_eq!(columns.len(), 9);

        for (e_index, from_id) in from_ids.iter().enumerate() {
            let from_id_str = from_id.iter().map(|x| *x as char).collect::<String>();
            let to_id_str = to_ids[e_index]
                .iter()
                .map(|x| *x as char)
                .collect::<String>();
            assert_eq!(from_id_str, format!("{}/{}", VERTEX_COLLECTION, e_index));
            assert_eq!(to_id_str, format!("{}/{}", VERTEX_COLLECTION, e_index + 1));
        }

        for (e_index, to_id) in to_ids.iter().enumerate() {
            let from_id_str = from_ids[e_index]
                .iter()
                .map(|x| *x as char)
                .collect::<String>();
            let to_id_str = to_id.iter().map(|x| *x as char).collect::<String>();
            assert_eq!(from_id_str, format!("{}/{}", VERTEX_COLLECTION, e_index));
            assert_eq!(to_id_str, format!("{}/{}", VERTEX_COLLECTION, e_index + 1));
        }

        for (_e_index, edge) in columns.iter().enumerate() {
            assert_eq!(edge.len(), 3);
            assert_eq!(edge.len(), edge_field_names.len());

            let collection_name = &edge
                [get_attribute_position(edge_field_names, "@collection_name")]
            .as_str()
            .unwrap();
            let expected_collection_name = EDGE_COLLECTION;
            assert_eq!(collection_name, &expected_collection_name);
        }

        assert_eq!(edge_field_names.len(), 1);
        assert!(edge_field_names.contains(&"@collection_name".to_string()));
        Ok(())
    };
    let edges_result = graph_loader.do_edges(handle_edges).await;
    assert!(edges_result.is_ok());

    teardown().await;
}

#[tokio::test]
#[serial]
async fn init_named_graph_loader_with_data_all_v_and_e_attributes_all_by_boolean() {
    setup(true).await;
    let db_config = build_db_config();
    let load_config = build_load_config_with_v_with_e(true, true);
    let global_fields = vec![];
    let graph_loader_res = GraphLoader::new_named(
        db_config,
        load_config,
        GRAPH.to_string(),
        Some(global_fields.clone()),
        Some(global_fields),
    )
    .await;
    if let Err(ref e) = graph_loader_res {
        println!("{:?}", e);
    }

    assert!(graph_loader_res.is_ok());

    // check that data is loaded.
    // in this case, no global vertex attributes have been requested
    // therefore only the _key attribute is loaded for vertex documents
    let graph_loader = graph_loader_res.unwrap();
    let handle_vertices = move |vertex_ids: &Vec<Vec<u8>>,
                                columns: &mut Vec<Vec<Value>>,
                                vertex_field_names: &Vec<String>| {
        assert_eq!(vertex_ids.len(), 10);
        assert_eq!(columns.len(), 10);

        for (v_index, v_id) in vertex_ids.iter().enumerate() {
            let id = v_id.iter().map(|x| *x as char).collect::<String>();
            let expected_id = format!("{}/{}", VERTEX_COLLECTION, v_index);
            assert_eq!(id, expected_id);
        }

        for (v_index, vertex_json_arr) in columns.iter().enumerate() {
            assert_eq!(vertex_json_arr.len(), 1);
            let vertex = &vertex_json_arr[0];
            assert_eq!(3, vertex.as_object().unwrap().len());

            let x = &vertex[get_attribute_position(vertex_field_names, "x")]
                .as_u64()
                .unwrap();
            let y = &vertex[get_attribute_position(vertex_field_names, "y")]
                .as_u64()
                .unwrap();
            let z = &vertex[get_attribute_position(vertex_field_names, "z")]
                .as_u64()
                .unwrap();
            let expected_x_value = (v_index + 1) as u64;
            let expected_y_value = (v_index + 2) as u64;
            let expected_z_value = (v_index + 3) as u64;
            assert_eq!(x, &expected_x_value);
            assert_eq!(y, &expected_y_value);
            assert_eq!(z, &expected_z_value);
        }

        assert_eq!(vertex_field_names.len(), 0);
        Ok(())
    };
    let vertices_result = graph_loader.do_vertices(handle_vertices).await;
    assert!(vertices_result.is_ok());

    let handle_edges = move |from_ids: &Vec<Vec<u8>>,
                             to_ids: &Vec<Vec<u8>>,
                             columns: &mut Vec<Vec<Value>>,
                             edge_field_names: &Vec<String>| {
        assert_eq!(from_ids.len(), 9);
        assert_eq!(from_ids.len(), to_ids.len());
        assert_eq!(columns.len(), 9);

        for (e_index, from_id) in from_ids.iter().enumerate() {
            let from_id_str = from_id.iter().map(|x| *x as char).collect::<String>();
            let to_id_str = to_ids[e_index]
                .iter()
                .map(|x| *x as char)
                .collect::<String>();
            assert_eq!(from_id_str, format!("{}/{}", VERTEX_COLLECTION, e_index));
            assert_eq!(to_id_str, format!("{}/{}", VERTEX_COLLECTION, e_index + 1));
        }

        for (e_index, to_id) in to_ids.iter().enumerate() {
            let from_id_str = from_ids[e_index]
                .iter()
                .map(|x| *x as char)
                .collect::<String>();
            let to_id_str = to_id.iter().map(|x| *x as char).collect::<String>();
            assert_eq!(from_id_str, format!("{}/{}", VERTEX_COLLECTION, e_index));
            assert_eq!(to_id_str, format!("{}/{}", VERTEX_COLLECTION, e_index + 1));
        }

        for (e_index, edge_json_arr) in columns.iter().enumerate() {
            assert_eq!(edge_json_arr.len(), 1);
            assert_eq!(edge_json_arr.len(), edge_field_names.len());
            let edge = &edge_json_arr[0];
            assert_eq!(3, edge.as_object().unwrap().len());

            let x = &edge[get_attribute_position(edge_field_names, "x")]
                .as_u64()
                .unwrap();
            let y = &edge[get_attribute_position(edge_field_names, "y")]
                .as_u64()
                .unwrap();
            let z = &edge[get_attribute_position(edge_field_names, "z")]
                .as_u64()
                .unwrap();
            let expected_x_value = (e_index + 1) as u64;
            let expected_y_value = (e_index + 2) as u64;
            let expected_z_value = (e_index + 3) as u64;
            assert_eq!(x, &expected_x_value);
            assert_eq!(y, &expected_y_value);
            assert_eq!(z, &expected_z_value);
        }

        assert_eq!(edge_field_names.len(), 0);
        Ok(())
    };
    let edges_result = graph_loader.do_edges(handle_edges).await;
    assert!(edges_result.is_ok());

    teardown().await;
}

#[tokio::test]
#[serial]
async fn init_unknown_named_graph_loader() {
    let db_config = build_db_config();
    let load_config = build_load_config();

    let graph_loader_res = GraphLoader::new_named(
        db_config,
        load_config,
        "UnknownGraph".to_string(),
        None,
        None,
    )
    .await;

    assert!(graph_loader_res.is_err());
}

#[tokio::test]
#[serial]
async fn init_custom_graph_loader() {
    setup(false).await;
    let db_config = build_db_config();
    let load_config = build_load_config();
    let vertex_collection_info = vec![CollectionInfo {
        name: VERTEX_COLLECTION.to_string(),
        fields: vec![],
    }];
    let edge_collection_info = vec![CollectionInfo {
        name: EDGE_COLLECTION.to_string(),
        fields: vec![],
    }];

    let graph_loader_res = GraphLoader::new_custom(
        db_config,
        load_config,
        vertex_collection_info,
        edge_collection_info,
    )
    .await;

    if let Err(ref e) = graph_loader_res {
        println!("{:?}", e);
    }
    assert!(graph_loader_res.is_ok());
    teardown().await;
}

#[fixture]
fn generate_collection_load_combinations() -> Vec<(bool, bool)> {
    let mut combinations = vec![];
    for &a in &[true, false] {
        for &b in &[true, false] {
            combinations.push((a, b));
        }
    }
    combinations
}

#[tokio::test]
#[serial]
async fn init_custom_graph_loader_with_fields_and_fetch_all_attributes_positive() {
    let test_variants: Vec<(bool, bool)> = generate_collection_load_combinations();
    setup(false).await;

    for (fetch_all_v_attributes, fetch_all_e_attributes) in test_variants {
        let vertex_fields = vec![];
        let edge_fields = vec![];

        let db_config = build_db_config();
        let load_config =
            build_load_config_with_v_with_e(fetch_all_v_attributes, fetch_all_e_attributes);
        let vertex_collection_info = vec![CollectionInfo {
            name: VERTEX_COLLECTION.to_string(),
            fields: vertex_fields,
        }];
        let edge_collection_info = vec![CollectionInfo {
            name: EDGE_COLLECTION.to_string(),
            fields: edge_fields,
        }];

        let graph_loader_res = GraphLoader::new_custom(
            db_config,
            load_config,
            vertex_collection_info,
            edge_collection_info,
        )
        .await;

        if let Err(ref e) = graph_loader_res {
            println!("{:?}", e);
        }
        // in case we never define specific fields (CollectionInfo), it should always pass
        assert!(graph_loader_res.is_ok());
    }
    teardown().await;
}

#[tokio::test]
#[serial]
async fn init_custom_graph_loader_with_fields_and_fetch_all_attributes_negative() {
    setup(false).await;
    let test_variants: Vec<(bool, bool)> = generate_collection_load_combinations();

    for (fetch_all_v_attributes, fetch_all_e_attributes) in test_variants {
        let vertex_fields = vec!["_id".to_string()];
        let edge_fields = vec!["_key".to_string()];

        let db_config = build_db_config();
        let load_config =
            build_load_config_with_v_with_e(fetch_all_v_attributes, fetch_all_e_attributes);
        let vertex_collection_info = vec![CollectionInfo {
            name: VERTEX_COLLECTION.to_string(),
            fields: vertex_fields,
        }];
        let edge_collection_info = vec![CollectionInfo {
            name: EDGE_COLLECTION.to_string(),
            fields: edge_fields,
        }];

        let graph_loader_res = GraphLoader::new_custom(
            db_config,
            load_config,
            vertex_collection_info,
            edge_collection_info,
        )
        .await;

        if let Err(ref e) = graph_loader_res {
            println!("{:?}", e);
        }
        // in case we never define specific fields (CollectionInfo), it should always pass
        if fetch_all_v_attributes || fetch_all_e_attributes {
            assert!(graph_loader_res.is_err());
        } else {
            assert!(graph_loader_res.is_ok());
        }
    }
    teardown().await;
}

#[tokio::test]
#[serial]
async fn init_empty_custom_graph_loader() {
    setup(false).await;
    let is_cluster = is_cluster().await;
    let db_config = build_db_config();
    let load_config = build_load_config();
    let version_str = get_arangodb_version().await.version;
    let (major, minor, _) = extract_version_parts(&version_str).unwrap();

    let graph_loader_res = GraphLoader::new_custom(db_config, load_config, vec![], vec![]).await;

    if let Err(ref e) = graph_loader_res {
        println!("{:?}", e);
    }
    assert!(graph_loader_res.is_ok());

    let graph_loader = graph_loader_res.unwrap();
    let handle_vertices =
        move |_vertex_keys: &Vec<Vec<u8>>,
              _vertex_json: &mut Vec<Vec<Value>>,
              _vertex_field_names: &Vec<String>| { Ok(()) };
    let vertices_result = graph_loader.do_vertices(handle_vertices).await;

    if let Err(ref e) = vertices_result {
        println!("{:?}", e);
    }
    if is_cluster {
        if major > 3 || (major == 3 && minor >= 12) {
            assert!(vertices_result.is_err());
            match vertices_result {
                Err(GraphLoaderError::Other(ref msg))
                    if msg.contains("No vertex collections given!") =>
                {
                    assert!(true)
                }
                _ => assert!(false),
            }
        }
    } else {
        if major > 3 || (major == 3 && minor >= 12) {
            // single server dump endpoint only supported from 3.12
            // all versions below will fall back to aql.
            // uses dump endpoint, must fail
            assert!(vertices_result.is_err());
        } else {
            // In the SingleServer case we do not have an error as we execute AQL on empty collections.
            // Means we're just not receiving any documents.
            assert!(vertices_result.is_ok());
        }
    }

    let handle_edges = move |_from_ids: &Vec<Vec<u8>>,
                             _to_ids: &Vec<Vec<u8>>,
                             _json: &mut Vec<Vec<Value>>,
                             _fields: &Vec<String>| { Ok(()) };
    let edges_result = graph_loader.do_edges(handle_edges).await;

    if is_cluster {
        if major > 3 || (major == 3 && minor >= 11) {
            assert!(edges_result.is_err());
            match edges_result {
                Err(GraphLoaderError::Other(ref msg))
                    if msg.contains("No edge collections given!") =>
                {
                    assert!(true)
                }
                _ => assert!(false),
            }
        }
    } else {
        if major > 3 || (major == 3 && minor >= 12) {
            // uses dump endpoint, must fail
            assert!(vertices_result.is_err());
        } else {
            // In the SingleServer case we do not have an error as we execute AQL on empty collections.
            // Means we're just not receiving any documents.
            assert!(vertices_result.is_ok());
        }
    }
    if let Err(ref e) = edges_result {
        println!("{:?}", e);
    }

    teardown().await;
}

#[tokio::test]
#[serial]
async fn init_unknown_custom_graph_loader() {
    let db_config = build_db_config();
    let load_config = build_load_config();
    let vertex_collection_info = vec![CollectionInfo {
        name: "UnknownVertex".to_string(),
        fields: vec![],
    }];
    let edge_collection_info = vec![CollectionInfo {
        name: "UnknownEdge".to_string(),
        fields: vec![],
    }];

    let graph_loader_res = GraphLoader::new_custom(
        db_config,
        load_config,
        vertex_collection_info,
        edge_collection_info,
    )
    .await;

    let is_cluster = is_cluster().await;
    if is_cluster {
        // will error out as we cannot compute the shard map during init
        assert!(graph_loader_res.is_err());
    } else {
        // as we are not in a cluster, we can compute the shard map during init
        assert!(graph_loader_res.is_ok());
    }
}
