//! Location 作者 API 经真实 Boa、活动 State 与 RuntimeSession 的边界测试。

use super::support::{navigate, ready, text, with_runtime};
use narrava_loom_protocol::{HostUpdateDto, RuntimeCommand};

const TOWN: &str = r#"
Location.add({id:'town',name:'新手镇',bounds:[[0,0],[10,0],[10,10],[0,10]],entry:[2,3]});
"#;

fn probe(script: &str, entered: bool) -> serde_json::Value {
    let story: &str = if entered {
        ":: Start\n<<link [[进入|Town]]>><</link>>\n:: Town [town outside]\n<<probe>>\n"
    } else {
        ":: Start\n<<probe>>\n"
    };
    let mut observed: Option<serde_json::Value> = None;
    with_runtime(story, script, |runtime| {
        let start: HostUpdateDto = ready(runtime.execute(RuntimeCommand::Start).unwrap()).1;
        let frame: HostUpdateDto = if entered {
            navigate(runtime, &start, "Town").1
        } else {
            start
        };
        observed = Some(serde_json::from_str(&text(&frame)).expect("probe 应返回 JSON 文本"));
    });
    observed.expect("Runtime 应执行 probe")
}

#[test]
fn location_bridge_accepts_optional_names_and_forward_parent_registration() {
    let result: serde_json::Value = probe(
        r#"
Location.add({id:'clinic',name:'诊所',parent:'town',bounds:[[2,2],[4,2],[4,4],[2,4]]});
Location.add({id:'town',bounds:[[0,0],[10,0],[10,10],[0,10]]});
Macro.add('probe', {handler: () => JSON.stringify({
    name: Location.get('clinic').name,
    parent: Location.get('clinic').parent,
    unnamed: Location.get('town').name === undefined,
    missing: Location.get('missing') === undefined,
    located: Location.locate([3,3]).map(place => place.id),
    places: Location.places().map(place => place.id),
    current: Location.current(),
})});
"#,
        false,
    );
    assert_eq!(result["name"], "诊所");
    assert_eq!(result["parent"], "town");
    assert_eq!(result["unnamed"], true);
    assert_eq!(result["missing"], true);
    assert_eq!(result["located"], serde_json::json!(["town", "clinic"]));
    assert_eq!(result["places"].as_array().unwrap().len(), 2);
    assert_eq!(result["current"], serde_json::Value::Null);
}

#[test]
fn location_bridge_returns_detached_place_and_position_snapshots() {
    let script: String = format!(
        r#"{TOWN}
Macro.add('probe', {{handler: () => {{
    const place = Location.get('town');
    place.bounds[0][0] = 99;
    place.entry[0] = 99;
    place.name = 'changed';
    const places = Location.places();
    places[0].bounds[1][0] = 99;
    places.length = 0;
    const located = Location.locate([2,3]);
    located[0].id = 'changed';
    const current = Location.current();
    current.point[0] = 99;
    current.place = 'changed';
    current.environment = 'inside';
    const before = Location.current();
    const moved = Location.move([4,5]);
    moved.point[0] = 99;
    return JSON.stringify({{place: Location.get('town'), before, after: Location.current()}});
}}}});
"#
    );
    let result: serde_json::Value = probe(&script, true);
    assert_eq!(result["place"]["name"], "新手镇");
    assert_eq!(result["place"]["id"], "town");
    assert_eq!(result["place"]["bounds"][0], serde_json::json!([0, 0]));
    assert_eq!(result["place"]["bounds"][1], serde_json::json!([10, 0]));
    assert_eq!(result["place"]["entry"], serde_json::json!([2, 3]));
    assert_eq!(result["before"]["point"], serde_json::json!([2, 3]));
    assert_eq!(result["before"]["place"], "town");
    assert_eq!(result["before"]["environment"], "outside");
    assert_eq!(result["after"]["point"], serde_json::json!([4, 5]));
}

#[test]
fn location_bridge_preserves_safe_integer_coordinates_larger_than_i32() {
    let result: serde_json::Value = probe(
        r#"
Location.add({id:'town',bounds:[[3000000000,0],[3000000010,0],[3000000010,10],[3000000000,10]],entry:[3000000001,1]});
Macro.add('probe', {handler: () => JSON.stringify({
    entry: Location.current().point,
    moved: Location.move([3000000002,3]).point,
    located: Location.locate([3000000002,3]).map(place => place.id),
})});
"#,
        true,
    );
    assert_eq!(result["entry"], serde_json::json!([3_000_000_001_i64, 1]));
    assert_eq!(result["moved"], serde_json::json!([3_000_000_002_i64, 3]));
    assert_eq!(result["located"], serde_json::json!(["town"]));
}

#[test]
fn location_bridge_rejects_invalid_coordinates_and_unknown_definition_fields() {
    let script: String = format!(
        r#"{TOWN}
const rejected = [];
const points = [[0.5,1],[2 ** 53,1],[1],[1,2,3]];
function reject(operation) {{
    try {{ operation(); rejected.push(false); }}
    catch (error) {{ rejected.push(error instanceof TypeError); }}
}}
for (const [index, point] of points.entries()) {{
    reject(() => Location.add({{id:`bad-entry-${{index}}`,bounds:[[0,0],[10,0],[10,10],[0,10]],entry:point}}));
    reject(() => Location.add({{id:`bad-bound-${{index}}`,bounds:[[0,0],[10,0],point,[0,10]]}}));
    reject(() => Location.locate(point));
}}
reject(() => Location.add({{id:'unknown-field',bounds:[[0,0],[10,0],[10,10],[0,10]],unknown:true}}));
Macro.add('probe', {{handler: () => {{
    const before = Location.current();
    for (const point of points) reject(() => Location.move(point));
    return JSON.stringify({{rejected, count: Location.places().length, before, after: Location.current()}});
}}}});
"#
    );
    let result: serde_json::Value = probe(&script, true);
    assert_eq!(result["rejected"], serde_json::json!(vec![true; 17]));
    assert_eq!(result["count"], 1);
    assert_eq!(result["before"], result["after"]);
}

#[test]
fn location_bridge_seals_registration_after_initial_scripts() {
    let script: String = format!(
        r#"{TOWN}
Macro.add('probe', {{handler: () => {{
    let rejected = false;
    try {{ Location.add({{id:'late',bounds:[[0,0],[10,0],[10,10],[0,10]]}}); }}
    catch (error) {{ rejected = error instanceof TypeError; }}
    return JSON.stringify({{rejected, count: Location.places().length, missing: Location.get('late') === undefined}});
}}}});
"#
    );
    let result: serde_json::Value = probe(&script, false);
    assert_eq!(
        result,
        serde_json::json!({"rejected":true,"count":1,"missing":true})
    );
}

#[test]
fn location_bridge_failed_movement_preserves_absent_and_existing_positions() {
    let script: String = format!(
        r#"{TOWN}
Macro.add('probe', {{handler: () => {{
    const before = Location.current();
    let error = '';
    try {{ Location.move(before === null ? [2,3] : [20,30]); }}
    catch (failure) {{ error = failure.message; }}
    return JSON.stringify({{before, after: Location.current(), error}});
}}}});
"#
    );
    let absent: serde_json::Value = probe(&script, false);
    assert_eq!(absent["before"], serde_json::Value::Null);
    assert_eq!(absent["after"], serde_json::Value::Null);
    assert!(
        absent["error"]
            .as_str()
            .unwrap()
            .contains("location.no_position")
    );
    let entered: serde_json::Value = probe(&script, true);
    assert_eq!(entered["before"], entered["after"]);
    assert_eq!(entered["after"]["point"], serde_json::json!([2, 3]));
    assert!(
        entered["error"]
            .as_str()
            .unwrap()
            .contains("location.invalid_position")
    );
}
