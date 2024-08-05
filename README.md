# arangors-graph-exporter (ArangoDB Rust Graph Loader)

This Rust-based library provides a high-performance and parallel way to load data from ArangoDB. It supports loading both named graphs and custom graphs, with options to specify which vertex and edge attributes to load.

[![Crates.io][crates-badge]][crates-url]
[![MIT licensed][mit-badge]][mit-url]
[![CircleCI](https://dl.circleci.com/status-badge/img/gh/arangodb/arangors-graph-exporter/tree/main.svg?style=shield&circle-token=CCIPRJ_7429hLhBbSuHBq59JVpQYZ_1339fa571ff52f0bd39712e72c637b1b3596b95a)](https://dl.circleci.com/status-badge/redirect/gh/arangodb/arangors-graph-exporter/tree/main)

[crates-url]: https://crates.io/crates/arangors-graph-exporter
[crates-badge]: https://img.shields.io/crates/v/arangors-graph-exporter.svg
[mit-url]: https://github.com/arangodb/arangors-graph-exporter/blob/main/LICENSE
[mit-badge]: https://img.shields.io/badge/license-MIT-blue.svg

[API Docs](https://arangodb.github.io/arangors-graph-exporter/arangors_graph_exporter/index.html) |
[ArangoDB Docs](https://docs.arangodb.com/stable) |
[ArangoDB](https://www.arangodb.com)


## Installation

Add the following to your `Cargo.toml`:

```toml
[dependencies]
arangodb-graph-loader = "0.0.6"
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
use arangodb_graph_loader::{DatabaseConfiguration, DataLoadConfiguration, GraphLoader, GraphLoaderError};

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
use arangodb_graph_loader::{DatabaseConfiguration, DataLoadConfiguration, GraphLoader, GraphLoaderError, CollectionInfo};

async fn create_custom_graph_loader() -> Result<GraphLoader, GraphLoaderError> {
    let db_config = DatabaseConfiguration::new(/* parameters */);
    let load_config = DataLoadConfiguration::new(/* parameters */);
    let vertex_collections = vec![CollectionInfo::new(/* parameters */)];
    let edge_collections = vec![CollectionInfo::new(/* parameters */)];

    GraphLoader::new_custom(db_config, load_config, vertex_collections, edge_collections).await
}
```

### Loading Data

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
