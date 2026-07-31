use topcoat::Result;
use topcoat::router::page;
use topcoat::view::view;
use crate::components::owner_login::owner_login;

#[page("/owner")]
pub async fn owner() -> Result {
    view! {
        owner_login()
    }
}
