//! The Ender Dragon's route-finding, shared by the End fight's dragon and
//! by dragons no fight owns (v1.0 Tracks 7 and 7b), after Java 26.3's
//! `EnderDragon` (`findClosestNode`, `findPath`, `reconstructPath`,
//! `nodeAdjacency`), `BinaryHeap` and `Node`, and the phases that fly
//! along a path (`DragonHoldingPatternPhase`, `DragonStrafePlayerPhase`,
//! `DragonLandingApproachPhase`, `DragonTakeoffPhase`).
//!
//! - **Nodes.** 24 fixed points, absolute around (0, 0) (Java never offsets
//!   them by the fight origin): twelve at radius 60, eight at radius 40 and
//!   four at radius 20, each at `max(73, MOTION_BLOCKING_NO_LEAVES height +
//!   5)` (+ 15 for the radius-40 ring), with Java's float `Mth.cos` /
//!   `Mth.sin` table values. A dragon computes them once, the first time it
//!   needs a node.
//! - **Closest node.** The nearest node (float squared distance from the
//!   floored point) closer than 100 blocks; node 0 when none is. With no
//!   fight, or no crystal alive on the pillars, only nodes 12–23 count.
//! - **Paths.** A* over `nodeAdjacency` with Java's binary heap (so ties
//!   break as Java's do), never through nodes below 12 when no crystal is
//!   alive. A path that can't reach its goal ends at the node nearest it;
//!   one that can't leave its start is no path (`null`).

use glam::{DVec3, IVec3};

use crate::physics::BlockSource;

/// `EnderDragon.findClosestNode`'s lowest node height.
pub const MIN_NODE_Y: i32 = 73;

/// `EnderDragon.nodeAdjacency`: which nodes each node links to.
const ADJACENCY: [u32; 24] = [
    6146, 8197, 8202, 16404, 32808, 32848, 65696, 131392, 131712, 263424, 526848, 525313, 1581057, 3166214, 2138120,
    6373424, 4358208, 12910976, 9044480, 9706496, 15216640, 13688832, 11763712, 8257536,
];

/// The top of the motion-blocking column at `(x, z)` with leaves left out
/// (`Heightmap.Types.MOTION_BLOCKING_NO_LEAVES`): the cell above the
/// highest block with collision or fluid, scanning down from `top`.
/// `None` while the column isn't loaded.
pub fn surface<W: BlockSource + ?Sized>(world: &W, x: i32, z: i32, top: i32, floor: i32) -> Option<i32> {
    for y in (floor..=top).rev() {
        let b = world.block(IVec3::new(x, y, z))?;
        if (b.is_solid() || b.is_fluid()) && !b.is_leaves() {
            return Some(y + 1);
        }
    }
    Some(floor)
}

/// Java's node `(x, z)` for node `i` (`Mth.floor(r * Mth.cos(2 * (-PI + step * k)))`).
pub fn node_column(i: usize) -> (i32, i32) {
    use crate::knockback::{mth_cos, mth_sin};
    let (radius, step, k) = match i {
        0..12 => (60.0f32, std::f32::consts::PI / 12.0, i),
        12..20 => (40.0, std::f32::consts::PI / 8.0, i - 12),
        _ => (20.0, std::f32::consts::PI / 4.0, i - 20),
    };
    let a = 2.0f32 * (-std::f32::consts::PI + step * k as f32);
    ((radius * mth_cos(a as f64)).floor() as i32, (radius * mth_sin(a as f64)).floor() as i32)
}

/// The 24 nodes, and whether every column was loaded (the result is kept
/// only then; until it is, unloaded columns sit at the lowest height).
pub fn nodes<W: BlockSource + ?Sized>(world: &W, top: i32, floor: i32) -> ([DVec3; 24], bool) {
    let mut complete = true;
    let nodes = std::array::from_fn(|i| {
        let (x, z) = node_column(i);
        let lift = if (12..20).contains(&i) { 15 } else { 5 };
        let ground = surface(world, x, z, top, floor);
        complete &= ground.is_some();
        let y = ground.map_or(MIN_NODE_Y, |g| (g + lift).max(MIN_NODE_Y));
        DVec3::new(x as f64, y as f64, z as f64)
    });
    (nodes, complete)
}

/// The lowest node index `findClosestNode` and `findPath` use: 12 with no
/// fight or no crystal alive on the pillars, else 0.
pub fn min_node(alive_crystals: u32) -> usize {
    if alive_crystals == 0 { 12 } else { 0 }
}

/// `EnderDragon.findClosestNode(x, y, z)`: the nearest node from `min`
/// up, measured from the floored point in floats, and only if closer than
/// 100 blocks (squared 10000); node 0 otherwise.
pub fn closest_node(nodes: &[DVec3; 24], p: DVec3, min: usize) -> usize {
    let at = p.floor();
    let mut closest = (10000.0f32, 0);
    for (i, node) in nodes.iter().enumerate().skip(min) {
        let d = (*node - at).as_vec3().length_squared();
        if d < closest.0 {
            closest = (d, i);
        }
    }
    closest.1
}

/// `(i - 12) & 7 + 12`: a node index folded onto the eight radius-40 nodes.
pub fn inner_ring(i: i32) -> usize {
    (((i - 12) & 7) + 12) as usize
}

/// Java's `BinaryHeap` over node indices, keyed by `f` (never NaN), so that equal
/// costs come off in the order Java's heap gives them.
struct Heap {
    heap: Vec<usize>,
    /// Each node's place in `heap` (`Node.heapIdx`), -1 when not in it.
    at: [i32; 24],
}

impl Heap {
    fn new() -> Self {
        Self { heap: Vec::with_capacity(24), at: [-1; 24] }
    }

    fn contains(&self, n: usize) -> bool {
        self.at[n] >= 0
    }

    fn insert(&mut self, n: usize, f: &[f32; 24]) {
        self.heap.push(n);
        self.at[n] = self.heap.len() as i32 - 1;
        self.up(self.heap.len() - 1, f);
    }

    fn pop(&mut self, f: &[f32; 24]) -> usize {
        let popped = self.heap[0];
        let last = self.heap.pop().unwrap();
        if !self.heap.is_empty() {
            self.heap[0] = last;
            self.down(0, f);
        }
        self.at[popped] = -1;
        popped
    }

    /// `changeCost`: `f[n]` already holds the new cost, `old` the previous one.
    fn change_cost(&mut self, n: usize, old: f32, f: &[f32; 24]) {
        let i = self.at[n] as usize;
        if f[n] < old { self.up(i, f) } else { self.down(i, f) }
    }

    fn up(&mut self, mut i: usize, f: &[f32; 24]) {
        let n = self.heap[i];
        let cost = f[n];
        while i > 0 {
            let parent = (i - 1) >> 1;
            let p = self.heap[parent];
            if cost >= f[p] {
                break;
            }
            self.heap[i] = p;
            self.at[p] = i as i32;
            i = parent;
        }
        self.heap[i] = n;
        self.at[n] = i as i32;
    }

    fn down(&mut self, mut i: usize, f: &[f32; 24]) {
        let n = self.heap[i];
        let cost = f[n];
        loop {
            let left = 1 + (i << 1);
            let right = left + 1;
            if left >= self.heap.len() {
                break;
            }
            let left_cost = f[self.heap[left]];
            let right_cost = self.heap.get(right).map_or(f32::INFINITY, |&r| f[r]);
            let next = if left_cost < right_cost {
                if left_cost >= cost {
                    break;
                }
                left
            } else {
                if right_cost >= cost {
                    break;
                }
                right
            };
            let m = self.heap[next];
            self.heap[i] = m;
            self.at[m] = i as i32;
            i = next;
        }
        self.heap[i] = n;
        self.at[n] = i as i32;
    }
}

/// `EnderDragon.findPath(from, to, last)`, already advanced past `from`
/// as every caller does: the nodes to fly through, next last (so `pop`
/// gives it), with `last` (Java's `finalNode`) at the end of the route.
/// Empty when the path is done at once; `None` when Java's is `null` (no
/// node nearer the goal could be reached).
pub fn find_path(nodes: &[DVec3; 24], from: usize, to: usize, min: usize, last: Option<DVec3>) -> Option<Vec<DVec3>> {
    // `Node.distanceTo`: float distance between whole-block positions.
    let dist = |a: usize, b: usize| (nodes[a] - nodes[b]).as_vec3().length();
    let mut g = [0.0f32; 24];
    let mut f = [0.0f32; 24];
    let mut came = [usize::MAX; 24];
    let mut closed = [false; 24];
    let mut open = Heap::new();
    f[from] = dist(from, to);
    open.insert(from, &f);
    let mut closest = from;
    let mut end = None;
    while !open.heap.is_empty() {
        let node = open.pop(&f);
        if node == to {
            end = Some(to);
            break;
        }
        if dist(node, to) < dist(closest, to) {
            closest = node;
        }
        closed[node] = true;
        for next in min..24 {
            if ADJACENCY[node] & (1 << next) == 0 || closed[next] {
                continue;
            }
            let tentative = g[node] + dist(node, next);
            if !open.contains(next) || tentative < g[next] {
                came[next] = node;
                g[next] = tentative;
                let old = f[next];
                f[next] = tentative + dist(next, to);
                if open.contains(next) {
                    open.change_cost(next, old, &f);
                } else {
                    open.insert(next, &f);
                }
            }
        }
    }
    let end = match end {
        Some(end) => end,
        None if closest == from => return None,
        None => closest,
    };
    let mut path: Vec<DVec3> = last.into_iter().collect();
    let mut at = end;
    while at != from {
        path.push(nodes[at]);
        at = came[at];
    }
    Some(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::block::Block;

    /// Stone up to y = 64, air above.
    struct Ground;
    impl BlockSource for Ground {
        fn block(&self, p: IVec3) -> Option<Block> {
            Some(if p.y < 64 { Block::STONE } else { Block::AIR })
        }
    }

    #[test]
    fn nodes_follow_javas_rings() {
        // `findClosestNode`: radius 60, 40 and 20, at `max(73, ground + 5/15/5)`.
        assert_eq!(node_column(0), (60, 0));
        assert_eq!(node_column(6), (-60, 0));
        assert_eq!(node_column(12), (40, 0));
        assert_eq!(node_column(20), (20, 0));
        let (nodes, complete) = nodes(&Ground, 320, -64);
        assert!(complete);
        assert_eq!(nodes[0].y, 73.0, "ground 64 + 5 is under the floor of 73");
        assert_eq!(nodes[12].y, 79.0, "the middle ring is 15 over the ground");
        assert_eq!(nodes[20].y, 73.0);
    }

    #[test]
    fn closest_nodes_count_only_within_100_blocks() {
        let (nodes, _) = nodes(&Ground, 320, -64);
        assert_eq!(closest_node(&nodes, DVec3::new(59.9, 80.0, 0.5), 0), 0);
        assert_eq!(closest_node(&nodes, DVec3::new(59.9, 80.0, 0.5), 12), 12, "only inner nodes with no crystals");
        assert_eq!(closest_node(&nodes, DVec3::new(19.0, 70.0, 1.0), 12), 20);
        // Java starts from node 0 at a squared distance of 10000: from far
        // away, even a search from node 12 up gives node 0.
        assert_eq!(closest_node(&nodes, DVec3::new(300.0, 100.0, -200.0), 12), 0);
    }

    #[test]
    fn paths_follow_the_links_and_end_at_the_final_node() {
        let (nodes, _) = nodes(&Ground, 320, -64);
        // Between two inner nodes with no crystals, only the inner nodes.
        let path = find_path(&nodes, 12, 16, 12, None).unwrap();
        assert!(!path.is_empty());
        assert!(path.iter().all(|p| nodes[12..].contains(p)));
        assert_eq!(path.first(), Some(&nodes[16]), "the goal is popped last");
        // Along the outer ring, one step is one node.
        assert_eq!(find_path(&nodes, 0, 1, 0, None).unwrap(), vec![nodes[1]]);
        // A final node goes after the goal.
        let egg = DVec3::new(0.0, 65.0, 0.0);
        let path = find_path(&nodes, 12, 20, 12, Some(egg)).unwrap();
        assert_eq!(path.first(), Some(&egg));
        assert_eq!(path.last(), Some(&nodes[20]));
        // Already there: done at once, or straight to the final node.
        assert_eq!(find_path(&nodes, 12, 12, 12, None), Some(Vec::new()));
        assert_eq!(find_path(&nodes, 12, 12, 12, Some(egg)), Some(vec![egg]));
        // An outer goal with no crystals can't be reached: the path ends at
        // the inner node nearest it.
        let path = find_path(&nodes, 14, 0, 12, None).unwrap();
        let end = path.first().unwrap();
        assert!(nodes[12..].contains(end));
        assert_eq!(*end, nodes[12], "node 12 (40, 0) is the inner node nearest node 0 (60, 0)");
        // From the outer ring with no crystals, node 0 still links to node 12.
        assert_eq!(find_path(&nodes, 0, 12, 12, None), Some(vec![nodes[12]]));
    }

    #[test]
    fn the_heap_breaks_ties_as_javas_does() {
        let mut f = [0.0f32; 24];
        let mut heap = Heap::new();
        for (n, cost) in [(3, 5.0), (7, 5.0), (1, 2.0), (9, 5.0), (4, 1.0)] {
            f[n] = cost;
            heap.insert(n, &f);
        }
        // Java's heap: insert 3, 7 (no swap on equal), 1 (to the top),
        // 9, 4 (to the top); pops 4, 1, then the equal fives as the sift
        // leaves them.
        let order: Vec<usize> = (0..5).map(|_| heap.pop(&f)).collect();
        assert_eq!(order, vec![4, 1, 9, 3, 7]);
    }
}
