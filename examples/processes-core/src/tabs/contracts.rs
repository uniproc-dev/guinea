use guinea_core::scope::Reducer;

#[derive(Default, Clone, PartialEq, Debug)]
pub struct Tabs {
    /// What the layout was reached with - `routes!` derives it as the
    /// intersection of the pages under it, so the tab strip navigates to a
    /// sibling with the context it is already in rather than inventing one.
    pub context: String,
    pub install_count: i32,
    pub kills_this_window: i32,
    pub kills_all_windows: i32,
    pub last_killed: Option<String>,
}

#[derive(Clone)]
pub enum Chrome {
    Reached(String),
    Installed(i32),
    LocalKill(String),
    GlobalKill,
}

impl Reducer for Tabs {
    type Update = Chrome;

    fn reduce(&mut self, update: Chrome) {
        match update {
            Chrome::Reached(context) => self.context = context,
            Chrome::Installed(n) => self.install_count = n,
            Chrome::LocalKill(name) => {
                self.kills_this_window += 1;
                self.last_killed = Some(name);
            }
            Chrome::GlobalKill => self.kills_all_windows += 1,
        }
    }
}
