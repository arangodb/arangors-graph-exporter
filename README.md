# arangors-graph-exporter (ArangoDB Rust Graph Loader)

This Rust-based library provides a high-performance and parallel way to load data from ArangoDB. It supports loading both named graphs and custom graphs, with options to specify which vertex and edge attributes to load.

[![Crates.io][crates-badge]][crates-url]
[![MIT licensed][mit-badge]][mit-url]
[![CircleCI](https://dl.circleci.com/status-badge/img/gh/arangodb/arangors-graph-exporter/tree/main.svg?style=shield&circle-token=CCIPRJ_7429hLhBbSuHBq59JVpQYZ_1339fa571ff52f0bd39712e72c637b1b3596b95a)](https://dl.circleci.com/status-badge/redirect/gh/arangodb/arangors-graph-exporter/tree/main)

[crates-url]: https://crates.io/crates/arangors-graph-exporter
[crates-badge]: https://img.shields.io/crates/v/arangors-graph-exporter.svg
[mit-url]: https://github.com/arangodb/arangors-graph-exporter/blob/main/LICENSE
[mit-badge]: https://img.shields.io/badge/license-MIT-blue.svg

[API Docs Stable](https://docs.rs/arangors-graph-exporter/latest/arangors_graph_exporter/) |
[API Docs Main](https://arangodb.github.io/arangors-graph-exporter/arangors_graph_exporter/index.html) |
[ArangoDB Docs](https://docs.arangodb.com/stable) |
[ArangoDB](https://www.arangodb.com)


## Installation

Add the following crate to your `Cargo.toml` by doing:

```bash
cargo add arangors-graph-exporter
```

## Usage

### Initialization

There are two different approaches to initialize the graph loader:
1. [Named Graph](https://docs.arangodb.com/3.12/graphs/#named-graphs)
2. [Custom Graph](https://docs.arangodb.com/3.12/graphs/#anonymous-graphs)

#### Named Graph

A named graph is a graph in ArangoDB that has a name and its graph definition is already stored in the database.
To initialize a graph loader for a named graph,  use the `GraphLoader::new_named` method.

```rust
use arangors_graph_exporter::{DatabaseConfiguration, DataLoadConfiguration, GraphLoader, GraphLoaderError};

async fn create_named_graph_loader() -> Result<GraphLoader, GraphLoaderError> {
    let db_config = DatabaseConfiguration::new(/* parameters */);
    let load_config = DataLoadConfiguration::new(/* parameters */);
    let graph_name = String::from("my_named_graph");
    let vertex_global_fields = Some(vec![String::from("lastname"), String::from("firstname")]);
    let edge_global_fields = Some(vec![String::from("field1"), String::from("field2")]);

    GraphLoader::new_named(db_config, load_config, graph_name, vertex_global_fields, edge_global_fields).await
}
```

#### Custom Graph

A custom graph or anonymous graph is a graph that can act as a graph but does not have a name or a graph definition
stored in the database.

To create a graph loader for a custom graph:

```rust
use arangors_graph_exporter::{DatabaseConfiguration, DataLoadConfiguration, GraphLoader, GraphLoaderError, CollectionInfo};

async fn create_custom_graph_loader() -> Result<GraphLoader, GraphLoaderError> {
    let db_config = DatabaseConfiguration::new(/* parameters */);
    let load_config = DataLoadConfiguration::new(/* parameters */);
    let vertex_collections = vec![CollectionInfo::new(/* parameters */)];
    let edge_collections = vec![CollectionInfo::new(/* parameters */)];

    GraphLoader::new_custom(db_config, load_config, vertex_collections, edge_collections).await
}
```

#### AQL-Based Graph Loading

AQL-based graph loading provides a flexible way to load subgraphs from ArangoDB using custom AQL queries. This approach is particularly well-suited for:
- Loading relatively small subgraphs
- Using indexes or traversals to find the right subgraph
- Applying complex filtering conditions to vertices and edges
- Executing graph traversals to define the subgraph

##### Graph Loading Specification

A "graph loading specification" is a list of lists of AQL queries, where each query is a pair of a query string and a map of bind parameters. The specification has the following semantics:

- The **outer list** is executed **sequentially**
- Each **inner list** contains queries that can be executed **in parallel**
- Each query must return items in the following format:

```json
{"vertices":[...], "edges":[...]}
```

Both `vertices` and `edges` attributes are optional. 
- Vertex entries must contain at least an `_id` attribute
- Edge entries must contain at least `_from` and `_to` attributes
- Edge values can be `null` and will be silently ignored (useful for traversal start nodes)

##### Attribute Specification

You can declare the vertex and edge attributes upfront with their types for efficient columnar storage.

**Supported Data Types:**

- **`DataType::Bool`** - Boolean values
  - Accepts: `true`/`false`, strings like "true"/"false"/"yes"/"no"/"1"/"0", numbers (0=false, non-zero=true)
  
- **`DataType::String`** - Text strings
  - Accepts: any value (null becomes empty string, objects/arrays become JSON strings)
  
- **`DataType::U64`** - Unsigned 64-bit integers (non-negative)
  - Accepts: non-negative integers, positive floats (rounded), numeric strings
  - Rejects: negative values
  
- **`DataType::I64`** - Signed 64-bit integers
  - Accepts: any integer, floats (rounded), numeric strings
  
- **`DataType::F64`** - 64-bit floating-point numbers
  - Accepts: any numeric value, numeric strings
  - Rejects: infinity and NaN values
  
- **`DataType::JSON`** - Any JSON value
  - Accepts: anything without type conversion

**Type Conversion Errors:**

When a value cannot be converted to the specified type, a default value is used and the error is recorded:
- `Bool` → `false`
- `String` → `""` (empty string)
- `U64` / `I64` → `0`
- `F64` → `0.0`
- `JSON` → `null`

The `_id` attribute for vertices and `_from`/`_to` attributes for edges are automatically included and don't need to be specified.

##### Edge Buffering

It's allowed to produce edges whose end-vertices appear later in the same query or in subsequent queries. The loader will buffer such edges until all vertices are available. For optimal performance, produce vertices before edges to minimize buffering.

##### Example 1: Filtered Vertex and Edge Collections

This example shows how to load a subgraph from multiple vertex and edge collections with filter conditions. The first inner list loads all vertices in parallel, and the second inner list loads all edges in parallel:

```json
[
  [
    {
      "query": "FOR x IN vertices1 FILTER x.status == 'active' RETURN {vertices:[x]}", 
      "bindVars": {}
    },
    {
      "query": "FOR x IN vertices2 FILTER x.type == 'user' RETURN {vertices:[x]}", 
      "bindVars": {}
    }
  ],
  [
    {
      "query": "FOR y IN edges1 FILTER y.weight > @minWeight RETURN {edges:[y]}", 
      "bindVars": {"minWeight": 0.5}
    },
    {
      "query": "FOR y IN edges2 FILTER y.relation == 'follows' RETURN {edges:[y]}", 
      "bindVars": {}
    }
  ]
]
```

This approach ensures that all vertices are loaded first (no edge buffering needed), and allows parallel loading within each phase.

##### Example 2: Graph Traversals

This example shows how to use graph traversals to define the subgraph. Note that traversals produce both vertices and edges in the same result:

```json
[
  [
    {
      "query": "FOR s IN @startids1 FOR x, y IN 0..10 OUTBOUND s GRAPH 'socialGraph' PRUNE x.depth > 5 FILTER x.active == true RETURN {vertices:[x], edges:[y]}",
      "bindVars": {"startids1": ["users/123", "users/456"]}
    },
    {
      "query": "FOR s IN @startids2 FOR x, y IN 0..10 OUTBOUND s GRAPH 'socialGraph' PRUNE x.depth > 5 FILTER x.active == true RETURN {vertices:[x], edges:[y]}",
      "bindVars": {"startids2": ["users/789"]}
    }
  ]
]
```

The depth 0 case includes the starting vertex with a `null` edge. Multiple traversals can be executed in parallel as shown above, or sequentially by placing them in separate inner lists:

```json
[
  [
    {
      "query": "FOR s IN @startids1 FOR x, y IN 0..10 OUTBOUND s GRAPH 'socialGraph' RETURN {vertices:[x], edges:[y]}",
      "bindVars": {"startids1": ["users/123"]}
    }
  ],
  [
    {
      "query": "FOR s IN @startids2 FOR x, y IN 0..10 OUTBOUND s GRAPH 'socialGraph' RETURN {vertices:[x], edges:[y]}",
      "bindVars": {"startids2": ["users/789"]}
    }
  ]
]
```

##### Creating an AqlGraphLoader

To create an AQL graph loader, use the `AqlGraphLoader::new` method:

```rust
use arangors_graph_exporter::{
    AqlGraphLoader, AqlQuery, DataItem, DataType, DatabaseConfiguration, GraphLoaderError
};
use std::collections::HashMap;
use serde_json::Value;

async fn create_aql_graph_loader() -> Result<AqlGraphLoader, GraphLoaderError> {
    let db_config = DatabaseConfiguration::new(/* parameters */);
    let batch_size = 1000;
    
    // Define vertex attributes to load
    let vertex_attributes = vec![
        DataItem::new("name".to_string(), DataType::String),
        DataItem::new("age".to_string(), DataType::U64),
    ];
    
    // Define edge attributes to load
    let edge_attributes = vec![
        DataItem::new("weight".to_string(), DataType::F64),
        DataItem::new("relation".to_string(), DataType::String),
    ];
    
    // Create vertex queries (loaded in parallel)
    let vertex_query1 = AqlQuery::new(
        "FOR x IN vertices1 FILTER x.status == 'active' RETURN {vertices:[x]}".to_string(),
        HashMap::new(),
    );
    let vertex_query2 = AqlQuery::new(
        "FOR x IN vertices2 FILTER x.type == @vtype RETURN {vertices:[x]}".to_string(),
        HashMap::from([("vtype".to_string(), Value::String("user".to_string()))]),
    );
    
    // Create edge queries (loaded in parallel, after vertices)
    let edge_query = AqlQuery::new(
        "FOR e IN edges1 FILTER e.weight > @minWeight RETURN {edges:[e]}".to_string(),
        HashMap::from([("minWeight".to_string(), Value::from(0.5))]),
    );
    
    // Organize queries: outer list = sequential, inner list = parallel
    let queries = vec![
        vec![vertex_query1, vertex_query2], // Load vertices in parallel
        vec![edge_query],                    // Then load edges
    ];
    
    // Configure type error reporting limit
    // None = report all type errors (recommended default)
    // Some(n) = report at most n type errors per GraphBatch
    let max_type_errors = None;
    
    AqlGraphLoader::new(
        db_config,
        batch_size,
        vertex_attributes,
        edge_attributes,
        queries,
        max_type_errors
    )
}
```

##### Example: Graph Traversal

For graph traversals that produce vertices and edges together:

```rust
use arangors_graph_exporter::{AqlGraphLoader, AqlQuery, DatabaseConfiguration, DataItem, DataType, GraphLoaderError};
use std::collections::HashMap;
use serde_json::Value;


async fn create_traversal_loader() -> Result<AqlGraphLoader, GraphLoaderError> {
    let db_config = DatabaseConfiguration::new(/* parameters */);
    let batch_size = 1000;
    
    // For traversals, we might not need extra attributes beyond _id, _from, _to
    let vertex_attributes = vec![];
    let edge_attributes = vec![
        DataItem::new("depth".to_string(), DataType::U64),
    ];
    
    // Traversal query that returns both vertices and edges
    let traversal_query = AqlQuery::new(
        "FOR v, e IN 0..10 OUTBOUND @start GRAPH 'socialGraph' \
         PRUNE v.depth > 5 \
         RETURN {vertices:[v], edges:[e]}".to_string(),
        HashMap::from([
            ("start".to_string(), Value::String("users/123".to_string()))
        ]),
    );
    
    let queries = vec![vec![traversal_query]];
    
    AqlGraphLoader::new(db_config, batch_size, vertex_attributes, edge_attributes, queries, None)
}
```

### Loading Data with GraphLoader

Once the graph loader is initialized, you can load vertices and edges using the following methods:
1. `do_vertices`: Load vertices from the graph.
2. `do_edges`: Load edges from the graph.

Both methods take a closure as an argument to handle the loaded data.
If during the initialization you specified the global fields to load, the closure will receive the global fields as well.
If no global fields are specified, the closure will receive only the required fields. For vertices, the required fields are the vertex ID and the vertex key.
For edges the required fields are the `from` vertex IDs and `to` vertex IDs.

#### Vertices

The closure for handling vertices takes the following arguments:

```rust
let handle_vertices = |vertex_ids: &Vec<Vec<u8>>, columns: &mut Vec<Vec<Value>>, vertex_field_names: &Vec<String>| {
    // Handle vertex data
};

graph_loader.do_vertices(handle_vertices).await?;
```

#### Edges

The closure for handling edges takes the following arguments:

```rust
let handle_edges = |from_ids: &Vec<Vec<u8>>, to_ids: &Vec<Vec<u8>>, columns: &mut Vec<Vec<Value>>, edge_field_names: &Vec<String>| {
    // Handle edge data
};

let edges_result = graph_loader.do_edges(handle_edges).await?;
```

### Loading Data with AqlGraphLoader

Once the AQL graph loader is initialized, load the graph data using the `do_load` method with a callback function.

#### The Callback Function

The callback receives a mutable reference to a `GraphBatch` containing both vertices and edges. The batch structure includes:

- **vertex_ids**: Vector of vertex IDs as byte vectors
- **vertex_attribute_values**: Vector of attribute vectors, parallel to vertex_ids
- **edge_from_ids**: Vector of source vertex IDs as byte vectors
- **edge_to_ids**: Vector of target vertex IDs as byte vectors
- **edge_attribute_values**: Vector of attribute vectors, parallel to edge IDs
- **type_error_count**: Total number of type conversion errors encountered
- **type_error_messages**: Type error messages (limited based on configuration)

#### Type Error Reporting Configuration

The `AqlGraphLoader` allows you to configure how many type error messages are collected per `GraphBatch`:

- **`None` (recommended default)**: All type conversion errors are reported. This is useful for debugging and ensures you see every issue.
- **`Some(n)`**: At most `n` type error messages are collected per `GraphBatch`. The total count is always tracked in `type_error_count`, but only the first `n` messages are stored in `type_error_messages`. This can help reduce memory usage when dealing with data that has many type errors.
- **`Some(0)`**: No error messages are collected (but `type_error_count` is still incremented). Use this if you only need to know the count of errors, not the details.

Examples:
```rust
// Report all type errors (recommended)
let loader = AqlGraphLoader::new(
    db_config, batch_size, vertex_attrs, edge_attrs, queries, None
)?;

// Report at most 10 type errors per batch
let loader = AqlGraphLoader::new(
    db_config, batch_size, vertex_attrs, edge_attrs, queries, Some(10)
)?;

// Track error count only, no messages
let loader = AqlGraphLoader::new(
    db_config, batch_size, vertex_attrs, edge_attrs, queries, Some(0)
)?;
```

The callback signature is:

```rust
Fn(&mut GraphBatch) -> Result<(), GraphLoaderError>
```

#### Example: Basic Loading

```rust
use arangors_graph_exporter::GraphBatch;

// Load the graph with a callback
aql_loader.do_load(|batch: &mut GraphBatch| {
    // Process vertices
    for (i, vertex_id) in batch.vertex_ids.iter().enumerate() {
        let id_str = String::from_utf8(vertex_id.clone()).unwrap();
        let attribute_values = &batch.vertex_attribute_values[i];
        
        println!("Vertex {}: {:?}", id_str, attribute_values);
        // Process vertex...
    }
    
    // Process edges
    for (i, from_id) in batch.edge_from_ids.iter().enumerate() {
        let from_str = String::from_utf8(from_id.clone()).unwrap();
        let to_str = String::from_utf8(batch.edge_to_ids[i].clone()).unwrap();
        let attribute_values = &batch.edge_attribute_values[i];
        
        println!("Edge {} -> {}: {:?}", from_str, to_str, attribute_values);
        // Process edge...
    }
    
    // Check for type errors
    if batch.type_error_count > 0 {
        eprintln!("Warning: {} type conversion errors", batch.type_error_count);
        for msg in &batch.type_error_messages {
            eprintln!("  {}", msg);
        }
    }
    
    Ok(())
}).await?;
```

#### Example: Collecting Data

```rust
use std::sync::{Arc, Mutex};
use std::collections::HashMap;

// Shared state to collect data
let vertices = Arc::new(Mutex::new(HashMap::new()));
let edges = Arc::new(Mutex::new(Vec::new()));

let vertices_clone = vertices.clone();
let edges_clone = edges.clone();

// Load with closure that captures shared state
aql_loader.do_load(move |batch: &mut GraphBatch| {
    let mut v_map = vertices_clone.lock().unwrap();
    let mut e_vec = edges_clone.lock().unwrap();
    
    // Collect vertices
    for (i, vertex_id) in batch.vertex_ids.iter().enumerate() {
        let id_str = String::from_utf8(vertex_id.clone()).unwrap();
        let attrs = batch.vertex_attribute_values[i].clone();
        v_map.insert(id_str, attrs);
    }
    
    // Collect edges
    for (i, from_id) in batch.edge_from_ids.iter().enumerate() {
        let from_str = String::from_utf8(from_id.clone()).unwrap();
        let to_str = String::from_utf8(batch.edge_to_ids[i].clone()).unwrap();
        let attrs = batch.edge_attribute_values[i].clone();
        e_vec.push((from_str, to_str, attrs));
    }
    
    Ok(())
}).await?;

// Access collected data after loading
let final_vertices = vertices.lock().unwrap();
let final_edges = edges.lock().unwrap();
println!("Loaded {} vertices and {} edges", final_vertices.len(), final_edges.len());
```

#### Important Notes

- The callback is called multiple times as batches are loaded (batch size is configured during initialization)
- Vertices and edges may arrive in the same batch (especially for traversal queries)
- The callback must be `Send + Sync + Clone` to support parallel query execution
- Attributes in `vertex_attribute_values` and `edge_attribute_values` correspond to the `DataItem` specifications provided during initialization
- Type conversion errors are collected but don't stop the loading process; check `type_error_count` to handle them appropriately

## Configuration

### Database Configuration

Provide your database configuration parameters to `DatabaseConfiguration::new`.
Please read the documentation for more information on the available parameters.

### Data Load Configuration

Configure data loading parameters with `DataLoadConfiguration::new`.
Please read the documentation for more information on the available parameters.

## Attributes

### Named Graph

- **graph_name**: The name of the graph in ArangoDB.
- **vertex_global_fields**: Optional. List of vertex attributes to load.
- **edge_global_fields**: Optional. List of edge attributes to load.

### Custom Graph

- **vertex_collections**: List of vertex collections to load.
- **edge_collections**: List of edge collections to load.

## Special Attributes as fields names

Right now there is only one special field available. Special fields are identified by the `@` prefix.

- **@collection_name**: Include the collection name in the returned data.

## Flags

- **load_all_vertex_attributes**: Boolean flag to load all vertex attributes.
- **load_all_edge_attributes**: Boolean flag to load all edge attributes.

## Error Handling

All methods return `Result` types. Handle errors using Rust's standard error handling mechanisms.
The error type is `GraphLoaderError`.

Example return type:

```
Result<(), GraphLoaderError>
```


```rust
match graph_loader.do_vertices(handle_vertices).await {
    Ok(_) => println!("Vertices loaded successfully"),
    Err(e) => eprintln!("Error loading vertices: {:?}", e),
}
```

## License

This project is licensed under the MIT License.

## Getting Help

First, see if the answer to your question can be found in the [API documentation].
If your question couldn't be solved, please feel free to pick one of those resources: 

- Please use GitHub for feature requests and bug reports:
  [https://github.com/arangodb/arangors-graph-exporter/issues](https://github.com/arangodb/arangors-graph-exporter/issues)

- Ask questions about the driver, Rust, usage scenarios, etc. on StackOverflow:
  [https://stackoverflow.com/questions/tagged/arangodb](https://stackoverflow.com/questions/tagged/arangodb)

- Chat with the community and the developers on Slack:
  [https://arangodb-community.slack.com/](https://arangodb-community.slack.com/)

- Learn more about ArangoDB with our YouTube channel: 
  [https://www.youtube.com/@ArangoDB](https://www.youtube.com/@ArangoDB)

- Follow us on X to stay up to date:
  [https://x.com/arangodb](https://x.com/arangodb)

- Find out more about our community: [https://www.arangodb.com/community](https://www.arangodb.com/community/)

## Contributing

Contributions are welcome! Please open an issue or submit a pull request.

---

This documentation provides a comprehensive overview of the API and usage of the Rust-based ArangoDB graph loader.
It covers initialization, configuration, data loading, and error handling.
For more detailed examples and advanced usage, please refer to the source code and additional documentation.
