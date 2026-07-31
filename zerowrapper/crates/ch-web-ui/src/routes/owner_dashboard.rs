use topcoat::Result;
use topcoat::router::page;
use topcoat::view::view;
use crate::components::owner_chat::owner_chat;

#[page("/owner/dashboard")]
pub async fn owner_dashboard() -> Result {
    view! {
        owner_chat()
    }
}
