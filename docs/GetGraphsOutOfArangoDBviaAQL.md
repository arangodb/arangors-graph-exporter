# Getting a graph out of ArangoDB via AQL

This proposal is about getting a subgraph of a graph in ArangoDB out using
AQL. We want to use this in particular in the Graph Analytics Engine (GAE) and
in Python code for graph analytics.


## Motivation

We already have a very fast and scalable method to get a full ArangoDB
graph out (with some limited amount of filtering and projections), so now
we need another additional approach, which is:

 - as flexible as possible, and
 - well suited for relatively small subgraphs, and
 - which allows to use indexes or traversals to find the right subgraph
 
Therefore, we take as guiding principle two common use cases:

 1. The subgraph is defined by naming the vertex collection(s) and
    specifying a `FILTER` condition to say which vertices are taken, and by
    naming the edge collection(s) and specifying a `FILTER` condition to say
    which edges are taken. The `FILTER` conditions might be so that indexes
    can help with the selection.

 2. The subgraph is defined by specifying one (or more) graph traversals 
    which "finds" the subgraph to be exported.

Other use cases are of course possible and should also be supported, but
we use these two for inspiration about the way we want to specify the
export.

Note that AQL does not make it particularly easy to produce results which
come from **different** collections. For example, there is no easy way to
return the union of two (filtered) collections:

```aql
LET l1 = (FOR x IN coll1
            FILTER x.type == "xyz"
            RETURN x)
LET l2 = (FOR y IN coll2
            FILTER y.type == "abc"
            RETURN y)
FOR z IN CONCATENATION(l1, l2)
  RETURN z
```

is possible, but not very efficient, and prone to OOM situations for larger
data. It is currently not very well optimized and streaming thus does not
work well.

This observation basically suggests, that it should be possible to give
**multiple** queries to get all vertices and all edges.

For the GAE it is currently particularly welcome, if all **vertices** are
exported before the **edges** arrive. If this is the case, then the edge-import
can directly verify the incoming edges to see if their end-vertices exist
and can thus avoid dangling edges and can encode the edge in a very 
memory-efficient way.

This observation suggests, that it should be possible to produce the
vertices first and then the edges.

However for the graph traversal cases, the vertices and edges are produced
at the same time, so this suggests that the GAE must also be able to store
the incoming edges till the end, and only then validate them. In any case,
the user needs control over the order of execution.

At the same time, parallelism should also be possible for performance
benefits. For example, it is trivially parallelizable to export vertices
from different vertex collections concurrently.

Although it is easy to distinguish a vertex from an edge by checking if there
are attributes `_from` and `_to`, it is also not possible to just assume that
the set of AQL results are simply a mixture of vertices and edges. Namely, in 
a query like:

```aql
FOR v, e IN 1..10 OUTBOUND @startid GRAPH "blabla"
  RETURN {v, e}
```

it is not easily possible to return the produced vertices and edges
**as different AQL results in the same stream**.

This suggests, that we want to always explicitly mark which results are
vertices and which are edges, to allow for returning both vertices and
edges **in the same AQL item in the output stream**.

All this considerations have lead to the proposal below.


## Proposal for a graph specification

A "graph loading specification" is a list of lists of AQL queries, each
of which is a pair of a query string and a map of bind parameters. The
meaning is as follows: The outer list is worked on **sequentially**. For
each inner list, the queries can be executed in parallel. Each query has
to return items of the form:

```json
{"vertices":[...], "edges":[...]}
```

and both `vertices` and `edges` attributes are optional. The entries in
the value of the `vertices` attribute are vertices in the form of JSON
objects, which must at least have an `_id` attribute. The entries in the
value of the `edges` attribute are edges and must each have at least
attributes `_from` and `_to`. It is allowed that an edge value is `null`
and is then silently ignored (this is important for the start node of a
graph traversal, where instead of an edge we only have `null`).

It is allowed to produce an item which contains an edge whose end-vertices
only occur later in the same query or in a later (or parallel) query. In that
case the GAE will postpone the insertion of the edge and buffer such
edges. It is advisable to first produce the vertices and then the edges,
since then the GAE has to buffer less data and can immediately convert the 
edges into their memory-efficient memory format.

For efficiency reasons, one should declare the attributes which are returned
for vertices and edges upfront and potentially even prescribe their types,
so that a nice columnar storage format can be used in the GAE.

In the Rust implementation, attributes are specified using the `DataItem` struct
with their data types. The following data types are supported:

- **`Bool`** - Boolean values (accepts booleans, strings like "true"/"false", numbers where 0=false)
- **`String`** - Text strings (accepts any value and converts it to string)
- **`U64`** - Unsigned 64-bit integers (non-negative integers, positive floats are rounded)
- **`I64`** - Signed 64-bit integers (any integer, floats are rounded)
- **`F64`** - 64-bit floating point numbers (any numeric value, must be finite)
- **`JSON`** - Any JSON value (accepts anything as-is without conversion)

Example in Rust:

```rust
let vertex_attributes = vec![
    DataItem::new("name".to_string(), DataType::String),
    DataItem::new("age".to_string(), DataType::U64),
    DataItem::new("score".to_string(), DataType::F64),
    DataItem::new("active".to_string(), DataType::Bool),
];
```

The attribute `_id` for vertices and the attributes `_from` and `_to` for edges
do not have to be specified as they are automatically included.


## How this can be used for the typical use cases above

For the above use case 1 (multiple filtered vertex and edge collections),
this setup can be used as follows:

```json
[ [{"query":"FOR x IN vertices1 FILTER x. ... RETURN {vertices:[x]}", "bindVars":{}},
   {"query":"FOR x IN vertices2 FILTER x. ... RETURN {vertices:[x]}", "bindVars":{}}
  ],
  [{"query":"FOR y IN edges1 FILTER y. ... RETURN {edges:[y]}", "bindVars":{}},
   {"query":"FOR y IN edges2 FILTER y. ... RETURN {edges:[y]}", "bindVars":{}}
  ] ]
```

The parallelisation rules would ensure, that first all vertices from all
vertex collections are loaded concurrently, and then all edges from all
edge collections are loaded concurrently. This means the GAE does not have
to buffer edges.

For the above use case 2 (graph traversals), this setup can be used as follows:

```json
[ [{"query":"FOR s IN @startids1 FOR x, y IN 0..10 OUTBOUND s GRAPH 'blabla' PRUNE ... FILTER ... RETURN {vertices:[x], edges:[y]}",
    "bindVars": {}},
   {"query":"FOR s IN @startids2 FOR x, y IN 0..10 OUTBOUND s GRAPH 'blabla' PRUNE ... FILTER ... RETURN {vertices:[x], edges:[y]}",
    "bindVars": {}} ]
]
```

Note that the depth 0 is possible, in which case `y` is `null` and `x` is the
starting vertex. Furthermore, note that the graph traversals would be run
in parallel. To avoid this, one could use this:

```json
[ [{"query":"FOR s IN @startids1 FOR x, y IN 0..10 OUTBOUND s GRAPH 'blabla' PRUNE ... FILTER ... RETURN {vertices:[x], edges:[y]}"],
    "bindVars": {} }],
  [{"query":"FOR s IN @startids2 FOR x, y IN 0..10 OUTBOUND s GRAPH 'blabla' PRUNE ... FILTER ... RETURN {vertices:[x], edges:[y]}"],
    "bindVars": {} }]
]
```
