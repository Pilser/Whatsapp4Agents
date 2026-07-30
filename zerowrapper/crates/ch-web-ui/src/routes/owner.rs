use topcoat::Result;
use topcoat::router::page;
use topcoat::view::view;

#[allow(unused_imports)]
use crate::components::layout::root_layout;
#[allow(unused_imports)]
use crate::components::owner_login::owner_login;

#[page("/owner")]
pub async fn owner() -> Result {
    view! { <root_layout><owner_login /></root_layout> }
}
