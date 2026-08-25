use guinea_macros::{ReducerState, reducer};

#[derive(ReducerState)]
pub struct TabsViewState {
    pub install_count: i32,
    pub kills_this_window: i32,
    pub kills_all_windows: i32,
    pub last_killed: Option<String>,
}

pub enum TabsMsg {
    Installed(i32),
    LocalKill(String),
    GlobalKill,
}

#[reducer]
pub fn tabs_reducer(state: &mut TabsViewState, msg: TabsMsg) {
    match msg {
        TabsMsg::Installed(n) => state.install_count = n,
        TabsMsg::LocalKill(name) => {
            state.kills_this_window += 1;
            state.last_killed = Some(name);
        }
        TabsMsg::GlobalKill => state.kills_all_windows += 1,
    }
}
