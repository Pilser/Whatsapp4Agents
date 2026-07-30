use topcoat::Result;
use topcoat::router::page;
use topcoat::view::view;

use crate::components::hero::hero;
use crate::components::faq::faq;

#[page("/")]
pub async fn home() -> Result {
    view! {
        hero()
        faq()
    }
}
