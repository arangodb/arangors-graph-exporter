use serde_json::json;

/// Configuration for graph setup
pub struct GraphConfig<'a> {
    pub db_endpoint: &'a str,
    pub username: &'a str,
    pub password: &'a str,
    pub graph_name: &'a str,
    pub vertex_collection: &'a str,
    pub edge_collection: &'a str,
    pub client: &'a reqwest_middleware::ClientWithMiddleware,
}

/// Creates a simple test graph with optional test data
pub async fn create_graph(config: GraphConfig<'_>, insert_data: bool) {
    // Drop graph if exists
    let drop_url = format!("{}/_api/gharial/{}", config.db_endpoint, config.graph_name);
    let _ = config
        .client
        .delete(&drop_url)
        .basic_auth(config.username, Some(config.password))
        .query(&[("dropCollections", "true")])
        .send()
        .await;

    // Create graph
    let create_graph_body = json!({
        "name": config.graph_name,
        "edgeDefinitions": [{
            "collection": config.edge_collection,
            "from": [config.vertex_collection],
            "to": [config.vertex_collection]
        }],
        "orphanCollections": []
    });

    let create_url = format!("{}/_api/gharial", config.db_endpoint);
    let resp = config
        .client
        .post(&create_url)
        .basic_auth(config.username, Some(config.password))
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&create_graph_body).unwrap())
        .send()
        .await
        .unwrap();

    assert!(
        resp.status().is_success(),
        "Failed to create graph: status={}",
        resp.status()
    );

    if insert_data {
        // Insert vertices
        let vertex_url = format!(
            "{}/_api/document/{}",
            config.db_endpoint, config.vertex_collection
        );
        for i in 0..10 {
            let doc = json!({
                "_key": i.to_string(),
                "x": i + 1,
                "y": i + 2,
                "z": i + 3
            });
            config
                .client
                .post(&vertex_url)
                .basic_auth(config.username, Some(config.password))
                .header("Content-Type", "application/json")
                .body(serde_json::to_string(&doc).unwrap())
                .send()
                .await
                .unwrap();
        }

        // Insert edges
        let edge_url = format!(
            "{}/_api/document/{}",
            config.db_endpoint, config.edge_collection
        );
        for i in 0..9 {
            let doc = json!({
                "_key": i.to_string(),
                "_from": format!("{}/{}", config.vertex_collection, i),
                "_to": format!("{}/{}", config.vertex_collection, i + 1),
                "x": i + 1,
                "y": i + 2,
                "z": i + 3
            });
            config
                .client
                .post(&edge_url)
                .basic_auth(config.username, Some(config.password))
                .header("Content-Type", "application/json")
                .body(serde_json::to_string(&doc).unwrap())
                .send()
                .await
                .unwrap();
        }
    }
}

/// Creates a binary tree graph of the specified depth
pub async fn create_binary_tree_graph(config: GraphConfig<'_>, depth: usize) {
    // Drop graph if exists
    let drop_url = format!("{}/_api/gharial/{}", config.db_endpoint, config.graph_name);
    let _ = config
        .client
        .delete(&drop_url)
        .basic_auth(config.username, Some(config.password))
        .query(&[("dropCollections", "true")])
        .send()
        .await;

    // Create graph
    let create_graph_body = json!({
        "name": config.graph_name,
        "edgeDefinitions": [{
            "collection": config.edge_collection,
            "from": [config.vertex_collection],
            "to": [config.vertex_collection]
        }],
        "orphanCollections": []
    });

    let create_url = format!("{}/_api/gharial", config.db_endpoint);
    let resp = config
        .client
        .post(&create_url)
        .basic_auth(config.username, Some(config.password))
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&create_graph_body).unwrap())
        .send()
        .await
        .unwrap();

    assert!(
        resp.status().is_success(),
        "Failed to create graph: status={}",
        resp.status()
    );

    // Calculate total number of vertices in a complete binary tree
    // Formula: 2^(depth+1) - 1
    let total_vertices = (1 << (depth + 1)) - 1;

    // Insert vertices with depth attribute
    let vertex_url = format!(
        "{}/_api/document/{}",
        config.db_endpoint, config.vertex_collection
    );

    for i in 0..total_vertices {
        // Calculate depth: floor(log2(i+1))
        let vertex_depth = if i == 0 {
            0
        } else {
            ((i + 1) as u32).ilog2() as u64
        };

        let doc = json!({
            "_key": i.to_string(),
            "depth": vertex_depth
        });
        config
            .client
            .post(&vertex_url)
            .basic_auth(config.username, Some(config.password))
            .header("Content-Type", "application/json")
            .body(serde_json::to_string(&doc).unwrap())
            .send()
            .await
            .unwrap();
    }

    // Insert edges
    let edge_url = format!(
        "{}/_api/document/{}",
        config.db_endpoint, config.edge_collection
    );

    let mut edge_key = 0;
    for i in 0..total_vertices {
        let left_child = 2 * i + 1;
        let right_child = 2 * i + 2;

        // Add left child edge if it exists
        if left_child < total_vertices {
            let child_depth = ((left_child + 1) as u32).ilog2() as u64;
            let doc = json!({
                "_key": edge_key.to_string(),
                "_from": format!("{}/{}", config.vertex_collection, i),
                "_to": format!("{}/{}", config.vertex_collection, left_child),
                "depth": child_depth,
                "type": "left"
            });
            config
                .client
                .post(&edge_url)
                .basic_auth(config.username, Some(config.password))
                .header("Content-Type", "application/json")
                .body(serde_json::to_string(&doc).unwrap())
                .send()
                .await
                .unwrap();
            edge_key += 1;
        }

        // Add right child edge if it exists
        if right_child < total_vertices {
            let child_depth = ((right_child + 1) as u32).ilog2() as u64;
            let doc = json!({
                "_key": edge_key.to_string(),
                "_from": format!("{}/{}", config.vertex_collection, i),
                "_to": format!("{}/{}", config.vertex_collection, right_child),
                "depth": child_depth,
                "type": "right"
            });
            config
                .client
                .post(&edge_url)
                .basic_auth(config.username, Some(config.password))
                .header("Content-Type", "application/json")
                .body(serde_json::to_string(&doc).unwrap())
                .send()
                .await
                .unwrap();
            edge_key += 1;
        }
    }
}
