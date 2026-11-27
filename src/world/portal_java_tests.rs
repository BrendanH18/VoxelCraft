//! v1.0 gap Track 5 (GEN-04c): portal travel against Java 26.3's own code,
//! `tests/fixtures/java/26.3/portals.json` (`tools/extract_java_portal_fixtures.py`
//! runs `tools/JavaPortalVectors.java` against the jar).

use std::collections::HashSet;

use glam::{DVec3, IVec3};
use serde_json::Value;

use super::*;

fn fixture() -> &'static Value {
    static DATA: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
    DATA.get_or_init(|| {
        serde_json::from_str(include_str!("../../tests/fixtures/java/26.3/portals.json")).expect("portal fixture")
    })
}

fn ivec(v: &Value) -> IVec3 {
    let a: Vec<i32> = v.as_array().unwrap().iter().map(|n| n.as_i64().unwrap() as i32).collect();
    IVec3::new(a[0], a[1], a[2])
}

fn dvec(v: &Value) -> DVec3 {
    let a: Vec<f64> = v.as_array().unwrap().iter().map(|n| n.as_f64().unwrap()).collect();
    DVec3::new(a[0], a[1], a[2])
}

#[test]
fn search_radii_and_coordinate_scales_are_javas() {
    let constants = &fixture()["constants"];
    assert_eq!(constants["nether_portal_radius"], NETHER_PORTAL_RADIUS);
    assert_eq!(constants["overworld_portal_radius"], OVERWORLD_PORTAL_RADIUS);
    let dims = &fixture()["dimension_types"];
    // `DimensionType.getTeleportationScale`: the Overworld's scale over the Nether's.
    let scale = dims["the_nether"]["coordinate_scale"].as_f64().unwrap()
        / dims["overworld"]["coordinate_scale"].as_f64().unwrap();
    assert_eq!(scale, 8.0);
    // `createPortal`'s highest frame: the Nether's logical height.
    assert_eq!(dims["the_nether"]["logical_height"], 128);
}

#[test]
fn the_world_border_is_javas_default() {
    let b = &fixture()["border"];
    assert_eq!(
        BORDER,
        WorldBorder {
            min_x: b["min_x"].as_f64().unwrap(),
            max_x: b["max_x"].as_f64().unwrap(),
            min_z: b["min_z"].as_f64().unwrap(),
            max_z: b["max_z"].as_f64().unwrap(),
        }
    );
    for case in fixture()["within"].as_array().unwrap() {
        let p = IVec3::new(case["x"].as_i64().unwrap() as i32, 64, case["z"].as_i64().unwrap() as i32);
        assert_eq!(BORDER.contains_block(p), case["within"].as_bool().unwrap(), "{p}");
    }
    for case in fixture()["clamp"].as_array().unwrap() {
        assert_eq!(BORDER.clamp(dvec(&case["at"])), ivec(&case["block"]), "{case}");
    }
}

#[test]
fn relative_positions_match_javas_portal_shape() {
    let cases = fixture()["relative"].as_array().unwrap();
    assert!(cases.len() >= 10);
    for case in cases {
        let [s1, s2] = [0, 1].map(|i| case["sizes"][i].as_i64().unwrap() as i32);
        let rect = PortalRect { min: ivec(&case["min_corner"]), width: s1, height: s2 };
        // Java's `EntityDimensions` are floats.
        let [w, h] = [0, 1].map(|i| f64::from(case["dimensions"][i].as_f64().unwrap() as f32));
        let along_x = case["axis"] == "x";
        let got = relative_position(rect, along_x, dvec(&case["position"]), w, h);
        let java = dvec(&case["relative"]);
        assert!((got - java).abs().max_element() < 1e-12, "{case}: {got}");
    }
}

/// The harness's shapes: cells (along, up) of a plane through (0, 64, 0).
fn shape(name: &str) -> (bool, Vec<(i32, i32)>, (i32, i32)) {
    let block = |w: i32, h: i32, x0: i32, y0: i32| -> Vec<(i32, i32)> {
        (0..w).flat_map(|i| (0..h).map(move |j| (x0 + i, y0 + j))).collect()
    };
    match name {
        "plain_x" => (true, block(2, 3, 0, 0), (1, 2)),
        "plain_z" => (false, block(2, 3, 0, 0), (0, 0)),
        "wide" => (true, block(6, 4, -2, 0), (0, 1)),
        "stacked" => (true, [block(2, 3, 0, 0), block(4, 2, -1, 3)].concat(), (0, 0)),
        "stacked_top" => (true, [block(2, 3, 0, 0), block(4, 2, -1, 3)].concat(), (2, 4)),
        "stepped" => (false, [block(1, 5, 0, 0), block(1, 3, 1, 1), block(3, 1, 2, 2)].concat(), (1, 2)),
        "huge" => (true, block(23, 23, 0, 0), (3, 4)),
        other => panic!("no shape {other}"),
    }
}

#[test]
fn portal_rectangles_match_javas_largest_rectangle() {
    let rects = fixture()["rectangles"].as_object().unwrap();
    assert_eq!(rects.len(), 7);
    for (name, java) in rects {
        let (along_x, cells, centre) = shape(name);
        let at = |a: i32, up: i32| if along_x { IVec3::new(a, 64 + up, 0) } else { IVec3::new(0, 64 + up, a) };
        let set: HashSet<IVec3> = cells.iter().map(|&(a, up)| at(a, up)).collect();
        let got = largest_rectangle_around(at(centre.0, centre.1), along_x, |p| set.contains(&p));
        let sizes = [0, 1].map(|i| java["sizes"][i].as_i64().unwrap() as i32);
        assert_eq!(got, PortalRect { min: ivec(&java["min_corner"]), width: sizes[0], height: sizes[1] }, "{name}");
    }
}

/// `createDimensionTransition` puts the traveller where
/// `getRelativePosition` measured them, in a portal of any size and axis.
#[test]
fn exits_put_travellers_where_they_stood() {
    let (w, h) = (f64::from(0.6f32), f64::from(1.8f32));
    for case in fixture()["relative"].as_array().unwrap() {
        let offset = dvec(&case["relative"]);
        for (rect, along_x) in [
            (PortalRect { min: IVec3::new(3, 70, -9), width: 2, height: 3 }, true),
            (PortalRect { min: IVec3::new(-40, 31, 12), width: 5, height: 4 }, false),
        ] {
            let at = exit_position(rect, along_x, offset, w, h);
            let back = relative_position(rect, along_x, at, w, h);
            assert!((back - offset).abs().max_element() < 1e-9, "{offset} in {rect:?}: {back}");
        }
    }
    // Java's numbers: the middle of a 2 x 3 portal along X, and along Z.
    let rect = PortalRect { min: IVec3::new(0, 64, 0), width: 2, height: 3 };
    assert_eq!(exit_position(rect, true, DVec3::new(0.5, 0.0, 0.0), 0.6, 1.8), DVec3::new(1.0, 64.0, 0.5));
    assert_eq!(exit_position(rect, false, DVec3::new(0.5, 0.0, 0.0), 0.6, 1.8), DVec3::new(0.5, 64.0, 1.0));
    // A traveller who entered at the left edge, standing 0.3 in front.
    let at = exit_position(rect, true, DVec3::new(0.0, 0.0, 0.3), 0.6, 1.8);
    assert!((at - DVec3::new(0.3, 64.0, 0.8)).abs().max_element() < 1e-12, "{at}");
}
