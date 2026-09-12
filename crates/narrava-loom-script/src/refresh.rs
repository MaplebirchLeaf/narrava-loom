//! 当前页重放只重建交互，完成正文后恢复已提交状态；随机数使用独立重绘序列。

use std::cell::{Ref, RefCell};

use boa_engine::{Context, Finalize, JsData, Trace};
use narrava_loom_core::{
    location::LocationState,
    random::RandomState,
    state::{State, StateCheckpoint},
};

use crate::{EcmaBinding, EcmaRuntime};

#[derive(Trace, Finalize, JsData)]
struct RefreshSlot {
    #[unsafe_ignore_trace]
    view: RefCell<Option<RefreshView>>,
}

struct RefreshView {
    checkpoint: Option<StateCheckpoint>,
    location: LocationState,
    random: RandomState,
    entered: bool,
}

pub(super) fn install(context: &mut Context) {
    context.insert_data(RefreshSlot {
        view: RefCell::new(None),
    });
}

fn slot(context: &Context) -> &RefreshSlot {
    context
        .get_data::<RefreshSlot>()
        .expect("Refresh 视图必须在脚本装载前安装")
}

pub(super) fn is_refreshing(context: &Context) -> bool {
    slot(context).view.borrow().is_some()
}

impl EcmaBinding {
    pub(crate) fn set_refresh(&self, state: Option<&State>) {
        let runtime: Ref<'_, EcmaRuntime> = self.runtime.borrow();
        *slot(&runtime.context).view.borrow_mut() = state.map(|state: &State| RefreshView {
            checkpoint: Some(state.checkpoint()),
            location: state.location_state().clone(),
            random: state.random_state(),
            entered: false,
        });
    }

    /// 首次生命周期开始时 Core 已恢复入页快照；后续生命周期表示一次真正的新导航。
    pub(crate) fn enter_refresh(&self, state: &mut State) -> bool {
        let runtime: Ref<'_, EcmaRuntime> = self.runtime.borrow();
        let mut refresh = slot(&runtime.context).view.borrow_mut();
        if let Some(view) = refresh.as_mut() {
            if !view.entered {
                let before: RandomState = state.random_state();
                state.restore_random_state(view.random);
                state.begin_random_replay(before);
                *state.location_state_mut() = view.location.clone();
                view.entered = true;
                return true;
            }
            *refresh = None;
        }
        state.end_random_replay();
        false
    }

    /// 正文已重建交互，辅助区域与输入同步必须读取已提交状态；挂起恢复不重复覆盖。
    pub(crate) fn finish_refresh_body(&self, state: &mut State) {
        let runtime: Ref<'_, EcmaRuntime> = self.runtime.borrow();
        let mut refresh = slot(&runtime.context).view.borrow_mut();
        let Some(checkpoint) = refresh.as_mut().and_then(|view| view.checkpoint.take()) else {
            return;
        };
        let replay: Option<RandomState> = state.random_replay_state();
        state.restore_checkpoint(checkpoint);
        if let Some(random) = replay {
            state.begin_random_replay(random);
        }
    }
}
