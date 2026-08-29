//! A feature with no actor and nothing to ask one - the chrome hears about
//! the world through the bus and keeps a count.

use guinea::feature::{Feature, FeatureInitContext};
use guinea_core::feature::Bound;

use crate::events::ProcessKilled;

use super::contracts::{self, Chrome};

pub struct TabsFeature {
    _chrome: Bound<contracts::Tabs>,
}

impl Feature for TabsFeature {
    type Params = str;
    type Exports = (contracts::Tabs,);

    fn install(cx: &FeatureInitContext, context: &str) -> anyhow::Result<Self> {
        let chrome = cx.state::<contracts::Tabs>().plain();

        let count = cx
            .scope
            .peek::<contracts::Tabs>()
            .map_or(0, |tabs| tabs.borrow().install_count);
        chrome.push(Chrome::Installed(count + 1));
        chrome.push(Chrome::Reached(context.to_string()));

        let local = chrome.clone();
        cx.subscribe::<ProcessKilled>(move |ev: ProcessKilled| {
            local.push(Chrome::LocalKill(ev.name));
        });

        let global = chrome.clone();
        cx.subscribe_global::<ProcessKilled>(move |_| {
            global.push(Chrome::GlobalKill);
        });

        Ok(Self { _chrome: chrome })
    }
}
