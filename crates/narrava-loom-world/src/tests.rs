use super::*;

fn square(id: &str, parent: Option<&str>, low: i64, high: i64) -> Place {
    Place {
        id: id.to_owned(),
        name: None,
        parent: parent.map(str::to_owned),
        bounds: vec![[low, low], [high, low], [high, high], [low, high]],
        entry: None,
    }
}

#[test]
fn registration_accepts_forward_parents_and_uses_ids() {
    let mut world: World = World::default();
    let mut hospital: Place = square("town-hospital", Some("town"), 2, 8);
    hospital.name = Some("医院".to_owned());
    world.add(hospital).unwrap();
    assert!(world.validate().is_err());
    world.add(square("town", None, 0, 10)).unwrap();
    world.validate().unwrap();
    assert_eq!(
        world.get("town-hospital").unwrap().name.as_deref(),
        Some("医院")
    );
    assert!(world.get("医院").is_none());
    assert_eq!(world.places().len(), 2);
    assert_eq!(
        world
            .ancestors("town-hospital")
            .unwrap()
            .iter()
            .map(|place| place.id.as_str())
            .collect::<Vec<&str>>(),
        vec!["town", "town-hospital"]
    );
}

#[test]
fn registration_rejects_reserved_and_invalid_ids_without_replacing_places() {
    let mut world: World = World::default();
    for id in [
        "",
        " ",
        "town hospital",
        "医院",
        "1town",
        "town/hospital",
        "inside",
        "outside",
        "exit",
        "Start",
        "StoryInit",
        "Header",
        "Footer",
        "Bar",
        "BarStowed",
    ] {
        assert!(world.add(square(id, None, 0, 10)).is_err(), "{id}");
    }
    world
        .add(square("Town.hospital_1-wing", None, 0, 10))
        .unwrap();
    assert!(
        world
            .add(square("Town.hospital_1-wing", None, 20, 30))
            .is_err()
    );
    assert_eq!(world.get("Town.hospital_1-wing").unwrap().bounds[0], [0, 0]);
}

#[test]
fn polygons_reject_degeneracy_intersections_and_unsafe_coordinates() {
    let invalid: Vec<Vec<Point>> = vec![
        vec![[0, 0], [1, 1]],
        vec![[0, 0], [1, 1], [2, 2]],
        vec![[0, 0], [4, 0], [4, 4], [0, 0]],
        vec![[0, 0], [4, 4], [0, 4], [4, 0]],
        vec![[0, 0], [4, 0], [2, 0], [2, 4], [0, 4]],
        vec![[0, 0], [4, 0], [4, 4], [2, 0], [0, 4]],
        vec![[0, 0], [MAX_COORDINATE + 1, 0], [0, 1]],
    ];
    for bounds in invalid {
        let mut world: World = World::default();
        let mut place: Place = square("town", None, 0, 10);
        place.bounds = bounds;
        assert!(world.add(place).is_err());
        assert_eq!(world.places().len(), 0);
    }
}

#[test]
fn geometry_accepts_concavity_collinear_edges_and_boundary_points() {
    let mut world: World = World::default();
    let mut place: Place = square("town", None, 0, 10);
    place.bounds = vec![[0, 0], [2, 0], [4, 0], [4, 4], [2, 2], [0, 4]];
    place.entry = Some([2, 2]);
    world.add(place).unwrap();
    world.validate().unwrap();
    assert_eq!(world.locate([2, 2]).len(), 1);
    assert_eq!(world.locate([1, 1]).len(), 1);
    assert!(world.locate([2, 3]).is_empty());
    assert!(world.locate([MAX_COORDINATE + 1, 0]).is_empty());
}

#[test]
fn geometry_is_exact_near_safe_integer_limits() {
    let mut world: World = World::default();
    world
        .add(square("world", None, -MAX_COORDINATE, MAX_COORDINATE))
        .unwrap();
    assert_eq!(world.locate([MAX_COORDINATE, MAX_COORDINATE]).len(), 1);
    assert_eq!(world.locate([-MAX_COORDINATE, MAX_COORDINATE]).len(), 1);
    assert_eq!(world.locate([0, 0]).len(), 1);
}

#[test]
fn movement_supports_negative_places_and_crossing_the_origin() {
    let mut world: World = World::default();
    world.add(square("town", None, -10, 10)).unwrap();
    let mut cellar: Place = square("cellar", Some("town"), -8, -2);
    cellar.entry = Some([-6, -4]);
    world.add(cellar).unwrap();
    world.validate().unwrap();

    let mut state: WorldState = WorldState::default();
    world.enter(&mut state, "cellar", None).unwrap();
    assert_eq!(state.position.as_ref().unwrap().point, [-6, -4]);
    assert_eq!(world.locate([-6, -4]).len(), 2);
    world.enter(&mut state, "town", None).unwrap();
    for point in [[0, 0], [-3, 2], [4, -1]] {
        assert!(world.move_to(&mut state, point).unwrap());
        assert_eq!(state.position.as_ref().unwrap().point, point);
        world.validate_state(&state).unwrap();
    }
}

#[test]
fn negative_safe_integer_boundary_is_inclusive_without_signed_overflow() {
    let mut world: World = World::default();
    world
        .add(square("world", None, -MAX_COORDINATE, MAX_COORDINATE))
        .unwrap();
    let mut state: WorldState = WorldState::default();
    world.enter(&mut state, "world", None).unwrap();
    assert_eq!(
        state.position.as_ref().unwrap().point,
        [-MAX_COORDINATE, -MAX_COORDINATE]
    );
    world.validate_state(&state).unwrap();
    let before: WorldState = state.clone();

    for coordinate in [-MAX_COORDINATE - 1, i64::MIN] {
        let point: Point = [coordinate, 0];
        let invalid: Place = square("invalid", None, coordinate, 0);
        assert_eq!(
            world.add(invalid).unwrap_err().code(),
            "world.invalid_bounds"
        );
        assert!(world.locate(point).is_empty());
        assert_eq!(
            world.move_to(&mut state, point).unwrap_err().code(),
            "world.invalid_position"
        );
        assert_eq!(state, before);
        let invalid: WorldState = WorldState {
            position: Some(WorldPosition {
                place: "world".to_owned(),
                point,
                environment: None,
            }),
        };
        assert_eq!(
            world.validate_state(&invalid).unwrap_err().code(),
            "world.invalid_position"
        );
    }
}

#[test]
fn validation_rejects_parent_cycles_and_child_edges_outside_concave_parent() {
    let mut cycle: World = World::default();
    cycle.add(square("a", Some("b"), 0, 10)).unwrap();
    cycle.add(square("b", Some("a"), 0, 10)).unwrap();
    assert!(cycle.validate().is_err());
    assert!(cycle.ancestors("a").is_err());

    let mut world: World = World::default();
    let mut parent: Place = square("town", None, 0, 10);
    parent.bounds = vec![
        [0, 0],
        [6, 0],
        [6, 6],
        [4, 6],
        [4, 2],
        [2, 2],
        [2, 6],
        [0, 6],
    ];
    world.add(parent).unwrap();
    let mut child: Place = square("hospital", Some("town"), 0, 1);
    // 顶点均在父区域内，但上边穿过父区域的凹口。
    child.bounds = vec![[1, 1], [5, 1], [5, 3], [1, 3]];
    world.add(child).unwrap();
    assert!(world.validate().is_err());
}

#[test]
fn child_edges_cannot_escape_through_parent_vertices() {
    let mut world: World = World::default();
    let mut parent: Place = square("town", None, 0, 10);
    parent.bounds = vec![
        [0, 0],
        [6, 0],
        [6, 6],
        [4, 6],
        [4, 3],
        [3, 2],
        [2, 3],
        [2, 6],
        [0, 6],
    ];
    world.add(parent).unwrap();
    let mut child: Place = square("hospital", Some("town"), 0, 1);
    child.bounds = vec![[1, 1], [5, 1], [5, 3], [1, 3]];
    world.add(child).unwrap();
    assert!(world.validate().is_err());
}

#[test]
fn locate_is_stable_parent_first_and_retains_overlaps() {
    let mut world: World = World::default();
    world.add(square("shop", Some("town"), 4, 9)).unwrap();
    world.add(square("hospital", Some("town"), 1, 8)).unwrap();
    world.add(square("town", None, 0, 10)).unwrap();
    world.validate().unwrap();
    assert_eq!(
        world
            .locate([5, 5])
            .iter()
            .map(|place| place.id.as_str())
            .collect::<Vec<&str>>(),
        vec!["town", "hospital", "shop"]
    );
    assert_eq!(
        world
            .locate([10, 10])
            .iter()
            .map(|place| place.id.as_str())
            .collect::<Vec<&str>>(),
        vec!["town"]
    );
}

#[test]
fn enter_preserves_same_place_position_and_movement_rejects_without_mutation() {
    let mut world: World = World::default();
    world.add(square("town", None, 0, 10)).unwrap();
    let mut hospital: Place = square("hospital", Some("town"), 2, 8);
    hospital.entry = Some([3, 3]);
    world.add(hospital).unwrap();
    let mut state: WorldState = WorldState::default();
    assert!(world.move_to(&mut state, [1, 1]).is_err());
    assert!(
        world
            .enter(&mut state, "town", Some(Environment::Outside))
            .unwrap()
    );
    assert_eq!(state.position.as_ref().unwrap().point, [0, 0]);
    assert!(world.move_to(&mut state, [5, 5]).unwrap());
    assert!(!world.enter(&mut state, "town", None).unwrap());
    assert_eq!(state.position.as_ref().unwrap().point, [5, 5]);
    assert_eq!(
        state.position.as_ref().unwrap().environment,
        Some(Environment::Outside)
    );
    assert!(
        world
            .enter(&mut state, "town", Some(Environment::Inside))
            .unwrap()
    );
    assert!(world.enter(&mut state, "hospital", None).unwrap());
    assert_eq!(state.position.as_ref().unwrap().point, [3, 3]);
    assert_eq!(state.position.as_ref().unwrap().environment, None);
    let before: WorldState = state.clone();
    assert!(world.move_to(&mut state, [0, 0]).is_err());
    assert!(world.enter(&mut state, "missing", None).is_err());
    assert_eq!(state, before);
    assert!(!world.move_to(&mut state, [3, 3]).unwrap());
    world.validate_state(&state).unwrap();
}

#[test]
fn state_and_entry_validation_include_boundaries_and_reject_invalid_locations() {
    let mut world: World = World::default();
    let mut invalid: Place = square("invalid", None, 0, 10);
    invalid.entry = Some([11, 5]);
    assert!(world.add(invalid).is_err());
    world.add(square("town", None, 0, 10)).unwrap();
    world.validate_state(&WorldState::default()).unwrap();
    for (id, point) in [
        ("missing", [1, 1]),
        ("town", [11, 1]),
        ("town", [MAX_COORDINATE + 1, 1]),
    ] {
        assert!(
            world
                .validate_state(&WorldState {
                    position: Some(WorldPosition {
                        place: id.to_owned(),
                        point,
                        environment: None
                    })
                })
                .is_err()
        );
    }
    world
        .validate_state(&WorldState {
            position: Some(WorldPosition {
                place: "town".to_owned(),
                point: [10, 10],
                environment: Some(Environment::Inside),
            }),
        })
        .unwrap();
}
