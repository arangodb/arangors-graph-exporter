use std::cmp::Ordering;

use crate::graph_store::graph::Edge;
use crate::graph_store::vertex_key_index::VertexIndex;

#[derive(Debug)]
pub struct NeighbourIndex {
    pub sorted_neighbours: Vec<VertexIndex>,
    pub vertex_offset: Vec<u64>,
}

// vertex_count has to include at least all vertices inside edges
impl NeighbourIndex {
    pub fn create_from(edges: &mut [Edge], vertices_count: usize) -> Self {
        Self::create(edges, vertices_count, Edge::from, Edge::to)
    }
    pub fn create_to(edges: &mut [Edge], vertices_count: usize) -> Self {
        Self::create(edges, vertices_count, Edge::to, Edge::from)
    }

    pub fn neighbours(&self, source: VertexIndex) -> impl Iterator<Item = &VertexIndex> {
        self.sorted_neighbours[self.vertex_offset[source.to_u64() as usize] as usize
            ..self.vertex_offset[source.to_u64() as usize + 1] as usize]
            .iter()
    }

    pub fn neighbour_count(&self, source: VertexIndex) -> u64 {
        let first_edge = self.vertex_offset[source.to_u64() as usize];
        let last_edge = self.vertex_offset[source.to_u64() as usize + 1];
        last_edge - first_edge
    }

    fn create(
        edges: &mut [Edge],
        vertices_count: usize,
        index_type: fn(&Edge) -> VertexIndex,
        neighbor_type: fn(&Edge) -> VertexIndex,
    ) -> Self {
        edges.sort_by(|a: &Edge, b: &Edge| -> Ordering {
            index_type(a).to_u64().cmp(&index_type(b).to_u64())
        });
        let mut vertex_offset = Vec::<u64>::with_capacity(vertices_count + 1);
        let mut sorted_neighbours = Vec::<VertexIndex>::with_capacity(edges.len());
        let mut current_vertex = VertexIndex::new(0);
        let mut pos: u64 = 0; // position in self.sorted_neighbors where
        // we currently write
        vertex_offset.push(0);
        // loop invariant: pos == sorted_neighbors.len()
        for e in edges.iter() {
            if index_type(e) != current_vertex {
                // fill vertex_offset for all vertices until next
                loop {
                    current_vertex = VertexIndex::new(current_vertex.to_u64() + 1);
                    vertex_offset.push(pos);
                    if current_vertex == index_type(e) {
                        break;
                    }
                }
            }
            sorted_neighbours.push(neighbor_type(e));
            pos += 1;
        }
        // fill vertex_offset
        while current_vertex.to_u64() < vertices_count as u64 {
            current_vertex = VertexIndex::new(current_vertex.to_u64() + 1);
            vertex_offset.push(pos);
        }
        NeighbourIndex {
            sorted_neighbours,
            vertex_offset,
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    mod from_index {
        use super::*;

        #[test]
        fn adds_from_index_and_retrieves_out_vertices_via_function() {
            let mut edges = vec![
                Edge::create(VertexIndex::new(4), VertexIndex::new(1)),
                Edge::create(VertexIndex::new(0), VertexIndex::new(3)),
                Edge::create(VertexIndex::new(0), VertexIndex::new(2)),
                Edge::create(VertexIndex::new(1), VertexIndex::new(6)),
                Edge::create(VertexIndex::new(1), VertexIndex::new(6)),
            ];

            let index = NeighbourIndex::create(&mut edges, 7, Edge::from, Edge::to);

            assert_eq!(
                index.neighbours(VertexIndex::new(0)).collect::<Vec<_>>(),
                vec![&VertexIndex::new(3), &VertexIndex::new(2)]
            );
            assert_eq!(
                index.neighbours(VertexIndex::new(1)).collect::<Vec<_>>(),
                vec![&VertexIndex::new(6), &VertexIndex::new(6)]
            );
            assert_eq!(index.neighbours(VertexIndex::new(2)).count(), 0);
            assert_eq!(index.neighbours(VertexIndex::new(3)).count(), 0);
            assert_eq!(
                index.neighbours(VertexIndex::new(4)).collect::<Vec<_>>(),
                vec![&VertexIndex::new(1)]
            );
        }

        #[test]
        fn counts_outgoing_vertices() {
            let mut edges = vec![
                Edge::create(VertexIndex::new(0), VertexIndex::new(0)),
                Edge::create(VertexIndex::new(0), VertexIndex::new(0)),
            ];

            let index = NeighbourIndex::create(&mut edges, 2, Edge::from, Edge::to);

            assert_eq!(index.neighbour_count(VertexIndex::new(0)), 2);
        }
    }

    mod to_index {
        use super::*;

        #[test]
        fn adds_to_index_and_retrieves_in_vertices_via_function() {
            let mut edges = vec![
                Edge::create(VertexIndex::new(1), VertexIndex::new(4)),
                Edge::create(VertexIndex::new(3), VertexIndex::new(0)),
                Edge::create(VertexIndex::new(2), VertexIndex::new(0)),
                Edge::create(VertexIndex::new(6), VertexIndex::new(1)),
                Edge::create(VertexIndex::new(6), VertexIndex::new(1)),
            ];

            let index = NeighbourIndex::create(&mut edges, 7, Edge::to, Edge::from);

            assert_eq!(
                index.neighbours(VertexIndex::new(0)).collect::<Vec<_>>(),
                vec![&VertexIndex::new(3), &VertexIndex::new(2)]
            );
            assert_eq!(
                index.neighbours(VertexIndex::new(1)).collect::<Vec<_>>(),
                vec![&VertexIndex::new(6), &VertexIndex::new(6)]
            );
            assert_eq!(index.neighbours(VertexIndex::new(2)).count(), 0);
            assert_eq!(index.neighbours(VertexIndex::new(3)).count(), 0);
            assert_eq!(
                index.neighbours(VertexIndex::new(4)).collect::<Vec<_>>(),
                vec![&VertexIndex::new(1)]
            );
        }

        #[test]
        fn counts_outgoing_vertices() {
            let mut edges = vec![
                Edge::create(VertexIndex::new(0), VertexIndex::new(0)),
                Edge::create(VertexIndex::new(0), VertexIndex::new(0)),
            ];

            let index = NeighbourIndex::create(&mut edges, 2, Edge::to, Edge::from);

            assert_eq!(index.neighbour_count(VertexIndex::new(0)), 2);
        }
    }
}
