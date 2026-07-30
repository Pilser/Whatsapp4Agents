use topcoat::Result;
use topcoat::router::page;
use topcoat::view::view;

#[allow(unused_imports)]
use crate::components::layout::root_layout;
#[allow(unused_imports)]
use crate::components::owner_chat::owner_chat;

#[page("/owner/dashboard")]
pub async fn owner_dashboard() -> Result {
    view! { <root_layout><owner_chat /></root_layout> }
}
