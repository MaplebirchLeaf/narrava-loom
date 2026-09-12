//! 安全整数坐标范围内的精确多边形校验与查询。

use std::collections::HashSet;

use crate::{MAX_COORDINATE, Point, WorldError};

type WidePoint = [i128; 2];

pub(crate) fn valid_point(point: Point) -> bool {
    point
        .iter()
        .all(|coordinate: &i64| (-MAX_COORDINATE..=MAX_COORDINATE).contains(coordinate))
}

fn wide(point: Point) -> WidePoint {
    [i128::from(point[0]), i128::from(point[1])]
}

fn doubled(point: Point) -> WidePoint {
    wide(point).map(|coordinate: i128| coordinate * 2)
}

fn orientation(a: WidePoint, b: WidePoint, c: WidePoint) -> i128 {
    (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
}

fn on_segment(a: WidePoint, b: WidePoint, point: WidePoint) -> bool {
    orientation(a, b, point) == 0
        && (a[0].min(b[0])..=a[0].max(b[0])).contains(&point[0])
        && (a[1].min(b[1])..=a[1].max(b[1])).contains(&point[1])
}

fn crosses(a: Point, b: Point, c: Point, d: Point, include_touch: bool) -> bool {
    let [a, b, c, d]: [WidePoint; 4] = [wide(a), wide(b), wide(c), wide(d)];
    let abc: i128 = orientation(a, b, c);
    let abd: i128 = orientation(a, b, d);
    let cda: i128 = orientation(c, d, a);
    let cdb: i128 = orientation(c, d, b);
    if abc.signum() * abd.signum() == -1 && cda.signum() * cdb.signum() == -1 {
        return true;
    }
    include_touch
        && (on_segment(a, b, c)
            || on_segment(a, b, d)
            || on_segment(c, d, a)
            || on_segment(c, d, b))
}

fn edges(bounds: &[Point]) -> impl Iterator<Item = (Point, Point)> + '_ {
    bounds
        .iter()
        .copied()
        .zip(bounds.iter().copied().cycle().skip(1))
}

pub(crate) fn validate_polygon(bounds: &[Point]) -> Result<(), WorldError> {
    let invalid = |reason: &str| WorldError::new("world.invalid_bounds", reason);
    if bounds.len() < 3 {
        return Err(invalid(
            "place bounds require at least three distinct vertices",
        ));
    }
    if bounds.iter().any(|point: &Point| !valid_point(*point)) {
        return Err(invalid(
            "place bounds exceed the safe integer coordinate range",
        ));
    }
    let vertices: HashSet<Point> = bounds.iter().copied().collect();
    if vertices.len() != bounds.len() {
        return Err(invalid(
            "place bounds must not repeat vertices, including the closing vertex",
        ));
    }
    if bounds[2..]
        .iter()
        .all(|point: &Point| orientation(wide(bounds[0]), wide(bounds[1]), wide(*point)) == 0)
    {
        return Err(invalid("place bounds must enclose a nonzero area"));
    }
    for (index, (a, b)) in edges(bounds).enumerate() {
        let previous: Point = bounds[(index + bounds.len() - 1) % bounds.len()];
        if on_segment(wide(previous), wide(a), wide(b))
            || on_segment(wide(a), wide(b), wide(previous))
        {
            return Err(invalid("adjacent polygon edges must not overlap"));
        }
        for (other, (c, d)) in edges(bounds).enumerate().skip(index + 1) {
            if other == index + 1 || (index == 0 && other == bounds.len() - 1) {
                continue;
            }
            if crosses(a, b, c, d, true) {
                return Err(invalid(
                    "place bounds must form a simple polygon without self intersections",
                ));
            }
        }
    }
    Ok(())
}

/// 边界视为内部；坐标倍增可精确表示半整数中点。
fn contains_doubled(bounds: &[Point], point: WidePoint) -> bool {
    let mut winding: i64 = 0;
    for (a, b) in edges(bounds) {
        let a: WidePoint = doubled(a);
        let b: WidePoint = doubled(b);
        if on_segment(a, b, point) {
            return true;
        }
        if a[1] <= point[1] && b[1] > point[1] && orientation(a, b, point) > 0 {
            winding += 1;
        } else if b[1] <= point[1] && a[1] > point[1] && orientation(a, b, point) < 0 {
            winding -= 1;
        }
    }
    winding != 0
}

pub(crate) fn contains(bounds: &[Point], point: Point) -> bool {
    valid_point(point) && contains_doubled(bounds, doubled(point))
}

pub(crate) fn contains_polygon(parent: &[Point], child: &[Point]) -> bool {
    if child.iter().any(|point: &Point| !contains(parent, *point)) {
        return false;
    }
    for (a, b) in edges(child) {
        if edges(parent).any(|(c, d): (Point, Point)| crosses(a, b, c, d, false)) {
            return false;
        }
        // 子边可能穿过凹多边形的顶点；在相交顶点处分段，再逐段判断内外。
        let mut breaks: Vec<Point> = vec![a, b];
        breaks.extend(
            parent
                .iter()
                .copied()
                .filter(|point: &Point| on_segment(wide(a), wide(b), wide(*point))),
        );
        breaks.sort_unstable();
        breaks.dedup();
        if breaks.windows(2).any(|pair: &[Point]| {
            !contains_doubled(
                parent,
                [
                    i128::from(pair[0][0]) + i128::from(pair[1][0]),
                    i128::from(pair[0][1]) + i128::from(pair[1][1]),
                ],
            )
        }) {
            return false;
        }
    }
    true
}
