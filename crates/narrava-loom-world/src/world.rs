use std::collections::{HashMap, HashSet};

use crate::geometry::{contains, contains_polygon, valid_point, validate_polygon};
use crate::{Environment, Place, Point, WorldError, WorldPosition, WorldState};

/// 地点注册表；玩家位置由调用方纳入事务管理。
#[derive(Clone, Debug, Default)]
pub struct World {
    places: HashMap<String, Place>,
    ordered_ids: Vec<String>,
}

impl World {
    /// 注册地点；前向父引用留到 `validate` 统一检查。
    pub fn add(&mut self, place: Place) -> Result<(), WorldError> {
        validate_id(&place.id)?;
        if self.places.contains_key(&place.id) {
            return Err(WorldError::new(
                "world.duplicate_place",
                format!("place '{}' is already registered", place.id),
            ));
        }
        if let Some(parent) = &place.parent {
            validate_id(parent)?;
        }
        validate_polygon(&place.bounds).map_err(|error: WorldError| {
            WorldError::new(error.code(), format!("place '{}': {error}", place.id))
        })?;
        if let Some(entry) = place.entry
            && !contains(&place.bounds, entry)
        {
            return Err(WorldError::new(
                "world.invalid_entry",
                format!("entry of place '{}' is outside its bounds", place.id),
            ));
        }
        let index: usize = self
            .ordered_ids
            .partition_point(|id: &String| id < &place.id);
        self.ordered_ids.insert(index, place.id.clone());
        self.places.insert(place.id.clone(), place);
        Ok(())
    }

    /// 按区分大小写的 ID 查询，显示名不参与匹配。
    pub fn get(&self, id: &str) -> Option<&Place> {
        self.places.get(id)
    }

    /// 按地点 ID 排序遍历，保证顺序稳定。
    pub fn places(&self) -> impl ExactSizeIterator<Item = &Place> {
        self.ordered_ids.iter().map(|id: &String| &self.places[id])
    }

    /// 游戏启动前检查完整父子关系及区域包含关系。
    pub fn validate(&self) -> Result<(), WorldError> {
        for place in self.places() {
            self.ancestors(&place.id)?;
            if let Some(parent_id) = &place.parent {
                let parent: &Place = self.require(parent_id)?;
                if !contains_polygon(&parent.bounds, &place.bounds) {
                    return Err(WorldError::new(
                        "world.outside_parent",
                        format!("place '{}' extends outside parent '{parent_id}'", place.id),
                    ));
                }
            }
        }
        Ok(())
    }

    /// 返回包含此点的全部地点（含边界），按深度、ID 排序。
    pub fn locate(&self, point: Point) -> Vec<&Place> {
        if !valid_point(point) {
            return Vec::new();
        }
        let mut located: Vec<(&Place, usize)> = self
            .places()
            .filter(|place: &&Place| contains(&place.bounds, point))
            .map(|place: &Place| {
                let depth: usize = self
                    .ancestors(&place.id)
                    .map(|ancestors: Vec<&Place>| ancestors.len())
                    .unwrap_or(usize::MAX);
                (place, depth)
            })
            .collect();
        located.sort_by(|(left, left_depth), (right, right_depth)| {
            left_depth
                .cmp(right_depth)
                .then_with(|| left.id.cmp(&right.id))
        });
        located.into_iter().map(|(place, _)| place).collect()
    }

    /// 返回根地点到自身的链路，拒绝缺失父地点和循环引用。
    pub fn ancestors(&self, id: &str) -> Result<Vec<&Place>, WorldError> {
        let mut chain: Vec<&Place> = Vec::new();
        let mut visited: HashSet<&str> = HashSet::new();
        let mut current: &Place = self.require(id)?;
        loop {
            if !visited.insert(current.id.as_str()) {
                return Err(WorldError::new(
                    "world.parent_cycle",
                    format!("parent cycle includes place '{}'", current.id),
                ));
            }
            chain.push(current);
            let Some(parent) = &current.parent else {
                break;
            };
            current = self.require(parent)?;
        }
        chain.reverse();
        Ok(chain)
    }

    /// 进入入口或首顶点；重入同一地点保留坐标。
    /// 省略环境时，同地点保留原环境，切换地点则清空。
    pub fn enter(
        &self,
        state: &mut WorldState,
        id: &str,
        environment: Option<Environment>,
    ) -> Result<bool, WorldError> {
        let place: &Place = self.require(id)?;
        if let Some(position) = state.position.as_ref()
            && position.place == id
        {
            self.validate_state(state)?;
            let next_environment: Option<Environment> = environment.or(position.environment);
            let changed: bool = next_environment != position.environment;
            if changed {
                state
                    .position
                    .as_mut()
                    .expect("position was checked")
                    .environment = next_environment;
            }
            return Ok(changed);
        }
        state.position = Some(WorldPosition {
            place: id.to_owned(),
            point: place.entry.unwrap_or(place.bounds[0]),
            environment,
        });
        Ok(true)
    }

    /// 在当前地点内移动，失败时不修改状态。
    pub fn move_to(&self, state: &mut WorldState, point: Point) -> Result<bool, WorldError> {
        let position: &mut WorldPosition = state.position.as_mut().ok_or_else(|| {
            WorldError::new("world.no_position", "cannot move before entering a place")
        })?;
        let place: &Place = self.require(&position.place)?;
        if !contains(&place.bounds, point) {
            return Err(WorldError::new(
                "world.invalid_position",
                format!(
                    "point is outside place '{}' or the safe integer range",
                    place.id
                ),
            ));
        }
        let changed: bool = position.point != point;
        position.point = point;
        Ok(changed)
    }

    /// 恢复存档前，按当前地点定义校验玩家位置。
    pub fn validate_state(&self, state: &WorldState) -> Result<(), WorldError> {
        if let Some(position) = &state.position {
            let place: &Place = self.require(&position.place)?;
            if !contains(&place.bounds, position.point) {
                return Err(WorldError::new(
                    "world.invalid_position",
                    format!(
                        "saved point is outside place '{}' or the safe integer range",
                        place.id
                    ),
                ));
            }
        }
        Ok(())
    }

    fn require(&self, id: &str) -> Result<&Place, WorldError> {
        self.get(id).ok_or_else(|| {
            WorldError::new(
                "world.unknown_place",
                format!("place '{id}' is not registered"),
            )
        })
    }
}

fn validate_id(id: &str) -> Result<(), WorldError> {
    let mut bytes: std::str::Bytes<'_> = id.bytes();
    let valid: bool = bytes
        .next()
        .is_some_and(|byte: u8| byte.is_ascii_alphabetic())
        && bytes.all(|byte: u8| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'));
    if !valid {
        return Err(WorldError::new(
            "world.invalid_id",
            format!(
                "place ID '{id}' must start with an ASCII letter and contain only letters, digits, '_', '-', or '.'"
            ),
        ));
    }
    if matches!(
        id,
        "inside"
            | "outside"
            | "exit"
            | "Start"
            | "StoryInit"
            | "Header"
            | "Footer"
            | "Bar"
            | "BarStowed"
    ) {
        return Err(WorldError::new(
            "world.reserved_id",
            format!("place ID '{id}' is reserved"),
        ));
    }
    Ok(())
}
