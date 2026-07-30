use topcoat::Result;
use topcoat::router::page;
use topcoat::view::view;

use crate::components::chat_app::chat_app;

#[page("/chat")]
pub async fn chat() -> Result {
    view! {
        chat_app()
    }
}
