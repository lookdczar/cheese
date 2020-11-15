use cgmath::{MetricSpace, Point2};
use ordered_float::OrderedFloat;
use spade::{
    delaunay::{
        CdtEdge, ConstrainedDelaunayTriangulation, FaceHandle, FixedVertexHandle,
        PositionInTriangulation, VertexHandle,
    },
    kernels::FloatKernel,
};
use std::hash::{Hash, Hasher};
use ultraviolet::Vec2;

pub struct MapHandle {
    top_left: FixedVertexHandle,
    top_right: FixedVertexHandle,
    bottom_left: FixedVertexHandle,
    bottom_right: FixedVertexHandle,
}

pub struct Map {
    dlt: ConstrainedDelaunayTriangulation<Point2<f32>, FloatKernel>,
}

impl Map {
    pub fn new() -> Self {
        let mut this = Self {
            dlt: ConstrainedDelaunayTriangulation::with_tree_locate(),
        };

        this.insert(Vec2::new(0.0, 0.0), Vec2::new(200.0, 200.0));

        this
    }

    pub fn edges(&self) -> impl Iterator<Item = (Vec2, Vec2, bool)> + '_ {
        self.dlt.edges().map(move |edge| {
            let from = point_to_vec2(*edge.from());
            let to = point_to_vec2(*edge.to());
            let is_constraint = self.dlt.is_constraint_edge(edge.fix());
            (from, to, is_constraint)
        })
    }

    fn locate(&self, point: Vec2) -> Option<TriangleRef> {
        match self.dlt.locate(&Point2::new(point.x, point.y)) {
            PositionInTriangulation::InTriangle(triangle) => Some(TriangleRef::new(triangle)),
            // These two seem very unlikely.
            PositionInTriangulation::OnPoint(_) => None,
            PositionInTriangulation::OnEdge(_) => None,
            PositionInTriangulation::OutsideConvexHull(_) => None,
            PositionInTriangulation::NoTriangulationPresent => None,
        }
    }

    pub fn insert(&mut self, center: Vec2, dimensions: Vec2) -> MapHandle {
        let tl = center - dimensions / 2.0;
        let br = center + dimensions / 2.0;

        let top_left = self.dlt.insert(Point2::new(tl.x, tl.y));
        let top_right = self.dlt.insert(Point2::new(br.x, tl.y));
        let bottom_left = self.dlt.insert(Point2::new(tl.x, br.y));
        let bottom_right = self.dlt.insert(Point2::new(br.x, br.y));

        self.dlt.add_constraint(top_left, top_right);
        self.dlt.add_constraint(bottom_left, bottom_right);
        self.dlt.add_constraint(top_left, bottom_left);
        self.dlt.add_constraint(top_right, bottom_right);

        MapHandle {
            top_left,
            top_right,
            bottom_left,
            bottom_right,
        }
    }

    pub fn remove(&mut self, handle: &MapHandle) {
        self.dlt.remove(handle.bottom_right);
        self.dlt.remove(handle.bottom_left);
        self.dlt.remove(handle.top_right);
        self.dlt.remove(handle.top_left);
    }

    pub fn pathfind(
        &self,
        start: Vec2,
        end: Vec2,
        unit_radius: f32,
        debug_triangles: Option<&mut Vec<(Vec2, Vec2, Vec2)>>,
        debug_funnel_portals: Option<&mut Vec<(Vec2, Vec2)>>,
    ) -> Option<Vec<Vec2>> {
        let start_tri = self.locate(start)?;
        let end_tri = self.locate(end)?;

        let (triangles, _length) = pathfinding::directed::astar::astar(
            &start_tri,
            |&tri| tri.neighbours(self, unit_radius * 2.0),
            |&tri| tri.distance(&end_tri),
            |&tri| tri == end_tri,
        )?;

        if let Some(debug_triangles) = debug_triangles {
            debug_triangles.clear();
            debug_triangles.extend(triangles.iter().map(|tri| tri.points()))
        }

        // If the two points are in the same triangle, just go right to the end.
        if triangles.len() == 1 {
            return Some(vec![end]);
        }

        let funnel_portals = funnel_portals(start, end, unit_radius, &triangles, self);

        if let Some(debug_funnel_portals) = debug_funnel_portals {
            debug_funnel_portals.clear();
            debug_funnel_portals.extend_from_slice(&funnel_portals);
        }

        Some(funnel(&funnel_portals))
    }

    fn offset_by_normal(&self, vertex: Vertex, offset: f32) -> Vec2 {
        // Sum up the lengths of all constraint edges that connect to the vertex
        let sum = vertex
            .ccw_out_edges()
            .filter(|edge| self.dlt.is_constraint_edge(edge.fix()))
            .fold(cgmath::Point2::new(0.0, 0.0), |normal, edge| {
                let edge_delta = *edge.from() - *edge.to();
                normal + edge_delta
            });

        // Normalize them into a normal pointing away from the edge.
        let normal = point_to_vec2(sum).normalized();

        point_to_vec2(*vertex) + (normal * offset)
    }
}

// Construct the 'portals' for a funnel.
// This funnel is a set of left and right points that are esseentially the range of where a path
// could go.
fn funnel_portals(
    start: Vec2,
    end: Vec2,
    unit_radius: f32,
    triangles: &[TriangleRef],
    map: &Map,
) -> Vec<(Vec2, Vec2)> {
    let mut portals = Vec::new();

    // Push the starting point
    portals.push((start, start));

    // Find the edge between the first and second triangles.
    let (mut latest_left, mut latest_right) = triangles[0].shared_edge(&triangles[1]).unwrap();

    // Push those points, but with an offset decided by the unit radius.
    portals.push((
        map.offset_by_normal(latest_left, unit_radius),
        map.offset_by_normal(latest_right, unit_radius),
    ));

    // Push all the middle points
    for i in 1..triangles.len() - 1 {
        let new_point = triangles[i]
            .opposite_point(latest_left, latest_right)
            .unwrap();

        if triangles[i + 1].contains(latest_left) {
            latest_right = new_point;
        } else {
            latest_left = new_point;
        }

        portals.push((
            map.offset_by_normal(latest_left, unit_radius),
            map.offset_by_normal(latest_right, unit_radius),
        ));
    }

    // Push the end point.
    portals.push((end, end));

    portals
}

fn triarea2(a: Vec2, b: Vec2, c: Vec2) -> f32 {
    let ax = b.x - a.x;
    let ay = b.y - a.y;
    let bx = c.x - a.x;
    let by = c.y - a.y;

    let area = bx * ay - ax * by;
    // We need to invert this for some reason.
    -area
}

pub fn funnel(portals: &[(Vec2, Vec2)]) -> Vec<Vec2> {
    // Implementation of the Simple Stupid Funnel Algorithm
    // http://digestingduck.blogspot.com/2010/03/simple-stupid-funnel-algorithm.html
    let (mut portal_left, mut portal_right) = portals[0];
    let mut portal_apex = portal_left;

    let mut points = vec![];

    let mut left_index = 0;
    let mut right_index = 0;

    let mut i = 1;

    while i < portals.len() {
        let (left, right) = portals[i];

        // Update right vertex
        if triarea2(portal_apex, portal_right, right) <= 0.0 {
            if portal_apex == portal_right || triarea2(portal_apex, portal_left, right) > 0.0 {
                // Tighten the funnel
                portal_right = right;
                right_index = i;
            } else {
                // Right over left, insert left to path and restart scan from portal left point.
                points.push(portal_left);

                // Make current left the new apex
                portal_apex = portal_left;
                let apex_index = left_index;

                // Reset portal
                portal_left = portal_apex;
                portal_right = portal_apex;
                left_index = apex_index;
                right_index = apex_index;

                // Reset scan
                i = apex_index + 1;
                continue;
            }
        }

        // Update left vertex
        if triarea2(portal_apex, portal_left, left) >= 0.0 {
            if portal_apex == portal_left || triarea2(portal_apex, portal_right, left) < 0.0 {
                // Tighten the funnel
                portal_left = left;
                left_index = i;
            } else {
                // Left over right, insert right to path and restart scan from portal right point.
                points.push(portal_right);

                // Make current right the new apex
                portal_apex = portal_right;
                let apex_index = right_index;

                // Reset portal
                portal_left = portal_apex;
                portal_right = portal_apex;
                left_index = apex_index;
                right_index = apex_index;

                // Reset scan
                i = apex_index + 1;
                continue;
            }
        }

        i += 1;
    }

    let end_point = portals[portals.len() - 1].0;

    if points[points.len() - 1] != end_point {
        points.push(end_point);
    }

    points
}

fn point_to_vec2(point: Point2<f32>) -> Vec2 {
    Vec2::new(point.x, point.y)
}

type Vertex<'a> = VertexHandle<'a, Point2<f32>, CdtEdge>;

#[derive(Debug, Clone, Copy, PartialEq)]
struct TriangleRef<'a> {
    a: Vertex<'a>,
    b: Vertex<'a>,
    c: Vertex<'a>,
}

impl<'a> TriangleRef<'a> {
    fn new(face: FaceHandle<'a, Point2<f32>, CdtEdge>) -> Self {
        let [a, b, c] = face.as_triangle();
        Self { a, b, c }
    }

    fn points(&self) -> (Vec2, Vec2, Vec2) {
        (
            point_to_vec2(*self.a),
            point_to_vec2(*self.b),
            point_to_vec2(*self.c),
        )
    }

    fn center(&self) -> Vec2 {
        Vec2::new(
            self.a.x + self.b.x + self.c.x,
            self.a.y + self.b.y + self.c.y,
        ) / 3.0
    }

    fn distance(&self, other: &Self) -> OrderedFloat<f32> {
        let vector = self.center() - other.center();
        OrderedFloat(vector.mag())
    }

    fn neighbours<'b>(
        &self,
        map: &'b Map,
        gap: f32,
    ) -> impl Iterator<Item = (TriangleRef<'b>, OrderedFloat<f32>)> {
        let center = self.center();

        arrayvec::ArrayVec::from([
            edge_tuple(self.a, self.b),
            edge_tuple(self.b, self.c),
            edge_tuple(self.c, self.a),
        ])
        .into_iter()
        .filter_map(move |(a, b, distance_sq)| {
            // Flipped here because we want the edge facing outside.
            let edge = map.dlt.get_edge_from_neighbors(b, a).unwrap();

            let face = edge.face();

            if !map.dlt.is_constraint_edge(edge.fix())
                && gap.powi(2) <= distance_sq
                && face != map.dlt.infinite_face()
            {
                let triangle = TriangleRef::new(face);
                let distance = (center - triangle.center()).mag();
                Some((triangle, OrderedFloat(distance)))
            } else {
                None
            }
        })
    }

    fn contains(&self, point: Vertex) -> bool {
        self.a == point || self.b == point || self.c == point
    }

    fn shared_edge(&self, other: &Self) -> Option<(Vertex, Vertex)> {
        for (a, b) in [(self.a, self.b), (self.b, self.c), (self.c, self.a)].iter() {
            if other.contains(*a) && other.contains(*b) {
                return Some((*a, *b));
            }
        }

        None
    }

    fn opposite_point(&self, a: Vertex, b: Vertex) -> Option<Vertex> {
        for point in [self.a, self.b, self.c].iter() {
            if *point != a && *point != b {
                return Some(*point);
            }
        }

        None
    }
}

fn edge_tuple(a: Vertex, b: Vertex) -> (FixedVertexHandle, FixedVertexHandle, f32) {
    (a.fix(), b.fix(), a.distance2(*b))
}

impl<'a> Eq for TriangleRef<'a> {}

impl<'a> Hash for TriangleRef<'a> {
    fn hash<H: Hasher>(&self, hasher: &mut H) {
        hash_point(*self.a, hasher);
        hash_point(*self.b, hasher);
        hash_point(*self.c, hasher);
    }
}

fn hash_point<H: Hasher>(point: Point2<f32>, hasher: &mut H) {
    ordered_float::OrderedFloat(point.x).hash(hasher);
    ordered_float::OrderedFloat(point.y).hash(hasher);
}