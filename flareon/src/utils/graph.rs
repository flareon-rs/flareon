use thiserror::Error;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Error)]
#[error("Cycle detected in the graph")]
pub struct CycleDetected;

pub fn apply_permutation<T>(items: &mut [T], order: &mut [usize]) {
    for i in 0..order.len() {
        let mut current = i;
        let mut next = order[current];

        while next != i {
            // process the cycle
            items.swap(current, next);
            order[current] = current;

            current = next;
            next = order[current];
        }

        order[current] = current;
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct Graph {
    vertex_edges: Vec<Vec<usize>>,
}

impl Graph {
    #[must_use]
    pub(crate) fn new(vertex_num: usize) -> Self {
        Self {
            vertex_edges: vec![Vec::new(); vertex_num],
        }
    }

    pub(crate) fn add_edge(&mut self, from: usize, to: usize) {
        self.vertex_edges[from].push(to);
    }

    #[must_use]
    pub(crate) fn vertex_num(&self) -> usize {
        self.vertex_edges.len()
    }

    pub(crate) fn toposort(&mut self) -> Result<Vec<usize>, CycleDetected> {
        let mut visited = vec![VisitedStatus::NotVisited; self.vertex_num()];
        let mut sorted_indices_stack = Vec::with_capacity(self.vertex_num());

        for index in (0..self.vertex_num()).rev() {
            self.toposort_visit(index, &mut visited, &mut sorted_indices_stack)?;
        }

        assert_eq!(sorted_indices_stack.len(), self.vertex_num());

        sorted_indices_stack.reverse();
        Ok(sorted_indices_stack)
    }

    fn toposort_visit(
        &self,
        index: usize,
        visited: &mut Vec<VisitedStatus>,
        sorted_indices_stack: &mut Vec<usize>,
    ) -> Result<(), CycleDetected> {
        match visited[index] {
            VisitedStatus::Visited => return Ok(()),
            VisitedStatus::Visiting => {
                return Err(CycleDetected);
            }
            VisitedStatus::NotVisited => {}
        }

        visited[index] = VisitedStatus::Visiting;

        for &neighbor in &self.vertex_edges[index] {
            self.toposort_visit(neighbor, visited, sorted_indices_stack)?;
        }

        visited[index] = VisitedStatus::Visited;
        sorted_indices_stack.push(index);

        Ok(())
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
enum VisitedStatus {
    NotVisited,
    Visiting,
    Visited,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graph_toposort_stable() {
        let mut graph = Graph::new(8);
        let sorted_indices = graph.toposort().unwrap();
        assert_eq!(sorted_indices, vec![0, 1, 2, 3, 4, 5, 6, 7]);
    }

    #[test]
    fn graph_toposort() {
        let mut graph = Graph::new(8);
        graph.add_edge(5, 3);
        graph.add_edge(1, 3);
        graph.add_edge(1, 2);
        graph.add_edge(4, 2);
        graph.add_edge(4, 6);
        graph.add_edge(3, 0);
        graph.add_edge(3, 7);
        graph.add_edge(3, 6);
        graph.add_edge(2, 7);

        let sorted_indices = graph.toposort().unwrap();

        assert_eq!(sorted_indices, vec![1, 4, 2, 5, 3, 0, 6, 7]);
    }

    #[test]
    fn graph_toposort_with_cycle() {
        let mut graph = Graph::new(4);
        graph.add_edge(0, 1);
        graph.add_edge(1, 2);
        graph.add_edge(2, 3);
        graph.add_edge(3, 0);

        assert!(matches!(graph.toposort(), Err(CycleDetected)));
    }
}
